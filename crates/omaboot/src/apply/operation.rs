//! The unit of work.
//!
//! Every side effect the pipeline can have is one `Operation`. A dry run
//! prints them; a real run performs them. There is no second code path, so the
//! list `--dry-run` shows is the list that would actually be carried out.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::exec::{CommandSpec, Runner};
use crate::hash::sha256_hex;
use crate::paths::guard_destination;
use crate::state;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    /// A read-only assertion, shown so the plan reads as a whole.
    Check {
        what: String,
    },
    MakeDir {
        path: PathBuf,
    },
    RemoveDirAll {
        path: PathBuf,
    },
    RemoveFile {
        path: PathBuf,
    },
    Write {
        path: PathBuf,
        bytes: Vec<u8>,
        origin: String,
    },
    Copy {
        from: PathBuf,
        to: PathBuf,
        bytes: u64,
    },
    Run {
        spec: CommandSpec,
        purpose: String,
    },
    /// Run the staged SDDM theme through the greeter in test mode, offscreen,
    /// and require it to stay up for the settling time without complaining
    /// about the theme (`crate::greeter`). This is the gate before every
    /// privileged step, and no plan may skip it.
    SmokeTestGreeter {
        spec: CommandSpec,
        theme_dir: PathBuf,
    },
    /// Ask the helper which protocol it speaks, unprivileged, before the
    /// first privileged step. A helper built from older source writes other
    /// paths than this engine verifies; the first real apply found that out
    /// at step 10 (A3). It is refused here instead.
    CheckHelper {
        helper: PathBuf,
    },
    /// Set the default Plymouth theme. On a real system this runs
    /// `plymouth-set-default-theme`; under a prefix it rewrites the prefixed
    /// `plymouthd.conf`, because the real tool must never run in a test.
    SetPlymouthDefault {
        theme: String,
        tool: Option<PathBuf>,
        conf: PathBuf,
    },
    /// Re-read an installed file and compare it with what was staged.
    VerifyFile {
        path: PathBuf,
        sha256: String,
        bytes: u64,
    },
    /// Resolve the login theme the way SDDM does, across every configuration
    /// file, and require that it is the expected one. Writing the drop-in is
    /// not enough: another drop-in that sorts later would win silently.
    VerifySddmTheme {
        conf: PathBuf,
        conf_dir: PathBuf,
        expected: String,
    },
}

impl Operation {
    pub fn write(path: impl Into<PathBuf>, bytes: Vec<u8>, origin: impl Into<String>) -> Self {
        Self::Write {
            path: path.into(),
            bytes,
            origin: origin.into(),
        }
    }

    pub fn check(what: impl Into<String>) -> Self {
        Self::Check { what: what.into() }
    }

    pub fn run(spec: CommandSpec, purpose: impl Into<String>) -> Self {
        Self::Run {
            spec,
            purpose: purpose.into(),
        }
    }

