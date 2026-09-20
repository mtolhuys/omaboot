//! The SDDM greeter under `--test-mode`: one command for the preview and the
//! smoke test, and one judgement of what it said.
//!
//! In test mode the greeter shows a theme in a window and runs until that
//! window is closed. It does not exit to say the theme is fine, and, at the
//! SDDM releases read so far, it does not exit to say the theme is broken
//! either: a `Main.qml` that fails to load is reported on stderr and the
//! greeter carries on with its embedded fallback theme. So the smoke test
//! gives the greeter a settling time, requires it to still be running when
//! that is up, stops it, and reads what it wrote: silence about the theme is
//! a pass; an exit before the time is up, whatever the code, or a complaint
//! about the theme, is a fail.
//!
//! What it wrote goes to stderr only when asked. SDDM 0.21.0 logs through
//! Qt's own handler, and Qt sends that to journald when stderr is not a
//! console, so a greeter run from the engine says nothing on stderr at all
//! (verified on the installed 0.21.0-7, 20 September 2026: the broken theme's
//! errors were in `journalctl --user` and the pipe was empty). Both commands
//! here therefore set `QT_FORCE_STDERR_LOGGING=1`, under which Qt writes
//! every message to stderr whatever it is attached to. The facts this leans
//! on carry a date in `docs/UPSTREAM.md`.

use std::path::Path;
use std::time::Duration;

use crate::exec::{CommandOutput, CommandSpec, Greeter};

/// How long the greeter gets to load the theme and complain before the smoke
/// test reads its verdict. Loading takes a second or two on the reference
/// machine; the margin is for a slower one.
pub const SETTLE: Duration = Duration::from_secs(10);

/// The greeter on this theme, in a window, saying on stderr what it thinks
/// of the theme: what the preview runs.
pub fn test_mode(greeter: &Greeter, theme_dir: &Path) -> CommandSpec {
    CommandSpec::new(greeter.path.display().to_string())
        .arg("--test-mode")
        .args(["--theme", &theme_dir.display().to_string()])
        .env("QT_FORCE_STDERR_LOGGING", "1")
}

/// The greeter on this theme, offscreen, required to stay up for `SETTLE`:
/// what the apply pipeline runs before anything privileged.
pub fn smoke_test(greeter: &Greeter, theme_dir: &Path) -> CommandSpec {
    test_mode(greeter, theme_dir)
        .env("QT_QPA_PLATFORM", "offscreen")
        .stays_up(SETTLE)
}

/// Why the greeter did not pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// What happened, as a clause after the command: "exited after ... " or
    /// "stayed up but complained about the theme".
    pub what: String,
    /// The lines that show it, joined for one sentence.
    pub said: String,
}

/// The lines on the greeter's stderr that mean the theme did not load.
///
/// On SDDM 0.21.0 a theme that fails to load produces the QML messages,
/// each `file://<the file under the theme directory>:<line>:<column>: <what>`
/// (a syntax error, or an image the theme names that cannot be opened),
/// and then `Fallback to embedded theme`; the greeter then stays up on its
/// built-in theme, so that line alone is a fail. The healthy greeter names
/// the theme too, `Loading file://.../Main.qml...`, with no position after
/// the file, and that is not a complaint. Older SDDM releases logged through
/// their own handler with `(WW)`, `(EE)` and `(FF)` severity tags; those are
/// still read. A message about a file that is not the theme's (the
/// `SddmComponents` the theme imports, say) is the greeter's own and is let
/// through, unless it is an error.
pub fn complaints(stderr: &str, theme_dir: &Path) -> Vec<String> {
    let theme_dir = theme_dir.display().to_string();
    stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            line.contains("(EE)")
                || line.contains("(FF)")
                || lower.contains("fallback to embedded theme")
                || (line.contains(&theme_dir)
                    && (line.contains("(WW)")
                        || lower.contains("error")
                        || positioned(line, &theme_dir)))
        })
        .map(str::to_string)
        .collect()
}

/// Whether `line` names a file under `theme_dir` followed by a
/// `:<line>:<column>:` position, which is how Qt reports a QML problem in
/// that file.
fn positioned(line: &str, theme_dir: &str) -> bool {
    let Some(start) = line.find(theme_dir) else {
        return false;
    };
    let rest = &line[start + theme_dir.len()..];
    // The rest of the path, then the position.
    let Some(after_file) = rest.find(':') else {
        return false;
    };
    let mut fields = rest[after_file + 1..].splitn(3, ':');
    let is_number = |field: Option<&str>| {
        field.is_some_and(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()))
    };
    is_number(fields.next()) && is_number(fields.next()) && fields.next().is_some()
}

