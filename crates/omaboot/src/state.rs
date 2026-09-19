//! Recorded state: what is applied, and what to go back to.
//!
//! Both files live under `~/.local/state/omaboot/`, are TOML, and are written
//! atomically. The rollback point is written before anything switches, which
//! is the whole reason it exists.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths::Layout;

/// Bumped when the on-disk shape changes. A file from a future version is
/// refused rather than misread.
pub const STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstalledFile {
    /// Absolute path on the running system, prefix included when one is used.
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

/// What is installed right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppliedState {
    pub version: u32,
    pub theme: String,
    pub theme_hash: String,
    pub applied_at_unix: i64,
    #[serde(default)]
    pub files: Vec<InstalledFile>,
}

/// What the system looked like before the last apply switched anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollbackPoint {
    pub version: u32,
    pub recorded_at_unix: i64,
    /// The Plymouth theme that was default before, if one could be read.
    pub previous_plymouth_theme: Option<String>,
    /// Whether the SDDM drop-in already existed, which decides whether revert
    /// restores it or removes it.
    pub sddm_dropin_existed: bool,
    pub previous_sddm_dropin: Option<String>,
    /// The theme this rollback point was recorded for.
    pub theme: String,
    pub theme_hash: String,
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

/// "just now", "3h ago", "2d ago". Deliberately coarse: the state file holds
/// the exact second, and a boot screen tool never needs more than this.
pub fn describe_age(then_unix: i64, now_unix: i64) -> String {
    let seconds = (now_unix - then_unix).max(0);
    match seconds {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m ago", seconds / 60),
        3600..=86399 => format!("{}h ago", seconds / 3600),
        _ => format!("{}d ago", seconds / 86400),
    }
}

pub fn read_applied(layout: &Layout) -> Result<Option<AppliedState>> {
    read_toml::<AppliedState>(&layout.applied_state_file())
        .map(|state| state.filter(|s| s.version <= STATE_VERSION))
}

pub fn read_rollback(layout: &Layout) -> Result<Option<RollbackPoint>> {
    read_toml::<RollbackPoint>(&layout.rollback_file())
}

pub fn require_rollback(layout: &Layout) -> Result<RollbackPoint> {
    read_rollback(layout)?.ok_or_else(|| Error::NoRollbackPoint {
        path: layout.rollback_file(),
    })
}

fn read_toml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(Error::read(path.to_path_buf(), source)),
    };
    let value = toml::from_str(&text).map_err(|source| Error::ParseToml {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(Some(value))
}

pub fn serialize_applied(state: &AppliedState) -> String {
    toml::to_string_pretty(state).expect("applied state is always serialisable")
}

pub fn serialize_rollback(point: &RollbackPoint) -> String {
    toml::to_string_pretty(point).expect("rollback point is always serialisable")
}

/// Write a file by writing a sibling and renaming it, so a reader never sees
/// half a state file and a crash never leaves one.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write as _;

    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|source| Error::write(parent.to_path_buf(), source))?;

    let temporary = parent.join(format!(
        ".{}.new",
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "omaboot".to_string())
    ));
    {
        let mut file = fs::File::create(&temporary)
            .map_err(|source| Error::write(temporary.clone(), source))?;
        file.write_all(bytes)
            .map_err(|source| Error::write(temporary.clone(), source))?;
        file.sync_all()
            .map_err(|source| Error::write(temporary.clone(), source))?;
    }
    fs::rename(&temporary, path).map_err(|source| Error::write(path.to_path_buf(), source))?;
    Ok(())
}

/// The Plymouth theme that is default right now, read from
/// `/etc/plymouth/plymouthd.conf`.
///
/// Reading the file rather than running `plymouth-set-default-theme` keeps the
/// rollback point recordable under a prefix and in a dry run.
pub fn current_plymouth_theme(layout: &Layout) -> Option<String> {
    let text = fs::read_to_string(layout.plymouthd_conf()).ok()?;
    parse_plymouthd_theme(&text)
}