    /// The destination this operation writes to, if it writes anywhere.
    pub fn destination(&self) -> Option<&Path> {
        match self {
            Self::MakeDir { path }
            | Self::RemoveDirAll { path }
            | Self::RemoveFile { path }
            | Self::Write { path, .. } => Some(path),
            Self::Copy { to, .. } => Some(to),
            Self::SetPlymouthDefault { conf, .. } => Some(conf),
            Self::Check { .. }
            | Self::Run { .. }
            | Self::SmokeTestGreeter { .. }
            | Self::CheckHelper { .. }
            | Self::VerifyFile { .. }
            | Self::VerifySddmTheme { .. } => None,
        }
    }
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Check { what } => write!(f, "check    {what}"),
            Self::MakeDir { path } => write!(f, "mkdir    {}", path.display()),
            Self::RemoveDirAll { path } => write!(f, "rm -r    {}", path.display()),
            Self::RemoveFile { path } => write!(f, "rm       {}", path.display()),
            Self::Write {
                path,
                bytes,
                origin,
            } => write!(
                f,
                "write    {} ({} bytes, sha256 {}, {origin})",
                path.display(),
                bytes.len(),
                short_hash(&sha256_hex(bytes))
            ),
            Self::Copy { from, to, bytes } => write!(
                f,
                "copy     {} -> {} ({bytes} bytes)",
                from.display(),
                to.display()
            ),
            Self::Run { spec, purpose } => write!(f, "run      {spec}    # {purpose}"),
            Self::CheckHelper { helper } => write!(
                f,
                "check    {} protocol    # must answer {}, the number this omaboot was built with",
                helper.display(),
                crate::helper_protocol::PROTOCOL
            ),
            Self::SmokeTestGreeter { spec, .. } => write!(
                f,
                "run      {spec}    # the greeter smoke test: it has to stay up for {} seconds without complaining about the theme, then it is stopped",
                crate::greeter::SETTLE.as_secs()
            ),
            Self::SetPlymouthDefault { theme, tool, conf } => match tool {
                Some(tool) => write!(f, "run      sudo {} {theme}", tool.display()),
                None => write!(f, "write    {} (Theme={theme})", conf.display()),
            },
            Self::VerifyFile {
                path,
                sha256,
                bytes,
            } => write!(
                f,
                "verify   {} ({bytes} bytes, sha256 {})",
                path.display(),
                short_hash(sha256)
            ),
            Self::VerifySddmTheme {
                conf,
                conf_dir,
                expected,
            } => write!(
                f,
                "verify   {} and {}/*.conf resolve [Theme] Current= to {expected}",
                conf.display(),
                conf_dir.display()
            ),
        }
    }
}

fn short_hash(hash: &str) -> String {
    hash.chars().take(12).collect()
}

/// Performs operations, or does not.
#[derive(Debug)]
pub struct Executor<'a> {
    runner: &'a dyn Runner,
    dry_run: bool,
}

impl<'a> Executor<'a> {
    pub fn new(runner: &'a dyn Runner, dry_run: bool) -> Self {
        Self { runner, dry_run }
    }

    pub fn dry_run(&self) -> bool {
        self.dry_run
    }

