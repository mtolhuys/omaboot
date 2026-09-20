//! The real preview: the actual daemons drawing the actual theme.
//!
//! The login screen is `sddm-greeter --test-mode` on the staged theme, which
//! opens as a window in the running session and needs no privilege. The unlock
//! and shutdown screens are `plymouthd` with its X11 renderer, drawing into an
//! X display: the one in `DISPLAY`, or a rootful `Xwayland` window this module
//! starts for the occasion. plymouthd needs root, so it is started through
//! `sudo`, and the theme it draws is put under `/run/plymouth/themes` by the
//! privileged helper, where a reboot removes it and where the installed theme
//! is never touched.
//!
//! Everything started here is stopped again when the preview ends, however it
//! ends: the guard types kill what they own on drop.

use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::{Duration, Instant};

use crate::apply::{ApplyRequest, Pipeline, StepId};
use crate::error::{Error, Result};
use crate::exec::{Owned, Tools};
use crate::generate::preview_theme_file;
use crate::paths::{Layout, PREVIEW_THEME_ID};
use crate::render::Screen;

/// What `omaboot preview` was asked to do.
#[derive(Debug, Clone)]
pub struct Options {
    pub screen: Screen,
    /// How long a preview stays up when nothing ends it sooner.
    pub seconds: u64,
    /// Where to save a PNG of the X display while the splash is up, if
    /// anywhere. Needs ImageMagick's `import`.
    pub screenshot: Option<PathBuf>,
    /// The size of an Xwayland window this starts.
    pub width: u32,
    pub height: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            screen: Screen::Unlock,
            seconds: 30,
            screenshot: None,
            width: 1920,
            height: 1080,
        }
    }
}

/// What a preview draws: a theme staged from your files, or the themes that
/// are installed right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staged {
    /// The SDDM theme directory the greeter is pointed at.
    pub sddm: PathBuf,
    /// The Plymouth theme to publish under `/run` before plymouthd starts,
    /// or nothing when the theme to show is already installed.
    pub plymouth_preview: Option<PathBuf>,
    /// The name plymouthd is asked for on its fake kernel command line.
    pub plymouth_theme: String,
    /// One line naming what is being shown.
    pub describe: String,
}

impl Staged {
    /// The screens that boot now, straight from the installed themes. No
    /// staging, no helper, nothing published: plymouthd reads the theme it
    /// already has, and the greeter is pointed at the installed directory.
    pub fn installed(layout: &Layout) -> Result<Self> {
        let snapshot = crate::system::Snapshot::read(layout);
        let plymouth_theme = snapshot
            .plymouth
            .theme
            .clone()
            .ok_or_else(|| Error::Environment {
                what: format!(
                    "{} names no Plymouth theme, so there is nothing to show",
                    snapshot.plymouth.conf.display()
                ),
                suggestion: "preview one of your themes instead".to_string(),
            })?;
        let sddm = snapshot
            .login
            .dir
            .clone()
            .ok_or_else(|| Error::Environment {
                what: match &snapshot.login.theme {
                    Some(theme) => format!("the login theme {} is not installed", theme.name),
                    None => "no SDDM configuration names a login theme".to_string(),
                },
                suggestion: "preview one of your themes instead".to_string(),
            })?;
        Ok(Self {
            sddm,
            plymouth_preview: None,
            plymouth_theme,
            describe: snapshot.headline(),
        })
    }
}