/// The smoke test's verdict on what the greeter did.
pub fn judge(output: &CommandOutput, theme_dir: &Path) -> std::result::Result<(), Refusal> {
    if !output.stayed_up {
        return Err(Refusal {
            what: format!(
                "exited with {} within {} seconds instead of staying up",
                output.describe_code(),
                SETTLE.as_secs()
            ),
            said: last_lines(&output.stderr, 5),
        });
    }
    let complaints = complaints(&output.stderr, theme_dir);
    if complaints.is_empty() {
        return Ok(());
    }
    Err(Refusal {
        what: "stayed up but complained about the theme".to_string(),
        said: complaints
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(" / "),
    })
}

fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return "(no output)".to_string();
    }
    let start = lines.len().saturating_sub(count);
    lines[start..].join(" / ")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn stage() -> PathBuf {
        PathBuf::from("/home/me/.local/state/omaboot/stage/sddm")
    }

    fn greeter() -> Greeter {
        Greeter {
            name: "sddm-greeter-qt6".to_string(),
            path: PathBuf::from("/usr/bin/sddm-greeter-qt6"),
        }
    }

    #[test]
    fn the_smoke_test_is_the_preview_command_offscreen_and_required_to_stay_up() {
        let spec = smoke_test(&greeter(), &stage());
        assert_eq!(
            spec.to_string(),
            "QT_FORCE_STDERR_LOGGING=1 QT_QPA_PLATFORM=offscreen /usr/bin/sddm-greeter-qt6 --test-mode --theme /home/me/.local/state/omaboot/stage/sddm"
        );
        assert_eq!(spec.verdict, crate::exec::Verdict::StaysUp);
        assert_eq!(spec.timeout, Some(SETTLE));
        assert_eq!(
            test_mode(&greeter(), &stage()).to_string(),
            "QT_FORCE_STDERR_LOGGING=1 /usr/bin/sddm-greeter-qt6 --test-mode --theme /home/me/.local/state/omaboot/stage/sddm"
        );
    }

    // What sddm-greeter-qt6 0.21.0-7 wrote with QT_FORCE_STDERR_LOGGING=1 on
    // the reference machine, 20 September 2026
    // (docs/evidence/closing-round-2026-09-20/greeter/), paths shortened.
    const SDDM_0_21_HEALTHY: &str = "High-DPI autoscaling not Enabled\n\
        Reading from \"/usr/share/wayland-sessions/hyprland.desktop\"\n\
        Loading theme configuration from \"/home/me/.local/state/omaboot/stage/sddm/theme.conf\"\n\
        Socket error:  \"QLocalSocket::connectToServer: Invalid name\"\n\
        Loading file:///home/me/.local/state/omaboot/stage/sddm/Main.qml...\n\
        Adding view for \"\" QRect(0,0 800x800)\n";

    const SDDM_0_21_BROKEN: &str = "Loading theme configuration from \"/home/me/.local/state/omaboot/stage/sddm/theme.conf\"\n\
        Socket error:  \"QLocalSocket::connectToServer: Invalid name\"\n\
        Loading file:///home/me/.local/state/omaboot/stage/sddm/Main.qml...\n\
        file:///home/me/.local/state/omaboot/stage/sddm/Main.qml:12:44: Expected a qualified name id \n\
               readonly property color foregroundColor: \"#bebebe\"  {{ broken on purpose \n\
                                                        ^\n\
        Fallback to embedded theme\n\
        qt.qml.propertyCache.append: Member enabled of the object ImageButton_QMLTYPE_17 overrides a member of the base object. Consider renaming it or adding final or override specifier\n\
        file:///usr/lib/qt6/qml/SddmComponents/LayoutBox.qml:35:5: QML Connections: Implicitly defined onFoo properties in Connections are deprecated. Use this syntax instead: function onFoo(<arguments>) { ... }\n\
        file:///usr/lib/qt6/qml/SddmComponents/ComboBox.qml:105:9: QML Image: Cannot open: file:///usr/lib/qt6/qml/SddmComponents/angle-down.png\n\
        qrc:/theme/Main.qml:41:5: QML Connections: Implicitly defined onFoo properties in Connections are deprecated. Use this syntax instead: function onFoo(<arguments>) { ... }\n\
        Adding view for \"\" QRect(0,0 800x800)\n";

    #[test]
    fn sddm_0_21_healthy_output_passes_and_the_socket_error_is_not_about_the_theme() {
        assert!(complaints(SDDM_0_21_HEALTHY, &stage()).is_empty());
        assert_eq!(
            judge(&CommandOutput::stays_up_saying(SDDM_0_21_HEALTHY), &stage()),
            Ok(())
        );
    }

    #[test]
    fn sddm_0_21_broken_output_is_named_by_the_positioned_qml_error_and_the_fallback() {
        let found = complaints(SDDM_0_21_BROKEN, &stage());
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found[0].starts_with("file:///home/me/.local/state/omaboot/stage/sddm/Main.qml:12:44:"),
            "{found:?}"
        );
        assert_eq!(found[1], "Fallback to embedded theme");
        let refusal =
            judge(&CommandOutput::stays_up_saying(SDDM_0_21_BROKEN), &stage()).unwrap_err();
        assert!(
            refusal.said.contains("Expected a qualified name id"),
            "{}",
            refusal.said
        );
    }

    #[test]
    fn an_image_of_the_theme_that_cannot_be_opened_is_a_complaint() {
        let stderr = "file:///home/me/.local/state/omaboot/stage/sddm/Main.qml:80:3: QML Image: Cannot open: file:///home/me/.local/state/omaboot/stage/sddm/logo.png\n";
        assert_eq!(complaints(stderr, &stage()).len(), 1);
    }

    #[test]
    fn a_greeter_that_loads_the_theme_and_only_informs_passes() {
        let stderr = "[12:00:00.001] (II) GREETER: Loading theme configuration from \"/home/me/.local/state/omaboot/stage/sddm/theme.conf\"\n\
                      [12:00:00.002] (II) GREETER: Connected to the daemon.\n\
                      [12:00:00.400] (WW) GREETER: QML Connections: Implicitly defined onFoo properties in Connections are deprecated. qrc:/qt/qml/Something.qml:12\n";
        assert!(complaints(stderr, &stage()).is_empty());
        assert_eq!(
            judge(&CommandOutput::stays_up_saying(stderr), &stage()),
            Ok(())
        );
    }

    #[test]
    fn a_theme_that_fails_to_load_is_named_by_its_qml_errors_and_the_fallback() {
        let stderr = "[12:00:00.001] (II) GREETER: Loading theme configuration from \"/home/me/.local/state/omaboot/stage/sddm/theme.conf\"\n\
                      [12:00:00.300] (WW) GREETER: file:///home/me/.local/state/omaboot/stage/sddm/Main.qml:12:5: Expected token `}`\n\
                      [12:00:00.301] (WW) GREETER: Fallback to embedded theme\n";
        let found = complaints(stderr, &stage());
        assert_eq!(found.len(), 2, "{found:?}");
        let refusal = judge(&CommandOutput::stays_up_saying(stderr), &stage()).unwrap_err();
        assert_eq!(refusal.what, "stayed up but complained about the theme");
        assert!(refusal.said.contains("Expected token"), "{}", refusal.said);
        assert!(refusal.said.contains("Fallback"), "{}", refusal.said);
    }

    #[test]
    fn an_error_line_is_a_complaint_wherever_it_points() {
        let stderr = "[12:00:00.001] (EE) GREETER: Failed to create the OpenGL context\n";
        assert_eq!(complaints(stderr, &stage()).len(), 1);
    }

    #[test]
    fn an_exit_before_the_settling_time_fails_whatever_the_code() {
        let refusal = judge(&CommandOutput::failure(0, ""), &stage()).unwrap_err();
        assert!(
            refusal.what.starts_with("exited with exit code 0 within"),
            "{}",
            refusal.what
        );
        assert_eq!(refusal.said, "(no output)");

        let refusal = judge(
            &CommandOutput::failure(1, "one\ntwo\nthree\nfour\nfive\nsix"),
            &stage(),
        )
        .unwrap_err();
        assert!(refusal.what.contains("exit code 1"));
        assert_eq!(refusal.said, "two / three / four / five / six");
    }

    #[test]
    fn a_greeter_that_stays_up_in_silence_passes() {
        assert_eq!(judge(&CommandOutput::stays_up(), &stage()), Ok(()));
    }
}