    /// Carry out one operation, unless this is a dry run.
    ///
    /// The destination guard runs even in a dry run, so a plan that would
    /// touch a directory Omarchy owns fails before it is ever printed as if it
    /// were acceptable.
    pub fn perform(&self, operation: &Operation) -> Result<()> {
        if let Some(destination) = operation.destination() {
            guard_destination(destination)?;
        }
        if self.dry_run {
            return Ok(());
        }

        match operation {
            Operation::Check { .. } => Ok(()),
            Operation::MakeDir { path } => {
                fs::create_dir_all(path).map_err(|source| Error::write(path.clone(), source))
            }
            Operation::RemoveDirAll { path } => match fs::remove_dir_all(path) {
                Ok(()) => Ok(()),
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(source) => Err(Error::write(path.clone(), source)),
            },
            Operation::RemoveFile { path } => match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(source) => Err(Error::write(path.clone(), source)),
            },
            Operation::Write { path, bytes, .. } => {
                refuse_symlink(path)?;
                state::write_atomic(path, bytes)
            }
            Operation::Copy { from, to, .. } => {
                refuse_symlink(to)?;
                let bytes = fs::read(from).map_err(|source| Error::read(from.clone(), source))?;
                state::write_atomic(to, &bytes)
            }
            Operation::Run { spec, purpose } => {
                let output = self.runner.run(spec)?;
                if output.is_success() {
                    return Ok(());
                }
                Err(Error::Command {
                    command: spec.to_string(),
                    code: output.describe_code(),
                    stderr: first_lines(&output.stderr, 5),
                    suggestion: format!(
                        "{purpose} did not succeed; read the message above and fix the cause, then run the step again"
                    ),
                })
            }
            Operation::CheckHelper { helper } => {
                let want = crate::helper_protocol::PROTOCOL;
                let spec = CommandSpec::new(helper.display().to_string()).arg("protocol");
                let output = self.runner.run(&spec)?;
                let answer = output.stdout.trim();
                let suggestion = "the helper was built from older source than this omaboot; run `cargo build --release` in the omaboot checkout (it builds both binaries), then `omaboot plugin install`, and apply again".to_string();
                if !output.is_success() {
                    return Err(Error::Environment {
                        what: format!(
                            "the helper {} does not answer `protocol` ({}: {})",
                            helper.display(),
                            output.describe_code(),
                            first_lines(&output.stderr, 2)
                        ),
                        suggestion,
                    });
                }
                match answer.parse::<u32>() {
                    Ok(got) if got == want => Ok(()),
                    Ok(got) => Err(Error::Environment {
                        what: format!(
                            "the helper {} speaks protocol {got} and this omaboot needs {want}",
                            helper.display()
                        ),
                        suggestion,
                    }),
                    Err(_) => Err(Error::Environment {
                        what: format!(
                            "the helper {} answered `protocol` with {answer:?} instead of a number",
                            helper.display()
                        ),
                        suggestion,
                    }),
                }
            }
            Operation::SmokeTestGreeter { spec, theme_dir } => {
                let output = self.runner.run(spec)?;
                crate::greeter::judge(&output, theme_dir).map_err(|refusal| {
                    Error::GreeterRefused {
                        command: spec.to_string(),
                        what: refusal.what,
                        said: refusal.said,
                        suggestion: "the greeter refused this theme, so it is not installed and login is untouched; fix what the greeter names, then run the step again".to_string(),
                    }
                })
            }
            Operation::SetPlymouthDefault { theme, tool, conf } => match tool {
                Some(tool) => {
                    let spec = CommandSpec::sudo(tool.display().to_string()).arg(theme);
                    let output = self.runner.run(&spec)?;
                    if output.is_success() {
                        Ok(())
                    } else {
                        Err(Error::Command {
                            command: spec.to_string(),
                            code: output.describe_code(),
                            stderr: first_lines(&output.stderr, 5),
                            suggestion:
                                "the default Plymouth theme was not changed; the system is unchanged"
                                    .to_string(),
                        })
                    }
                }
                None => {
                    refuse_symlink(conf)?;
                    let existing = fs::read_to_string(conf).unwrap_or_default();
                    let updated = state::set_plymouthd_theme(&existing, theme);
                    state::write_atomic(conf, updated.as_bytes())
                }
            },
            Operation::VerifyFile {
                path,
                sha256,
                bytes,
            } => {
                let actual = fs::read(path).map_err(|source| Error::read(path.clone(), source))?;
                if actual.len() as u64 != *bytes {
                    return Err(Error::Drift {
                        path: path.clone(),
                        what: format!("it is {} bytes, expected {bytes}", actual.len()),
                    });
                }
                let actual_hash = sha256_hex(&actual);
                if &actual_hash != sha256 {
                    return Err(Error::Drift {
                        path: path.clone(),
                        what: format!(
                            "its contents hash to {} and the staged file hashes to {sha256}",
                            actual_hash
                        ),
                    });
                }
                Ok(())
            }
            Operation::VerifySddmTheme {
                conf,
                conf_dir,
                expected,
            } => match state::resolve_sddm_theme(conf, conf_dir) {
                Some(theme) if &theme.name == expected => Ok(()),
                Some(theme) => Err(Error::Drift {
                    path: theme.decided_by.clone(),
                    what: format!(
                        "it sets the login theme to {} after omaboot's drop-in, so SDDM would use that instead of {expected}; rename or remove it",
                        theme.name
                    ),
                }),
                None => Err(Error::Drift {
                    path: conf_dir.clone(),
                    what: format!(
                        "no SDDM configuration names a login theme, so {expected} is not in effect"
                    ),
                }),
            },
        }
    }
}

/// A destination that is a symlink is refused rather than followed, the same
/// way `omarchy-plymouth-set` refuses one.
fn refuse_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(Error::step(
            "install",
            format!("{} is a symlink", path.display()),
            "remove the symlink; omaboot never writes through one",
        )),
        _ => Ok(()),
    }
}

