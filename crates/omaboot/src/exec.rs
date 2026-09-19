//! Running external commands, and finding them first.
//!
//! Nothing in the pipeline reaches `std::process` directly. Every command goes
//! through a `Runner`, so a dry run can describe a command it did not run and
//! a test can script a failure without a system to fail on.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::paths::Layout;

/// One command, described well enough to print it exactly as it would run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub timeout: Option<Duration>,
}

impl CommandSpec {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            timeout: None,
        }
    }

    /// A command run through sudo: `sudo <program>`, or `sudo -n <program>`
    /// once the engine holds a ticket it acquired without a terminal.
    pub fn sudo(program: impl Into<String>) -> Self {
        let mut args = crate::auth::sudo_args();
        let sudo = args.remove(0);
        Self::new(sudo).args(args).arg(program)
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

impl fmt::Display for CommandSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (key, value) in &self.env {
            write!(f, "{key}={value} ")?;
        }
        write!(f, "{}", self.program)?;
        for arg in &self.args {
            write!(f, " {arg}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

impl CommandOutput {
    pub fn success() -> Self {
        Self {
            code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            timed_out: false,
        }
    }

    pub fn failure(code: i32, stderr: impl Into<String>) -> Self {
        Self {
            code: Some(code),
            stdout: String::new(),
            stderr: stderr.into(),
            timed_out: false,
        }
    }

    pub fn timed_out() -> Self {
        Self {
            code: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: true,
        }
    }

    pub fn is_success(&self) -> bool {
        !self.timed_out && self.code == Some(0)
    }

    pub fn describe_code(&self) -> String {
        if self.timed_out {
            "a timeout".to_string()
        } else {
            match self.code {
                Some(code) => format!("exit code {code}"),
                None => "a signal".to_string(),
            }
        }
    }
}

pub trait Runner: fmt::Debug {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput>;
}

/// Runs commands for real. Only reachable when there is no `--root` prefix and
/// the run is not a dry run.
#[derive(Debug, Default)]
pub struct RealRunner;

impl Runner for RealRunner {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput> {
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in &spec.env {
            command.env(key, value);
        }

        let Some(timeout) = spec.timeout else {
            let output = command.output().map_err(|source| Error::Command {
                command: spec.to_string(),
                code: "no exit code".to_string(),
                stderr: source.to_string(),
                suggestion: "check that the command exists and is executable".to_string(),
            })?;
            return Ok(CommandOutput {
                code: output.status.code(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                timed_out: false,
            });
        };

        let mut child = command.spawn().map_err(|source| Error::Command {
            command: spec.to_string(),
            code: "not started".to_string(),
            stderr: source.to_string(),
            suggestion: "check that the command exists and is executable".to_string(),
        })?;

        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_status)) => break,
                Ok(None) => {
                    if started.elapsed() >= timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Ok(CommandOutput::timed_out());
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(source) => {
                    return Err(Error::Command {
                        command: spec.to_string(),
                        code: "unknown".to_string(),
                        stderr: source.to_string(),
                        suggestion: "retry, and report this if it keeps happening".to_string(),
                    });
                }
            }
        }

        let output = child.wait_with_output().map_err(|source| Error::Command {
            command: spec.to_string(),
            code: "unknown".to_string(),
            stderr: source.to_string(),
            suggestion: "retry, and report this if it keeps happening".to_string(),
        })?;
        Ok(CommandOutput {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            timed_out: false,
        })
    }
}

/// A runner for prefixed runs: it executes nothing and reports success.
///
/// Rebuilding an initramfs or setting a default theme against a prefix is
/// meaningless, and doing it for real would be dangerous, so `--root` gets
/// this and never touches the system.
#[derive(Debug, Default)]
pub struct SimulatedRunner;

impl Runner for SimulatedRunner {
    fn run(&self, _spec: &CommandSpec) -> Result<CommandOutput> {
        Ok(CommandOutput::success())
    }
}

/// Which greeter binary this system has. Qt 6 renamed it, and Omarchy's own
/// theme declares `QtVersion=6`, so the Qt 6 binary is preferred when both
/// exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Greeter {
    pub path: PathBuf,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitramfsKind {
    /// `limine-mkinitcpio`, which is what Omarchy uses when it is installed.
    Limine,
    /// `mkinitcpio -P`.
    Mkinitcpio,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Initramfs {
    pub path: PathBuf,
    pub kind: InitramfsKind,
}

impl Initramfs {
    pub fn command(&self) -> CommandSpec {
        let spec = CommandSpec::sudo(self.path.display().to_string());
        match self.kind {
            InitramfsKind::Limine => spec,
            InitramfsKind::Mkinitcpio => spec.arg("-P"),
        }
    }
}

/// External tools, detected at runtime. Nothing here is hard coded to a name
/// that might not exist on the user's machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tools {
    pub greeter: Option<Greeter>,
    pub initramfs: Option<Initramfs>,
    pub plymouth_set_default: Option<PathBuf>,
    pub helper: Option<PathBuf>,
}

impl Tools {
    pub fn detect(layout: &Layout) -> Self {
        let greeter = ["sddm-greeter-qt6", "sddm-greeter"]
            .into_iter()
            .find_map(|name| {
                which(layout, name).map(|path| Greeter {
                    path,
                    name: name.to_string(),
                })
            });
        let initramfs = which(layout, "limine-mkinitcpio")
            .map(|path| Initramfs {
                path,
                kind: InitramfsKind::Limine,
            })
            .or_else(|| {
                which(layout, "mkinitcpio").map(|path| Initramfs {
                    path,
                    kind: InitramfsKind::Mkinitcpio,
                })
            });
        Self {
            greeter,
            initramfs,
            plymouth_set_default: which(layout, "plymouth-set-default-theme"),
            helper: which(layout, "omaboot-apply"),
        }
    }

    pub fn require_greeter(&self) -> Result<&Greeter> {
        self.greeter.as_ref().ok_or_else(|| Error::ToolMissing {
            tool: "sddm-greeter-qt6 or sddm-greeter".to_string(),
            suggestion:
                "install sddm; the greeter is smoke tested before it can ever be shown at login"
                    .to_string(),
        })
    }

    pub fn require_initramfs(&self) -> Result<&Initramfs> {
        self.initramfs.as_ref().ok_or_else(|| Error::ToolMissing {
            tool: "limine-mkinitcpio or mkinitcpio".to_string(),
            suggestion: "install mkinitcpio; without it the new theme cannot reach the initramfs"
                .to_string(),
        })
    }

    pub fn require_plymouth_set_default(&self) -> Result<&Path> {
        self.plymouth_set_default
            .as_deref()
            .ok_or_else(|| Error::ToolMissing {
                tool: "plymouth-set-default-theme".to_string(),
                suggestion: "install plymouth".to_string(),
            })
    }

    pub fn require_helper(&self) -> Result<&Path> {
        self.helper.as_deref().ok_or_else(|| Error::ToolMissing {
            tool: "omaboot-apply".to_string(),
            suggestion: "install the omaboot package; the privileged helper ships beside omaboot"
                .to_string(),
        })
    }
}

/// Find an executable. Under a prefix only the prefixed system directories are
/// searched, so a test never finds the developer's own binaries.
pub fn which(layout: &Layout, name: &str) -> Option<PathBuf> {
    if layout.is_prefixed() {
        return ["/usr/bin", "/usr/local/bin", "/bin"]
            .into_iter()
            .map(|dir| layout.system(dir).join(name))
            .find(|candidate| candidate.is_file());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
pub(crate) mod testing {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;

    /// A runner that records what it was asked to do and answers from a script.
    ///
    /// Any command without a scripted answer succeeds silently, so a test only
    /// states the failure it cares about.
    #[derive(Debug, Default)]
    pub struct RecordingRunner {
        calls: RefCell<Vec<String>>,
        answers: RefCell<HashMap<String, CommandOutput>>,
        once: RefCell<HashMap<String, CommandOutput>>,
    }

    impl RecordingRunner {
        pub fn new() -> Self {
            Self::default()
        }

        /// Fail every command whose printed form contains `needle`.
        pub fn fail_containing(self, needle: &str, output: CommandOutput) -> Self {
            self.answers.borrow_mut().insert(needle.to_string(), output);
            self
        }

        /// Fail the first matching command, then let it succeed. This is how a
        /// transient failure is scripted, so a test can watch the revert that
        /// follows it succeed.
        pub fn fail_once_containing(self, needle: &str, output: CommandOutput) -> Self {
            self.once.borrow_mut().insert(needle.to_string(), output);
            self
        }

        pub fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }

        pub fn ran_containing(&self, needle: &str) -> bool {
            self.calls.borrow().iter().any(|call| call.contains(needle))
        }
    }

    impl Runner for RecordingRunner {
        fn run(&self, spec: &CommandSpec) -> Result<CommandOutput> {
            let printed = spec.to_string();
            self.calls.borrow_mut().push(printed.clone());
            let once_match = self
                .once
                .borrow()
                .iter()
                .find(|(needle, _)| printed.contains(needle.as_str()))
                .map(|(needle, answer)| (needle.clone(), answer.clone()));
            if let Some((needle, answer)) = once_match {
                self.once.borrow_mut().remove(&needle);
                return Ok(answer);
            }
            for (needle, answer) in self.answers.borrow().iter() {
                if printed.contains(needle) {
                    return Ok(answer.clone());
                }
            }
            Ok(CommandOutput::success())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::testing::RecordingRunner;
    use super::*;

    fn prefixed(root: &Path) -> Layout {
        Layout::with_dirs(
            Some(root.to_path_buf()),
            root.join("config"),
            root.join("state"),
        )
    }

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"#!/bin/sh\n").unwrap();
    }

    #[test]
    fn the_qt6_greeter_wins_when_both_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = prefixed(tmp.path());
        touch(&tmp.path().join("usr/bin/sddm-greeter"));
        touch(&tmp.path().join("usr/bin/sddm-greeter-qt6"));
        let tools = Tools::detect(&layout);
        assert_eq!(tools.require_greeter().unwrap().name, "sddm-greeter-qt6");
    }

    #[test]
    fn the_qt5_greeter_is_used_when_it_is_the_only_one() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = prefixed(tmp.path());
        touch(&tmp.path().join("usr/bin/sddm-greeter"));
        assert_eq!(
            Tools::detect(&layout).require_greeter().unwrap().name,
            "sddm-greeter"
        );
    }

    #[test]
    fn a_missing_greeter_says_what_to_install() {
        let tmp = tempfile::tempdir().unwrap();
        let tools = Tools::detect(&prefixed(tmp.path()));
        let error = tools.require_greeter().unwrap_err().to_string();
        assert!(error.contains("sddm"), "{error}");
    }

    #[test]
    fn limine_mkinitcpio_wins_and_takes_no_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = prefixed(tmp.path());
        touch(&tmp.path().join("usr/bin/mkinitcpio"));
        touch(&tmp.path().join("usr/bin/limine-mkinitcpio"));
        let tools = Tools::detect(&layout);
        let initramfs = tools.require_initramfs().unwrap();
        assert_eq!(initramfs.kind, InitramfsKind::Limine);
        assert!(!initramfs.command().to_string().contains("-P"));
    }

    #[test]
    fn plain_mkinitcpio_is_invoked_with_dash_p() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = prefixed(tmp.path());
        touch(&tmp.path().join("usr/bin/mkinitcpio"));
        let tools = Tools::detect(&layout);
        assert!(
            tools
                .require_initramfs()
                .unwrap()
                .command()
                .to_string()
                .ends_with("mkinitcpio -P")
        );
    }

    #[test]
    fn a_spec_prints_exactly_what_would_run() {
        let spec = CommandSpec::new("sddm-greeter-qt6")
            .arg("--test-mode")
            .args(["--theme", "/tmp/stage/sddm"])
            .env("QT_QPA_PLATFORM", "offscreen");
        assert_eq!(
            spec.to_string(),
            "QT_QPA_PLATFORM=offscreen sddm-greeter-qt6 --test-mode --theme /tmp/stage/sddm"
        );
    }

    #[test]
    fn the_recording_runner_answers_from_its_script() {
        let runner = RecordingRunner::new()
            .fail_containing("sddm-greeter", CommandOutput::failure(1, "qml error"));
        let failed = runner
            .run(&CommandSpec::new("sddm-greeter").arg("--test-mode"))
            .unwrap();
        assert!(!failed.is_success());
        assert!(
            runner
                .run(&CommandSpec::new("mkinitcpio"))
                .unwrap()
                .is_success()
        );
        assert!(runner.ran_containing("mkinitcpio"));
    }

    #[test]
    fn a_real_command_that_hangs_is_killed_at_the_timeout() {
        let runner = RealRunner;
        let output = runner
            .run(
                &CommandSpec::new("sleep")
                    .arg("30")
                    .timeout(Duration::from_millis(300)),
            )
            .unwrap();
        assert!(output.timed_out);
        assert_eq!(output.describe_code(), "a timeout");
    }
}
