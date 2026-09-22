//! Installing and opening the Quattro plugin.
//!
//! The plugin is QML that runs inside `omarchy-shell`. It lives in the
//! repository under `plugin/` and is linked, not copied, into the place the
//! shell discovers plugins, so editing the QML is picked up by the shell's
//! hot reload. The two binaries are linked into `~/.local/bin`, which is
//! where the plugin looks for its engine.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::api::{PLUGIN_ID, plugin_source};
use crate::error::{Error, Result};
use crate::exec::which;
use crate::paths::Layout;

/// Where the shell looks for user plugins.
pub fn install_dir(layout: &Layout) -> PathBuf {
    layout.config_base().join("omarchy/plugins").join(PLUGIN_ID)
}

fn home(layout: &Layout) -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| layout.config_base().join(".."))
}

fn bin_dir(layout: &Layout) -> PathBuf {
    home(layout).join(".local/bin")
}

/// The plugin directory to install, or the reason there is none.
pub fn locate() -> Result<PathBuf> {
    plugin_source().ok_or_else(|| Error::Environment {
        what: "the plugin directory was not found next to this binary".to_string(),
        suggestion:
            "run this from the omaboot checkout (target/release/omaboot), or set OMABOOT_PLUGIN_DIR"
                .to_string(),
    })
}

pub fn install(layout: &Layout, source: &Path, dry_run: bool, out: &mut dyn Write) -> Result<()> {
    let target = install_dir(layout);
    let verb = if dry_run { "would link" } else { "linked" };

    link(source, &target, dry_run)?;
    writeln!(
        out,
        "{verb}     {} -> {}",
        target.display(),
        source.display()
    )
    .ok();

    let exe = std::env::current_exe().map_err(|error| Error::Environment {
        what: format!("the running binary could not be located: {error}"),
        suggestion: "run omaboot by its path".to_string(),
    })?;
    let bin = bin_dir(layout);
    for name in ["omaboot", "omaboot-apply"] {
        let binary = exe.with_file_name(name);
        if !binary.is_file() {
            writeln!(
                out,
                "skipped    {name}: {} is not there (cargo build --release builds both)",
                binary.display()
            )
            .ok();
            continue;
        }
        let dest = bin.join(name);
        if dest.exists() && !dest.is_symlink() {
            // A copy left there by hand goes stale the moment the binary is
            // rebuilt, so it gives way to a link, unless it is that binary.
            let same = fs::read(&dest).ok() == fs::read(&binary).ok();
            if same {
                writeln!(out, "kept       {} (identical copy)", dest.display()).ok();
                continue;
            }
            if !dry_run {
                fs::remove_file(&dest).map_err(|error| Error::write(dest.clone(), error))?;
            }
            writeln!(out, "replaced   {} (an older copy)", dest.display()).ok();
        }
        link(&binary, &dest, dry_run)?;
        writeln!(out, "{verb}     {} -> {}", dest.display(), binary.display()).ok();
    }

    // The helper next to this binary must be from the same build: one built
    // from older source writes other paths than this engine verifies, and
    // the engine refuses it at apply time. Saying so here is earlier.
    let helper = exe.with_file_name("omaboot-apply");
    if helper.is_file() {
        let want = crate::helper_protocol::PROTOCOL;
        let answer = std::process::Command::new(&helper)
            .arg("protocol")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string());
        match answer {
            Some(got) if got == want.to_string() => {
                writeln!(out, "checked    omaboot-apply speaks protocol {want}").ok();
            }
            Some(got) => {
                writeln!(
                    out,
                    "warning    omaboot-apply speaks protocol {got}, this omaboot needs {want}: it is from an older build; run cargo build --release (it builds both) and install again"
                )
                .ok();
            }
            None => {
                writeln!(
                    out,
                    "warning    omaboot-apply does not answer `protocol`: it is from an older build; run cargo build --release (it builds both) and install again"
                )
                .ok();
            }
        }
    }

    if dry_run || layout.is_prefixed() {
        writeln!(out, "would run  omarchy-shell shell rescanPlugins").ok();
        writeln!(
            out,
            "would run  omarchy-shell shell setPluginEnabled {PLUGIN_ID} true"
        )
        .ok();
        return Ok(());
    }
    shell(layout, &["shell", "rescanPlugins"], out)?;
    // The rescan runs in the background; enabling before it has seen the
    // manifest answers "unknown". Wait for the plugin to be listed.
    if wait_until_listed(layout) == Some(false) {
        return Err(Error::Environment {
            what: format!(
                "omarchy-shell did not list {PLUGIN_ID} after the rescan; its log says why (journalctl --user -u omarchy-shell, or the terminal the shell runs in)"
            ),
            suggestion: format!(
                "check {}/manifest.json and run `omaboot plugin install` again",
                target.display()
            ),
        });
    }
    shell(
        layout,
        &["shell", "setPluginEnabled", PLUGIN_ID, "true"],
        out,
    )?;
    writeln!(
        out,
        "\nopen it with `omaboot`, or from the shell: omarchy-shell shell summon {PLUGIN_ID} '{{}}'"
    )
    .ok();
    Ok(())
}