pub fn parse_plymouthd_theme(text: &str) -> Option<String> {
    let mut in_daemon = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') {
            in_daemon = line.eq_ignore_ascii_case("[daemon]");
            continue;
        }
        if !in_daemon {
            continue;
        }
        if let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("theme")
        {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Rewrite `Theme=` in a plymouthd.conf body, adding the section or the key if
/// they are missing. Used only under a prefix, where
/// `plymouth-set-default-theme` must not run.
pub fn set_plymouthd_theme(text: &str, theme: &str) -> String {
    let mut out = String::with_capacity(text.len() + 32);
    let mut in_daemon = false;
    let mut replaced = false;
    let mut saw_daemon = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_daemon = trimmed.eq_ignore_ascii_case("[daemon]");
            if in_daemon {
                saw_daemon = true;
            }
        } else if in_daemon
            && !replaced
            && let Some((key, _)) = trimmed.split_once('=')
            && key.trim().eq_ignore_ascii_case("theme")
        {
            out.push_str(&format!("Theme={theme}\n"));
            replaced = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }

    if !saw_daemon {
        out.push_str("[Daemon]\n");
        out.push_str(&format!("Theme={theme}\n"));
    } else if !replaced {
        // The section exists but has no Theme key; append one to the end of it.
        let mut rebuilt = String::with_capacity(out.len() + 32);
        let mut inserted = false;
        let mut inside = false;
        for line in out.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                if inside && !inserted {
                    rebuilt.push_str(&format!("Theme={theme}\n"));
                    inserted = true;
                }
                inside = trimmed.eq_ignore_ascii_case("[daemon]");
            }
            rebuilt.push_str(line);
            rebuilt.push('\n');
        }
        if !inserted {
            rebuilt.push_str(&format!("Theme={theme}\n"));
        }
        out = rebuilt;
    }

    out
}

/// The single line omaboot writes into `/etc/sddm.conf.d/zz-omaboot.conf`.
pub fn sddm_dropin_contents(theme: &str) -> String {
    format!("[Theme]\nCurrent={theme}\n")
}

/// Read the drop-in, if it is there.
pub fn read_sddm_dropin(layout: &Layout) -> Option<String> {
    fs::read_to_string(layout.sddm_dropin()).ok()
}

/// The login theme SDDM will actually use, and the file that decides it.
///
/// SDDM reads `/etc/sddm.conf`, then every file in `/etc/sddm.conf.d` in
/// file-name order, and the last `[Theme] Current=` wins. Reading it the same
/// way is the only honest answer to "what is my login screen", and it is what
/// the verify step checks after a switch: that omaboot's drop-in is the one
/// that won.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SddmTheme {
    pub name: String,
    pub decided_by: PathBuf,
}

pub fn effective_sddm_theme(layout: &Layout) -> Option<SddmTheme> {
    resolve_sddm_theme(&layout.sddm_conf(), &layout.sddm_conf_dir())
}

/// The same resolution for explicit paths, which is what an operation in the
/// pipeline carries.
pub fn resolve_sddm_theme(conf: &Path, conf_dir: &Path) -> Option<SddmTheme> {
    let mut files = vec![conf.to_path_buf()];
    if let Ok(entries) = fs::read_dir(conf_dir) {
        let mut dropins: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_file() && path.extension().is_some_and(|e| e == "conf"))
            .collect();
        dropins.sort();
        files.extend(dropins);
    }

    let mut winner = None;
    for file in files {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        if let Some(name) = parse_sddm_current(&text) {
            winner = Some(SddmTheme {
                name,
                decided_by: file,
            });
        }
    }
    winner
}

/// `Current=` under `[Theme]`, or nothing.
pub fn parse_sddm_current(text: &str) -> Option<String> {
    let mut in_theme = false;
    let mut current = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            in_theme = line.eq_ignore_ascii_case("[theme]");
            continue;
        }
        if in_theme
            && let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("current")
        {
            let value = value.trim();
            if !value.is_empty() {
                current = Some(value.to_string());
            }
        }
    }
    current
}