/// Run validation, generation and staging, then write the preview variant of
/// the Plymouth theme next to the stage. Nothing outside the state directory
/// is touched.
pub fn stage(pipeline: &Pipeline<'_>, theme: &str) -> Result<Staged> {
    let valid = pipeline.load_theme(theme)?;
    let request = ApplyRequest {
        theme: theme.to_string(),
        dry_run: false,
        only: Some(vec![StepId::Stage]),
    };
    pipeline.apply(&request)?;

    let layout = pipeline.layout();
    let source = layout.stage_dir().join("plymouth");
    let target = layout.preview_stage_dir();
    let _ = fs::remove_dir_all(&target);
    fs::create_dir_all(&target).map_err(|error| Error::write(target.clone(), error))?;
    for entry in fs::read_dir(&source).map_err(|error| Error::read(source.clone(), error))? {
        let entry = entry.map_err(|error| Error::read(source.clone(), error))?;
        let name = entry.file_name();
        if name == "omaboot.plymouth" {
            continue;
        }
        fs::copy(entry.path(), target.join(&name))
            .map_err(|error| Error::write(target.join(&name), error))?;
    }
    let theme_file = preview_theme_file(&valid)?;
    crate::state::write_atomic(
        &target.join(format!("{PREVIEW_THEME_ID}.plymouth")),
        theme_file.as_bytes(),
    )?;

    Ok(Staged {
        sddm: layout.stage_dir().join("sddm"),
        plymouth_preview: Some(target),
        plymouth_theme: PREVIEW_THEME_ID.to_string(),
        describe: format!("your theme {theme}"),
    })
}

/// The commands a preview would run, for `--dry-run` and for the tests.
pub fn plan(layout: &Layout, tools: &Tools, staged: &Staged, options: &Options) -> Vec<String> {
    let mut lines = Vec::new();
    match options.screen {
        Screen::Login => {
            let greeter = tools
                .greeter
                .as_ref()
                .map(|greeter| greeter.path.display().to_string())
                .unwrap_or_else(|| "sddm-greeter-qt6 (not found)".to_string());
            lines.push(format!(
                "run      {greeter} --test-mode --theme {}",
                staged.sddm.display()
            ));
            lines.push(format!(
                "wait     until the greeter window is closed, or {} seconds",
                options.seconds
            ));
        }
        Screen::Unlock | Screen::Shutdown => {
            let sudo = privilege_prefix();
            let helper = tools
                .helper
                .as_ref()
                .map(|helper| helper.display().to_string())
                .unwrap_or_else(|| "omaboot-apply".to_string());
            let mode = if options.screen == Screen::Shutdown {
                "shutdown"
            } else {
                "boot"
            };
            if !sudo.is_empty() {
                lines.push(format!("run      {sudo}-v"));
            }
            match &staged.plymouth_preview {
                Some(preview) => lines.push(format!(
                    "run      {sudo}{helper} preview install --staged {}",
                    preview.display()
                )),
                None => lines.push(format!(
                    "use      the installed Plymouth theme {}",
                    staged.plymouth_theme
                )),
            }
            match std::env::var("DISPLAY") {
                Ok(display) if !display.is_empty() => {
                    lines.push(format!("use      DISPLAY={display}"));
                }
                _ => lines.push(format!(
                    "run      Xwayland :<free> -ac -geometry {}x{} -decorate -noreset",
                    options.width, options.height
                )),
            }
            lines.push(format!(
                "run      {sudo}script -qfec \"DISPLAY=<display> plymouthd --no-daemon --mode={mode} --tty=$(tty) \
                 --kernel-command-line='splash plymouth.ignore-serial-consoles plymouth.splash={}'\" /dev/null",
                staged.plymouth_theme
            ));
            lines.push(format!("run      {sudo}plymouth show-splash"));
            if options.screen == Screen::Unlock {
                lines.push(format!(
                    "run      {sudo}plymouth ask-for-password    # type into the preview window, Enter continues to the progress bar"
                ));
            }
            lines.push(format!(
                "wait     {} seconds, or Enter here",
                options.seconds
            ));
            lines.push(format!("run      {sudo}plymouth quit"));
            if staged.plymouth_preview.is_some() {
                lines.push(format!("run      {sudo}{helper} preview remove"));
            }
            let _ = layout;
        }
    }
    lines
}

/// Run the preview for real. `say` gets one line per thing that happens, so
/// the CLI and the TUI can both show progress.
pub fn run(
    layout: &Layout,
    tools: &Tools,
    staged: &Staged,
    options: &Options,
    say: &mut dyn FnMut(&str),
    stop: &mut dyn FnMut() -> bool,
) -> Result<()> {
    if layout.is_prefixed() {
        return Err(Error::Environment {
            what: "a live preview cannot run against a --root prefix".to_string(),
            suggestion: "run it without --root; it changes nothing on the system, or use --dry-run to see the commands"
                .to_string(),
        });
    }
    match options.screen {
        Screen::Login => run_greeter(tools, staged, options, say, stop),
        Screen::Unlock | Screen::Shutdown => run_plymouth(tools, staged, options, say, stop),
    }
}