fn first_lines(text: &str, count: usize) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "(no output)".to_string();
    }
    trimmed.lines().take(count).collect::<Vec<_>>().join(" / ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::CommandOutput;
    use crate::exec::testing::RecordingRunner;

    #[test]
    fn a_dry_run_performs_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let runner = RecordingRunner::new();
        let executor = Executor::new(&runner, true);
        let target = tmp.path().join("file");
        executor
            .perform(&Operation::write(&target, b"bytes".to_vec(), "generated"))
            .unwrap();
        executor
            .perform(&Operation::run(CommandSpec::new("mkinitcpio"), "rebuild"))
            .unwrap();
        assert!(!target.exists());
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn a_write_into_an_omarchy_directory_is_refused_even_in_a_dry_run() {
        let runner = RecordingRunner::new();
        for dry_run in [true, false] {
            let executor = Executor::new(&runner, dry_run);
            let error = executor
                .perform(&Operation::write(
                    "/usr/share/plymouth/themes/omarchy/logo.png",
                    b"x".to_vec(),
                    "generated",
                ))
                .unwrap_err();
            assert!(error.to_string().contains("owned by Omarchy"), "{error}");
        }
    }

    #[test]
    fn a_failing_command_reports_its_stderr_and_a_next_step() {
        let runner = RecordingRunner::new().fail_containing(
            "mkinitcpio",
            CommandOutput::failure(1, "==> ERROR: no hook\nsecond"),
        );
        let executor = Executor::new(&runner, false);
        let error = executor
            .perform(&Operation::run(
                CommandSpec::new("mkinitcpio").arg("-P"),
                "rebuilding the initramfs",
            ))
            .unwrap_err()
            .to_string();
        assert!(error.contains("exit code 1"), "{error}");
        assert!(error.contains("no hook"), "{error}");
        assert!(error.contains("rebuilding the initramfs"), "{error}");
    }

    #[test]
    fn verify_reports_a_content_mismatch_as_drift() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("installed");
        fs::write(&path, b"changed").unwrap();
        let runner = RecordingRunner::new();
        let error = Executor::new(&runner, false)
            .perform(&Operation::VerifyFile {
                path: path.clone(),
                sha256: sha256_hex(b"staged"),
                bytes: 7,
            })
            .unwrap_err()
            .to_string();
        assert!(error.contains("hash"), "{error}");
    }

    #[test]
    fn verify_reports_a_size_mismatch_before_hashing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("installed");
        fs::write(&path, b"short").unwrap();
        let runner = RecordingRunner::new();
        let error = Executor::new(&runner, false)
            .perform(&Operation::VerifyFile {
                path,
                sha256: sha256_hex(b"much longer content"),
                bytes: 19,
            })
            .unwrap_err()
            .to_string();
        assert!(error.contains("5 bytes, expected 19"), "{error}");
    }

    #[test]
    fn writing_through_a_symlink_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let victim = tmp.path().join("victim");
        fs::write(&victim, b"original").unwrap();
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&victim, &link).unwrap();

        let runner = RecordingRunner::new();
        let error = Executor::new(&runner, false)
            .perform(&Operation::write(&link, b"attack".to_vec(), "generated"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("symlink"), "{error}");
        assert_eq!(fs::read(&victim).unwrap(), b"original");
    }

    #[test]
    fn the_printed_form_states_path_size_and_digest() {
        let printed =
            Operation::write("/tmp/x/omaboot.script", b"abc".to_vec(), "generated").to_string();
        assert!(
            printed.starts_with("write    /tmp/x/omaboot.script (3 bytes, sha256 ba7816bf8f01"),
            "{printed}"
        );
    }

    #[test]
    fn setting_the_theme_under_a_prefix_rewrites_the_conf_instead_of_running_the_tool() {
        let tmp = tempfile::tempdir().unwrap();
        let conf = tmp.path().join("etc/plymouth/plymouthd.conf");
        fs::create_dir_all(conf.parent().unwrap()).unwrap();
        fs::write(&conf, "[Daemon]\nTheme=omarchy\n").unwrap();

        let runner = RecordingRunner::new();
        Executor::new(&runner, false)
            .perform(&Operation::SetPlymouthDefault {
                theme: "omaboot".to_string(),
                tool: None,
                conf: conf.clone(),
            })
            .unwrap();

        assert!(
            runner.calls().is_empty(),
            "no command may run under a prefix"
        );
        assert!(fs::read_to_string(&conf).unwrap().contains("Theme=omaboot"));
    }
}