pub fn uninstall(layout: &Layout, dry_run: bool, out: &mut dyn Write) -> Result<()> {
    let target = install_dir(layout);
    if target.is_symlink() {
        if !dry_run {
            fs::remove_file(&target).map_err(|error| Error::write(target.clone(), error))?;
        }
        writeln!(
            out,
            "{} {}",
            if dry_run { "would remove" } else { "removed" },
            target.display()
        )
        .ok();
    } else if target.exists() {
        return Err(Error::Environment {
            what: format!("{} is not a link omaboot made", target.display()),
            suggestion: "remove it yourself if that is what you want".to_string(),
        });
    } else {
        writeln!(out, "nothing at {}", target.display()).ok();
    }
    if dry_run || layout.is_prefixed() {
        writeln!(
            out,
            "would run  omarchy-shell shell setPluginEnabled {PLUGIN_ID} false"
        )
        .ok();
        writeln!(out, "would run  omarchy-shell shell rescanPlugins").ok();
        return Ok(());
    }
    shell(
        layout,
        &["shell", "setPluginEnabled", PLUGIN_ID, "false"],
        out,
    )?;
    shell(layout, &["shell", "rescanPlugins"], out)?;
    Ok(())
}

/// What the shell knows about the plugin: listed, and enabled.
fn listed(layout: &Layout) -> Option<(bool, bool)> {
    let tool = which(layout, "omarchy-shell")?;
    let output = Command::new(&tool)
        .args(["shell", "listPlugins"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    let entry = value
        .as_array()?
        .iter()
        .find(|entry| entry["id"].as_str() == Some(PLUGIN_ID));
    Some(match entry {
        Some(entry) => (true, entry["enabled"].as_bool().unwrap_or(false)),
        None => (false, false),
    })
}

/// Poll `listPlugins` for a few seconds after a rescan. `None` when the
/// shell cannot be asked at all.
fn wait_until_listed(layout: &Layout) -> Option<bool> {
    for _ in 0..25 {
        match listed(layout) {
            None => return None,
            Some((true, _)) => return Some(true),
            Some((false, _)) => std::thread::sleep(std::time::Duration::from_millis(200)),
        }
    }
    Some(false)
}

/// What the shell answers about the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Window {
    /// Loaded and open: the summon landed.
    Open,
    /// Loaded but closed: the summon was sent before the window existed and
    /// was dropped, or the window has been closed since.
    Closed,
    /// The shell has no instance of the plugin yet (it answers "unknown").
    NotLoaded,
    /// The shell cannot be asked: no `call` method (an older shell), or no
    /// answer at all. Polling is then pointless and the summon has to do.
    Unavailable,
}

/// The two IPC calls `open` needs, so the waiting can be tested without a
/// shell.
trait ShellIpc {
    /// `shell summon`: what the shell said, or `None` when it could not be run.
    fn summon(&self) -> std::result::Result<String, String>;
    fn window(&self) -> Window;
}

struct RunningShell {
    tool: PathBuf,
}

impl ShellIpc for RunningShell {
    fn summon(&self) -> std::result::Result<String, String> {
        let output = Command::new(&self.tool)
            .args(["shell", "summon", PLUGIN_ID, "{}"])
            .output()
            .map_err(|error| format!("omarchy-shell could not be run: {error}"))?;
        let reply = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if output.status.success() {
            Ok(reply)
        } else if reply.is_empty() {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        } else {
            Err(reply)
        }
    }

    fn window(&self) -> Window {
        let Ok(output) = Command::new(&self.tool)
            .args(["shell", "call", PLUGIN_ID, "ping", ""])
            .output()
        else {
            return Window::Unavailable;
        };
        if !output.status.success() {
            return Window::Unavailable;
        }
        match String::from_utf8_lossy(&output.stdout).trim() {
            "open" => Window::Open,
            "closed" => Window::Closed,
            "unknown" => Window::NotLoaded,
            _ => Window::Unavailable,
        }
    }
}

/// Time, so the wait in `open` can be tested in no time at all.
trait Clock {
    fn elapsed(&self) -> std::time::Duration;
    fn sleep(&self, duration: std::time::Duration);
}

struct WallClock(std::time::Instant);

impl Clock for WallClock {
    fn elapsed(&self) -> std::time::Duration {
        self.0.elapsed()
    }
    fn sleep(&self, duration: std::time::Duration) {
        std::thread::sleep(duration);
    }
}

/// How long `open` waits for the window before giving up, and how often it
/// asks. A plugin reload takes the shell a second or two; ten seconds is
/// long enough for the reload `plugin install` starts, plus the one the file
/// watcher adds behind it, and short enough to sit through when it fails.
const OPEN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const OPEN_POLL: std::time::Duration = std::time::Duration::from_millis(200);
/// A summon that has not produced a window after this long is sent again:
/// the shell drops a summon that arrives while it is reloading its plugins.
const RESUMMON_AFTER: std::time::Duration = std::time::Duration::from_secs(2);

/// Summon the window and wait until the shell says it is open.
///
/// `omarchy-shell shell summon` answers "ok" as soon as it has noted the
/// request, and a request noted while the shell is reloading its plugins
/// (which `plugin install` and the file watcher both cause) is cleared with
/// the panels and never opens anything. So after each summon the window is
/// asked, through `shell call`, whether it is open, and the summon is sent
/// again when nothing has appeared; after `OPEN_TIMEOUT` this gives up and
/// says so. A shell that cannot answer `call` is trusted on its "ok".
fn open_window(shell: &dyn ShellIpc, clock: &dyn Clock) -> Result<()> {
    let mut last_summon = None;
    let mut last_reply = String::new();
    loop {
        let elapsed = clock.elapsed();
        if last_summon.is_some() && elapsed >= OPEN_TIMEOUT {
            return Err(Error::Environment {
                what: if last_reply == "ok" {
                    format!(
                        "the shell accepted the summon but no window appeared in {} seconds; it was most likely still reloading its plugins (it does that after `plugin install`, and again when the plugin's files change)",
                        OPEN_TIMEOUT.as_secs()
                    )
                } else {
                    format!(
                        "the shell did not open the plugin (it said: {})",
                        if last_reply.is_empty() {
                            "nothing"
                        } else {
                            &last_reply
                        }
                    )
                },
                suggestion: "run `omaboot` again; if it still does not open, the shell's log says why: journalctl --user -u omarchy-shell, or the terminal the shell runs in".to_string(),
            });
        }
        let due = match last_summon {
            None => true,
            Some(at) => elapsed.saturating_sub(at) >= RESUMMON_AFTER,
        };
        if due {
            match shell.summon() {
                Ok(reply) => last_reply = reply,
                Err(reason) => {
                    return Err(Error::Environment {
                        what: format!("the shell did not open the plugin (it said: {reason})"),
                        suggestion: "is the shell running? Its log says why: journalctl --user -u omarchy-shell, or the terminal the shell runs in".to_string(),
                    });
                }
            }
            last_summon = Some(elapsed);
        }
        match shell.window() {
            Window::Open => return Ok(()),
            Window::Unavailable if last_reply == "ok" => return Ok(()),
            Window::Unavailable | Window::NotLoaded | Window::Closed => {}
        }
        clock.sleep(OPEN_POLL);
    }
}

/// Summon the plugin in the running shell.
pub fn open(layout: &Layout, out: &mut dyn Write) -> Result<()> {
    if layout.is_prefixed() {
        writeln!(
            out,
            "would run  omarchy-shell shell summon {PLUGIN_ID} '{{}}'"
        )
        .ok();
        return Ok(());
    }
    let Some(tool) = which(layout, "omarchy-shell") else {
        return Err(Error::Environment {
            what: "omarchy-shell was not found, so there is no shell to open the plugin in"
                .to_string(),
            suggestion: "use the subcommands: omaboot status, omaboot new, omaboot apply"
                .to_string(),
        });
    };
    // A linked plugin is a development checkout, and the shell's file
    // watcher does not follow the link, so its code is reloaded here before
    // every open. A copied plugin is left alone.
    if install_dir(layout).is_symlink() {
        let _ = Command::new(&tool)
            .args(["shell", "rescanPlugins"])
            .output();
        let _ = wait_until_listed(layout);
    }
    // Not listed: a stale scan, or not installed. Listed but disabled:
    // enable. Either way the summon that follows is the one that matters.
    match listed(layout) {
        Some((false, _)) => {
            if !install_dir(layout).join("manifest.json").is_file() {
                return Err(Error::Environment {
                    what: "the plugin is not installed in omarchy-shell".to_string(),
                    suggestion: "run `omaboot plugin install` once".to_string(),
                });
            }
            let _ = Command::new(&tool)
                .args(["shell", "rescanPlugins"])
                .output();
            if wait_until_listed(layout) != Some(true) {
                return Err(Error::Environment {
                    what: format!(
                        "omarchy-shell does not list {PLUGIN_ID}, although it is linked at {}",
                        install_dir(layout).display()
                    ),
                    suggestion: "the shell's log says why it refused the manifest: journalctl --user -u omarchy-shell, or the terminal the shell runs in"
                        .to_string(),
                });
            }
            let _ = Command::new(&tool)
                .args(["shell", "setPluginEnabled", PLUGIN_ID, "true"])
                .output();
        }
        Some((true, false)) => {
            let _ = Command::new(&tool)
                .args(["shell", "setPluginEnabled", PLUGIN_ID, "true"])
                .output();
        }
        _ => {}
    }
    open_window(
        &RunningShell { tool },
        &WallClock(std::time::Instant::now()),
    )
}

fn shell(layout: &Layout, args: &[&str], out: &mut dyn Write) -> Result<()> {
    let Some(tool) = which(layout, "omarchy-shell") else {
        writeln!(
            out,
            "omarchy-shell was not found; when the shell runs: omarchy-shell {}",
            args.join(" ")
        )
        .ok();
        return Ok(());
    };
    let output = Command::new(&tool)
        .args(args)
        .output()
        .map_err(|error| Error::Environment {
            what: format!("omarchy-shell could not be run: {error}"),
            suggestion: "is the shell running?".to_string(),
        })?;
    let reply = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr);
        let said = said.trim().lines().last().unwrap_or("nothing").to_string();
        writeln!(
            out,
            "failed     omarchy-shell {} ({}): {said}",
            args.join(" "),
            output
                .status
                .code()
                .map(|code| format!("exit code {code}"))
                .unwrap_or_else(|| "a signal".to_string())
        )
        .ok();
        return Err(Error::Environment {
            what: format!(
                "the shell did not take `omarchy-shell {}`; it said: {said}. The links are in place",
                args.join(" ")
            ),
            suggestion: "run it from the desktop session (OMARCHY_PATH set, the shell running), or run the omarchy-shell command by hand"
                .to_string(),
        });
    }
    writeln!(
        out,
        "ran        omarchy-shell {}{}",
        args.join(" "),
        if reply.is_empty() {
            String::new()
        } else {
            format!("  ({reply})")
        }
    )
    .ok();
    Ok(())
}