fn run_greeter(
    tools: &Tools,
    staged: &Staged,
    options: &Options,
    say: &mut dyn FnMut(&str),
    stop: &mut dyn FnMut() -> bool,
) -> Result<()> {
    let greeter = tools.require_greeter()?;
    say(&format!(
        "starting {} --test-mode on {}",
        greeter.name,
        staged.sddm.display()
    ));
    // The same command the smoke test runs, minus the offscreen platform and
    // the settling time: here it stays up until the window is closed.
    let child = crate::greeter::test_mode(greeter, &staged.sddm)
        .to_command()
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| spawn_error(&greeter.name, error))?;
    let mut child = Owned(child);
    say("the greeter is open in its own window; close it to end the preview");

    let started = Instant::now();
    loop {
        if let Some(status) = child.0.try_wait().map_err(wait_error)? {
            if status.success() {
                say("the greeter closed");
                return Ok(());
            }
            let stderr = child
                .0
                .stderr
                .take()
                .map(|mut pipe| {
                    let mut text = String::new();
                    let _ = std::io::Read::read_to_string(&mut pipe, &mut text);
                    text
                })
                .unwrap_or_default();
            let complaints = crate::greeter::complaints(&stderr, &staged.sddm);
            let said = if complaints.is_empty() {
                stderr.lines().rev().take(5).collect::<Vec<_>>().join(" / ")
            } else {
                complaints.join(" / ")
            };
            return Err(Error::Command {
                command: format!("{} --test-mode", greeter.name),
                code: status
                    .code()
                    .map(|code| format!("exit code {code}"))
                    .unwrap_or_else(|| "a signal".to_string()),
                stderr: said,
                suggestion: "the greeter refused this theme; fix what it names, this is exactly what the smoke test in apply would have stopped"
                    .to_string(),
            });
        }
        if stop() {
            say("stopping the greeter");
            return Ok(());
        }
        if started.elapsed() >= Duration::from_secs(options.seconds) {
            say("time is up, stopping the greeter");
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn run_plymouth(
    tools: &Tools,
    staged: &Staged,
    options: &Options,
    say: &mut dyn FnMut(&str),
    stop: &mut dyn FnMut() -> bool,
) -> Result<()> {
    let sudo = privilege_prefix();
    let mode = if options.screen == Screen::Shutdown {
        "shutdown"
    } else {
        "boot"
    };

    if !sudo.is_empty() {
        say("asking for authorisation once: plymouthd has to run as root");
        run_checked(&privileged(&["-v"]))?;
    }

    let _cleanup = match &staged.plymouth_preview {
        Some(preview) => {
            let helper = tools.require_helper()?;
            say(&format!(
                "putting the preview theme under {}",
                crate::paths::PREVIEW_THEME_DIR
            ));
            run_checked(&privileged(&[
                &helper.display().to_string(),
                "preview",
                "install",
                "--staged",
                &preview.display().to_string(),
            ]))?;
            Some(PreviewTheme {
                helper: helper.to_path_buf(),
            })
        }
        None => {
            say(&format!(
                "showing the installed Plymouth theme {}",
                staged.plymouth_theme
            ));
            None
        }
    };

    let display = Display::acquire(options, say)?;

    say(&format!(
        "starting plymouthd in {mode} mode on {}",
        display.name()
    ));
    // DISPLAY travels inside the command line rather than through sudo's
    // environment handling, which sudoers may or may not allow.
    let plymouthd = format!(
        "DISPLAY={} plymouthd --no-daemon --mode={mode} --tty=$(tty) \
         --kernel-command-line='splash plymouth.ignore-serial-consoles plymouth.splash={}'",
        display.name(),
        staged.plymouth_theme
    );
    let argv = privileged(&["script", "-qfec", &plymouthd, "/dev/null"]);
    let mut command = Command::new(&argv[0]);
    command.args(&argv[1..]);
    let daemon = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| spawn_error("plymouthd", error))?;
    let mut daemon = Owned(daemon);

    // Wait for the daemon to answer.
    let started = Instant::now();
    loop {
        if run_quiet(&privileged(&["plymouth", "--ping"])) {
            break;
        }
        if let Some(status) = daemon.0.try_wait().map_err(wait_error)? {
            return Err(Error::Command {
                command: "plymouthd".to_string(),
                code: status
                    .code()
                    .map(|code| format!("exit code {code}"))
                    .unwrap_or_else(|| "a signal".to_string()),
                stderr: "it exited before answering a ping".to_string(),
                suggestion: "run it by hand with --debug to see why; a missing x11 renderer plugin or a display root cannot open are the usual causes"
                    .to_string(),
            });
        }
        if started.elapsed() > Duration::from_secs(10) {
            return Err(Error::step(
                "preview",
                "plymouthd did not answer within ten seconds",
                "run `plymouthd --no-daemon --debug` by hand to see what it is waiting for",
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }

    run_checked(&privileged(&["plymouth", "show-splash"]))?;
    say("the splash is showing");

    let mut password: Option<Owned> = None;
    if options.screen == Screen::Unlock {
        let argv = privileged(&["plymouth", "ask-for-password", "--prompt", ""]);
        let child = Command::new(&argv[0])
            .args(&argv[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| spawn_error("plymouth ask-for-password", error))?;
        password = Some(Owned(child));
        say("type anything into the preview window and press Enter to see the progress bar");
    }

    if let Some(path) = &options.screenshot {
        thread::sleep(Duration::from_millis(800));
        screenshot(display.name(), path, say);
    }

    say(&format!(
        "the preview stays up for {} seconds",
        options.seconds
    ));
    let started = Instant::now();
    let mut answered_at: Option<Instant> = None;
    loop {
        if let Some(child) = password.as_mut()
            && answered_at.is_none()
            && child.0.try_wait().ok().flatten().is_some()
        {
            say("password accepted; the progress bar is running");
            answered_at = Some(Instant::now());
            if let Some(path) = &options.screenshot {
                thread::sleep(Duration::from_millis(800));
                let after = path.with_file_name(format!(
                    "{}-after.png",
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("preview")
                ));
                screenshot(display.name(), &after, say);
            }
        }
        if answered_at.is_some_and(|at| at.elapsed() >= Duration::from_secs(4)) {
            break;
        }
        if stop() || started.elapsed() >= Duration::from_secs(options.seconds) {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    say("stopping plymouthd");
    let _ = run_quiet(&privileged(&["plymouth", "quit"]));
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if daemon.0.try_wait().ok().flatten().is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    drop(password);
    drop(daemon);
    drop(display);
    Ok(())
}

/// An X display to draw into: the session's, or one started for this.
enum Display {
    Existing(String),
    /// The child is held so that dropping the display stops the server.
    Spawned {
        name: String,
        _child: Owned,
    },
}

impl Drop for Display {
    fn drop(&mut self) {
        if let Self::Spawned { name, _child } = self {
            // Killing the server leaves its socket and lock behind; they are
            // ours, and a stale one would make the next preview think the
            // number is taken.
            let _ = _child.0.kill();
            let _ = _child.0.wait();
            let number = name.trim_start_matches(':');
            let _ = fs::remove_file(format!("/tmp/.X11-unix/X{number}"));
            let _ = fs::remove_file(format!("/tmp/.X{number}-lock"));
        }
    }
}

impl Display {
    fn acquire(options: &Options, say: &mut dyn FnMut(&str)) -> Result<Self> {
        if let Ok(display) = std::env::var("DISPLAY")
            && !display.is_empty()
        {
            say(&format!("drawing into DISPLAY={display}"));
            return Ok(Self::Existing(display));
        }
        if std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return Err(Error::Environment {
                what: "there is no display to draw the Plymouth preview into".to_string(),
                suggestion: "run this from a graphical session, or set DISPLAY to an X server"
                    .to_string(),
            });
        }
        let xwayland =
            crate::exec::which(&Layout::discover(None)?, "Xwayland").ok_or_else(|| {
                Error::ToolMissing {
                    tool: "Xwayland".to_string(),
                    suggestion:
                        "install xorg-xwayland; the Plymouth preview draws through it on Wayland"
                            .to_string(),
                }
            })?;
        let number = free_display_number().ok_or_else(|| {
            Error::step(
                "preview",
                "no free X display number between :50 and :99",
                "close some X servers, or set DISPLAY to one of them",
            )
        })?;
        let name = format!(":{number}");
        say(&format!(
            "opening an Xwayland window {} for the preview",
            name
        ));
        let child = Command::new(xwayland)
            .arg(&name)
            .args([
                "-ac",
                "-geometry",
                &format!("{}x{}", options.width, options.height),
                "-decorate",
                "-noreset",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| spawn_error("Xwayland", error))?;
        let mut child = Owned(child);

        let socket = PathBuf::from(format!("/tmp/.X11-unix/X{number}"));
        let started = Instant::now();
        while !socket.exists() {
            if let Some(status) = child.0.try_wait().map_err(wait_error)? {
                return Err(Error::Command {
                    command: "Xwayland".to_string(),
                    code: status
                        .code()
                        .map(|code| format!("exit code {code}"))
                        .unwrap_or_else(|| "a signal".to_string()),
                    stderr: "it exited before opening its display".to_string(),
                    suggestion:
                        "run `Xwayland :77 -ac -geometry 1920x1080 -decorate` by hand to see why"
                            .to_string(),
                });
            }
            if started.elapsed() > Duration::from_secs(5) {
                return Err(Error::step(
                    "preview",
                    "Xwayland did not open its display within five seconds",
                    "run it by hand to see what it is waiting for",
                ));
            }
            thread::sleep(Duration::from_millis(50));
        }
        Ok(Self::Spawned {
            name,
            _child: child,
        })
    }

    fn name(&self) -> &str {
        match self {
            Self::Existing(name) | Self::Spawned { name, .. } => name,
        }
    }
}

fn free_display_number() -> Option<u32> {
    (50..100).find(|number| {
        !Path::new(&format!("/tmp/.X11-unix/X{number}")).exists()
            && !Path::new(&format!("/tmp/.X{number}-lock")).exists()
    })
}

/// Takes the preview theme away again when the preview ends.
struct PreviewTheme {
    helper: PathBuf,
}

impl Drop for PreviewTheme {
    fn drop(&mut self) {
        let _ = run_quiet(&privileged(&[
            &self.helper.display().to_string(),
            "preview",
            "remove",
        ]));
    }
}

/// `sudo ` unless this process is root already.
fn privilege_prefix() -> &'static str {
    if is_root() { "" } else { "sudo " }
}

fn is_root() -> bool {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("Uid:")
                    .and_then(|rest| rest.split_whitespace().nth(1))
                    .and_then(|uid| uid.parse::<u32>().ok())
            })
        })
        .is_some_and(|uid| uid == 0)
}

/// A command line, with `sudo` in front when needed.
fn privileged(args: &[&str]) -> Vec<String> {
    let mut command: Vec<String> = Vec::new();
    if !is_root() {
        command.extend(crate::auth::sudo_args());
    }
    command.extend(args.iter().map(|arg| arg.to_string()));
    command
}

fn run_checked(command: &[String]) -> Result<()> {
    let output = Command::new(&command[0])
        .args(&command[1..])
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| spawn_error(&command[0], error))?;
    if output.status.success() {
        return Ok(());
    }
    Err(Error::Command {
        command: command.join(" "),
        code: output
            .status
            .code()
            .map(|code| format!("exit code {code}"))
            .unwrap_or_else(|| "a signal".to_string()),
        stderr: String::from_utf8_lossy(&output.stderr)
            .lines()
            .take(5)
            .collect::<Vec<_>>()
            .join(" / "),
        suggestion: "the preview was stopped and everything it started was cleaned up".to_string(),
    })
}

fn run_quiet(command: &[String]) -> bool {
    Command::new(&command[0])
        .args(&command[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn screenshot(display: &str, path: &Path, say: &mut dyn FnMut(&str)) {
    let status = Command::new("import")
        .env("DISPLAY", display)
        .args(["-window", "root"])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match status {
        Ok(status) if status.success() => say(&format!("saved {}", path.display())),
        _ => say("no screenshot: ImageMagick's `import` is not available"),
    }
}

/// A line of stdin, delivered from a thread so a loop never blocks on it.
/// The CLI uses this so Enter ends a preview; the TUI, which owns its
/// terminal, does not.
pub fn watch_stdin() -> Receiver<String> {
    let (sender, receiver) = channel();
    thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut line = String::new();
        // End of input is not a key press: a preview started with stdin
        // closed simply runs for its time.
        if let Ok(read) = stdin.lock().read_line(&mut line)
            && read > 0
        {
            let _ = sender.send(line);
        }
    });
    receiver
}

fn spawn_error(what: &str, error: std::io::Error) -> Error {
    Error::Command {
        command: what.to_string(),
        code: "not started".to_string(),
        stderr: error.to_string(),
        suggestion: "check that it is installed and on PATH".to_string(),
    }
}

fn wait_error(error: std::io::Error) -> Error {
    Error::Command {
        command: "a preview process".to_string(),
        code: "unknown".to_string(),
        stderr: error.to_string(),
        suggestion: "try again".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::testing::RecordingRunner;
    use crate::generate::{AssetSource, fixture};

    fn world() -> (tempfile::TempDir, Layout, AssetSource) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        for tool in ["sddm-greeter-qt6", "omaboot-apply"] {
            let path = root.join("usr/bin").join(tool);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"#!/bin/sh\n").unwrap();
        }
        let layout = Layout::with_dirs(
            Some(root),
            tmp.path().join("config/omaboot"),
            tmp.path().join("state"),
        );
        fixture::theme(&layout.theme_dir("t"), "[meta]\nname = \"T\"\n");
        let assets = AssetSource::at(fixture::omarchy_tree(&tmp.path().join("omarchy")));
        (tmp, layout, assets)
    }

    #[test]
    fn staging_writes_a_preview_theme_that_points_under_run() {
        let (_tmp, layout, assets) = world();
        let runner = RecordingRunner::new();
        let pipeline = Pipeline::new(&layout, &runner).with_assets(assets);
        let staged = stage(&pipeline, "t").unwrap();

        let preview = staged
            .plymouth_preview
            .as_ref()
            .expect("a staged theme is published");
        assert_eq!(staged.plymouth_theme, "omaboot-preview");
        let theme_file = fs::read_to_string(preview.join("omaboot-preview.plymouth")).unwrap();
        assert!(
            theme_file.contains("ImageDir=/run/plymouth/themes/omaboot-preview"),
            "{theme_file}"
        );
        assert!(
            theme_file.contains("ScriptFile=/run/plymouth/themes/omaboot-preview/omaboot.script")
        );
        assert!(
            !preview.join("omaboot.plymouth").exists(),
            "the installed name stays out"
        );
        assert!(preview.join("omaboot.script").is_file());
        assert!(preview.join("logo.png").is_file());
        assert!(staged.sddm.join("Main.qml").is_file());
        // Staging never installs anything.
        assert!(!layout.plymouth_theme_dir().exists());
    }

    #[test]
    fn the_plan_for_the_login_screen_is_the_greeter_in_test_mode() {
        let (_tmp, layout, assets) = world();
        let runner = RecordingRunner::new();
        let pipeline = Pipeline::new(&layout, &runner).with_assets(assets);
        let staged = stage(&pipeline, "t").unwrap();
        let options = Options {
            screen: Screen::Login,
            ..Options::default()
        };
        let lines = plan(&layout, pipeline.tools(), &staged, &options);
        assert!(
            lines[0].contains("sddm-greeter-qt6 --test-mode --theme"),
            "{}",
            lines[0]
        );
        assert!(
            lines.iter().all(|line| !line.contains("sudo")),
            "the greeter needs no root"
        );
    }

    #[test]
    fn the_installed_screens_are_shown_without_staging_or_the_helper() {
        let (_tmp, layout, _assets) = world();
        fs::create_dir_all(layout.plymouthd_conf().parent().unwrap()).unwrap();
        fs::write(layout.plymouthd_conf(), "[Daemon]\nTheme=omarchy\n").unwrap();
        let sddm = layout.sddm_theme_root().join("omarchy-onscreen-keyboard");
        fs::create_dir_all(&sddm).unwrap();
        fs::create_dir_all(layout.sddm_conf_dir()).unwrap();
        fs::write(
            layout
                .sddm_conf_dir()
                .join("99-z-omarchy-onscreen-keyboard.conf"),
            "[Theme]\nCurrent=omarchy-onscreen-keyboard\n",
        )
        .unwrap();

        let staged = Staged::installed(&layout).unwrap();
        assert_eq!(staged.plymouth_theme, "omarchy");
        assert_eq!(staged.plymouth_preview, None);
        assert_eq!(staged.sddm, sddm);

        let tools = Tools::detect(&layout);
        let lines = plan(&layout, &tools, &staged, &Options::default()).join("\n");
        assert!(lines.contains("plymouth.splash=omarchy"), "{lines}");
        assert!(
            lines.contains("the installed Plymouth theme omarchy"),
            "{lines}"
        );
        assert!(!lines.contains("preview install"), "{lines}");
        assert!(!lines.contains("preview remove"), "{lines}");
        let login = Options {
            screen: Screen::Login,
            ..Options::default()
        };
        let lines = plan(&layout, &tools, &staged, &login).join("\n");
        assert!(lines.contains("omarchy-onscreen-keyboard"), "{lines}");
    }

    #[test]
    fn the_installed_screens_cannot_be_shown_when_nothing_is_configured() {
        let (_tmp, layout, _assets) = world();
        let error = Staged::installed(&layout).unwrap_err().to_string();
        assert!(error.contains("names no Plymouth theme"), "{error}");
    }

    #[test]
    fn the_plan_for_plymouth_never_names_an_installed_directory() {
        let (_tmp, layout, assets) = world();
        let runner = RecordingRunner::new();
        let pipeline = Pipeline::new(&layout, &runner).with_assets(assets);
        let staged = stage(&pipeline, "t").unwrap();
        for screen in [Screen::Unlock, Screen::Shutdown] {
            let options = Options {
                screen,
                ..Options::default()
            };
            let lines = plan(&layout, pipeline.tools(), &staged, &options);
            let text = lines.join("\n");
            assert!(text.contains("preview install --staged"), "{text}");
            assert!(text.contains("plymouth.splash=omaboot-preview"), "{text}");
            assert!(text.contains("preview remove"), "{text}");
            assert!(!text.contains("/usr/share/plymouth"), "{text}");
            assert!(!text.contains("/etc/plymouth"), "{text}");
            if screen == Screen::Shutdown {
                assert!(text.contains("--mode=shutdown"), "{text}");
                assert!(!text.contains("ask-for-password"), "{text}");
            } else {
                assert!(text.contains("--mode=boot"), "{text}");
                assert!(text.contains("ask-for-password"), "{text}");
            }
        }
    }

    #[test]
    fn a_prefixed_run_refuses_to_start_daemons() {
        let (_tmp, layout, assets) = world();
        let runner = RecordingRunner::new();
        let pipeline = Pipeline::new(&layout, &runner).with_assets(assets);
        let staged = stage(&pipeline, "t").unwrap();
        let mut said = Vec::new();
        let error = run(
            &layout,
            pipeline.tools(),
            &staged,
            &Options::default(),
            &mut |line| said.push(line.to_string()),
            &mut || false,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("--root"), "{error}");
        assert!(said.is_empty());
    }

    #[test]
    fn a_free_display_number_is_one_without_a_socket() {
        let number = free_display_number().expect("a machine has fewer than 50 X servers");
        assert!((50..100).contains(&number));
    }
}