pub fn state_paths(layout: &Layout) -> Vec<PathBuf> {
    vec![layout.applied_state_file(), layout.rollback_file()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(root: &Path) -> Layout {
        Layout::with_dirs(
            Some(root.to_path_buf()),
            root.join("config"),
            root.join("state"),
        )
    }

    #[test]
    fn age_is_described_coarsely() {
        assert_eq!(describe_age(100, 130), "just now");
        assert_eq!(describe_age(0, 60 * 5), "5m ago");
        assert_eq!(describe_age(0, 3600 * 3), "3h ago");
        assert_eq!(describe_age(0, 86400 * 2 + 5), "2d ago");
        // A clock that went backwards must not produce a negative age.
        assert_eq!(describe_age(500, 100), "just now");
    }

    #[test]
    fn plymouthd_theme_is_read_from_the_daemon_section_only() {
        let text = "[Other]\nTheme=decoy\n\n[Daemon]\n# comment\nTheme=omarchy\nShowDelay=0\n";
        assert_eq!(parse_plymouthd_theme(text).as_deref(), Some("omarchy"));
        assert_eq!(parse_plymouthd_theme("[Daemon]\nShowDelay=0\n"), None);
        assert_eq!(parse_plymouthd_theme(""), None);
    }

    #[test]
    fn setting_the_theme_replaces_the_existing_key() {
        let text = "[Daemon]\nTheme=omarchy\nShowDelay=0\n";
        let out = set_plymouthd_theme(text, "omaboot");
        assert_eq!(parse_plymouthd_theme(&out).as_deref(), Some("omaboot"));
        assert!(out.contains("ShowDelay=0"));
        assert_eq!(out.matches("Theme=").count(), 1);
    }

    #[test]
    fn setting_the_theme_creates_the_section_when_there_is_none() {
        let out = set_plymouthd_theme("", "omaboot");
        assert_eq!(parse_plymouthd_theme(&out).as_deref(), Some("omaboot"));
    }

    #[test]
    fn setting_the_theme_adds_the_key_to_an_existing_section() {
        let out = set_plymouthd_theme("[Daemon]\nShowDelay=0\n", "omaboot");
        assert_eq!(parse_plymouthd_theme(&out).as_deref(), Some("omaboot"));
    }

    #[test]
    fn the_effective_sddm_theme_is_the_last_drop_in_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = layout(tmp.path());
        let dir = layout.sddm_conf_dir();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("10-theme.conf"), "[Theme]\nCurrent=omarchy\n").unwrap();
        fs::write(
            dir.join("99-omarchy-login.conf"),
            "[Theme]\nCurrent=omarchy\n\n[Users]\nRememberLastUser=true\n",
        )
        .unwrap();
        fs::write(
            dir.join("99-z-omarchy-onscreen-keyboard.conf"),
            "[Theme]\nCurrent=omarchy-onscreen-keyboard\n",
        )
        .unwrap();
        fs::write(dir.join("autologin.conf"), "[Autologin]\nUser=me\n").unwrap();

        let theme = effective_sddm_theme(&layout).unwrap();
        assert_eq!(theme.name, "omarchy-onscreen-keyboard");
        assert!(
            theme
                .decided_by
                .ends_with("99-z-omarchy-onscreen-keyboard.conf")
        );

        // omaboot's drop-in has to win against exactly that file.
        fs::write(layout.sddm_dropin(), sddm_dropin_contents("omaboot")).unwrap();
        let theme = effective_sddm_theme(&layout).unwrap();
        assert_eq!(theme.name, "omaboot");
        assert!(theme.decided_by.ends_with("zz-omaboot.conf"));
    }

    #[test]
    fn a_90_prefixed_drop_in_would_have_lost() {
        // The reason the file is not called 90-omaboot.conf.
        let mut names = [
            "90-omaboot.conf",
            "99-z-omarchy-onscreen-keyboard.conf",
            "zz-omaboot.conf",
        ];
        names.sort();
        assert_eq!(names.last().copied(), Some("zz-omaboot.conf"));
        assert_eq!(names[0], "90-omaboot.conf");
    }

    #[test]
    fn sddm_current_is_read_from_the_theme_section_only() {
        assert_eq!(
            parse_sddm_current("[General]\nCurrent=decoy\n[Theme]\nCurrent=maya\n").as_deref(),
            Some("maya")
        );
        assert_eq!(
            parse_sddm_current("[Theme]\n# Current=old\nCurrent=\n"),
            None
        );
    }

    #[test]
    fn a_state_file_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = layout(tmp.path());
        let state = AppliedState {
            version: STATE_VERSION,
            theme: "nord".to_string(),
            theme_hash: "abc".to_string(),
            applied_at_unix: 1_700_000_000,
            files: vec![InstalledFile {
                path: "/usr/share/plymouth/themes/omaboot/logo.png".to_string(),
                sha256: "def".to_string(),
                bytes: 12,
            }],
        };
        write_atomic(
            &layout.applied_state_file(),
            serialize_applied(&state).as_bytes(),
        )
        .unwrap();
        assert_eq!(read_applied(&layout).unwrap().unwrap(), state);
    }

    #[test]
    fn a_missing_rollback_point_says_what_to_do_instead() {
        let tmp = tempfile::tempdir().unwrap();
        let error = require_rollback(&layout(tmp.path()))
            .unwrap_err()
            .to_string();
        assert!(error.contains("omaboot reset"), "{error}");
    }

    #[test]
    fn a_corrupt_state_file_names_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = layout(tmp.path());
        write_atomic(&layout.applied_state_file(), b"this is not toml {{").unwrap();
        let error = read_applied(&layout).unwrap_err().to_string();
        assert!(error.contains("applied.toml"), "{error}");
    }

    #[test]
    fn an_unknown_key_in_state_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = layout(tmp.path());
        write_atomic(
            &layout.rollback_file(),
            b"version = 1\nrecorded_at_unix = 1\nsddm_dropin_existed = false\ntheme = \"x\"\ntheme_hash = \"y\"\nsurprise = 1\n",
        )
        .unwrap();
        assert!(read_rollback(&layout).is_err());
    }

    #[test]
    fn writing_is_atomic_and_leaves_no_temporary_behind() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("nested/deep/file.toml");
        write_atomic(&target, b"a = 1\n").unwrap();
        write_atomic(&target, b"a = 2\n").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "a = 2\n");
        let leftovers: Vec<_> = fs::read_dir(target.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with('.'))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }
}