/// Make `link` point at `target`, replacing a previous link and refusing to
/// replace anything that is not one.
fn link(target: &Path, link: &Path, dry_run: bool) -> Result<()> {
    if link.exists() && !link.is_symlink() {
        return Err(Error::Environment {
            what: format!("{} exists and is not a link", link.display()),
            suggestion: "move it out of the way first".to_string(),
        });
    }
    if dry_run {
        return Ok(());
    }
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent).map_err(|error| Error::write(parent.to_path_buf(), error))?;
    }
    if link.is_symlink() {
        fs::remove_file(link).map_err(|error| Error::write(link.to_path_buf(), error))?;
    }
    std::os::unix::fs::symlink(target, link)
        .map_err(|error| Error::write(link.to_path_buf(), error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::time::Duration;

    /// A shell that answers from a script: what each summon says, and what
    /// the window says on each ask, the last answer repeating.
    struct ScriptedShell {
        summons: RefCell<Vec<&'static str>>,
        windows: RefCell<Vec<Window>>,
        summoned: Cell<usize>,
        asked: Cell<usize>,
    }

    impl ScriptedShell {
        fn new(summons: &[&'static str], windows: &[Window]) -> Self {
            Self {
                summons: RefCell::new(summons.to_vec()),
                windows: RefCell::new(windows.to_vec()),
                summoned: Cell::new(0),
                asked: Cell::new(0),
            }
        }
    }

    impl ShellIpc for ScriptedShell {
        fn summon(&self) -> std::result::Result<String, String> {
            let n = self.summoned.get();
            self.summoned.set(n + 1);
            let summons = self.summons.borrow();
            let reply = summons[n.min(summons.len() - 1)];
            match reply.strip_prefix("error:") {
                Some(reason) => Err(reason.to_string()),
                None => Ok(reply.to_string()),
            }
        }
        fn window(&self) -> Window {
            let n = self.asked.get();
            self.asked.set(n + 1);
            let windows = self.windows.borrow();
            windows[n.min(windows.len() - 1)]
        }
    }

    /// A clock that only moves when slept on.
    struct FakeClock(Cell<Duration>);

    impl Clock for FakeClock {
        fn elapsed(&self) -> Duration {
            self.0.get()
        }
        fn sleep(&self, duration: Duration) {
            self.0.set(self.0.get() + duration);
        }
    }

    fn clock() -> FakeClock {
        FakeClock(Cell::new(Duration::ZERO))
    }

    #[test]
    fn a_summon_that_opens_the_window_is_sent_once() {
        let shell = ScriptedShell::new(&["ok"], &[Window::NotLoaded, Window::Open]);
        open_window(&shell, &clock()).unwrap();
        assert_eq!(shell.summoned.get(), 1);
        assert_eq!(shell.asked.get(), 2);
    }

    #[test]
    fn a_summon_the_shell_dropped_while_reloading_is_sent_again() {
        // The first summon is accepted and never opens anything (the shell
        // was reloading); after two seconds it goes again, and that one lands.
        let clock = clock();
        let not_loaded = vec![Window::NotLoaded; 12];
        let mut windows = not_loaded;
        windows.push(Window::Open);
        let shell = ScriptedShell::new(&["ok"], &windows);
        open_window(&shell, &clock).unwrap();
        assert_eq!(shell.summoned.get(), 2);
        assert!(clock.elapsed() >= RESUMMON_AFTER);
        assert!(clock.elapsed() < OPEN_TIMEOUT);
    }

    #[test]
    fn giving_up_says_the_shell_was_reloading() {
        let clock = clock();
        let shell = ScriptedShell::new(&["ok"], &[Window::NotLoaded]);
        let error = open_window(&shell, &clock).unwrap_err().to_string();
        assert!(
            error.contains("no window appeared in 10 seconds"),
            "{error}"
        );
        assert!(error.contains("reloading its plugins"), "{error}");
        assert!(error.contains("run `omaboot` again"), "{error}");
        assert!(clock.elapsed() >= OPEN_TIMEOUT);
        // Sent again every RESUMMON_AFTER until the timeout.
        assert_eq!(
            shell.summoned.get() as u64,
            OPEN_TIMEOUT.as_secs() / RESUMMON_AFTER.as_secs()
        );
    }

    #[test]
    fn a_shell_that_cannot_be_asked_is_trusted_on_its_ok() {
        let shell = ScriptedShell::new(&["ok"], &[Window::Unavailable]);
        open_window(&shell, &clock()).unwrap();
        assert_eq!(shell.summoned.get(), 1);
    }

    #[test]
    fn an_unknown_plugin_is_reported_when_nothing_appears() {
        let clock = clock();
        let shell = ScriptedShell::new(&["unknown"], &[Window::NotLoaded]);
        let error = open_window(&shell, &clock).unwrap_err().to_string();
        assert!(error.contains("it said: unknown"), "{error}");
    }

    #[test]
    fn a_shell_that_cannot_be_run_fails_at_once() {
        let clock = clock();
        let shell = ScriptedShell::new(
            &["error:omarchy-shell is not running"],
            &[Window::NotLoaded],
        );
        let error = open_window(&shell, &clock).unwrap_err().to_string();
        assert!(error.contains("omarchy-shell is not running"), "{error}");
        assert_eq!(clock.elapsed(), Duration::ZERO);
    }

    #[test]
    fn a_link_replaces_a_link_and_refuses_a_real_file() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target");
        fs::create_dir(&target).unwrap();
        let at = tmp.path().join("plugins/x");
        link(&target, &at, false).unwrap();
        assert_eq!(fs::read_link(&at).unwrap(), target);
        let other = tmp.path().join("other");
        fs::create_dir(&other).unwrap();
        link(&other, &at, false).unwrap();
        assert_eq!(fs::read_link(&at).unwrap(), other);

        let real = tmp.path().join("real");
        fs::write(&real, b"x").unwrap();
        assert!(link(&target, &real, false).is_err());
        assert_eq!(fs::read(&real).unwrap(), b"x");
    }

    #[test]
    fn a_dry_run_install_under_a_prefix_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let plugin = tmp.path().join("plugin");
        fs::create_dir_all(&plugin).unwrap();
        fs::write(plugin.join("manifest.json"), "{}").unwrap();
        let layout = Layout::with_dirs(
            Some(tmp.path().join("root")),
            tmp.path().join("config/omaboot"),
            tmp.path().join("state"),
        );
        let mut out = Vec::new();
        install(&layout, &plugin, true, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("would link"), "{text}");
        assert!(
            text.contains("setPluginEnabled mtolhuys.omaboot true"),
            "{text}"
        );
        assert!(!install_dir(&layout).exists());
    }

    #[test]
    fn a_shell_call_that_fails_is_reported_as_failed_with_what_the_shell_said() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let layout = Layout::with_dirs(
            Some(root.clone()),
            tmp.path().join("config/omaboot"),
            tmp.path().join("state"),
        );
        let bin = root.join("usr/bin");
        fs::create_dir_all(&bin).unwrap();
        let fake = bin.join("omarchy-shell");
        fs::write(
            &fake,
            "#!/bin/sh\necho 'OMARCHY_PATH is not set' >&2\nexit 1\n",
        )
        .unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();

        let mut out = Vec::new();
        let error = shell(&layout, &["shell", "rescanPlugins"], &mut out)
            .unwrap_err()
            .to_string();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("failed     omarchy-shell shell rescanPlugins (exit code 1): OMARCHY_PATH is not set"),
            "{text}"
        );
        assert!(error.contains("OMARCHY_PATH is not set"), "{error}");
        assert!(error.contains("The links are in place"), "{error}");

        fs::write(&fake, "#!/bin/sh\necho ok\n").unwrap();
        let mut out = Vec::new();
        shell(&layout, &["shell", "rescanPlugins"], &mut out).unwrap();
        assert!(
            String::from_utf8(out)
                .unwrap()
                .contains("ran        omarchy-shell shell rescanPlugins  (ok)")
        );
    }

    /// The manifest the shell reads and the crate are one release. `plugin
    /// install` links the QML and the binaries together, the engine refuses a
    /// helper from another build, and the package takes its version from the
    /// crate, so a manifest that says something else is a lie the shell
    /// repeats.
    #[test]
    fn the_manifest_carries_the_crate_version_and_the_plugin_id() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../plugin/manifest.json")).unwrap();
        assert_eq!(
            manifest["version"].as_str(),
            Some(env!("CARGO_PKG_VERSION")),
            "plugin/manifest.json and Cargo.toml disagree about the version"
        );
        assert_eq!(
            manifest["id"].as_str(),
            Some(PLUGIN_ID),
            "plugin/manifest.json and api::PLUGIN_ID disagree about the id"
        );
    }
}
