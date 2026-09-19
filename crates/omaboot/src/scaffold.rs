//! Creating a theme.
//!
//! Both `omaboot new` and the terminal interface come through here, so a theme
//! made in the TUI and a theme made from the shell are the same theme.

use std::path::PathBuf;

use crate::error::{Error, Result};
use crate::omarchy::{OmarchyThemes, Palette};
use crate::paths::Layout;
use crate::state;
use crate::system::{Derived, Snapshot};
use crate::theme::MANIFEST;

/// What a new theme starts from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The built-in defaults, and no logo: the user brings their own.
    Blank,
    /// An Omarchy theme's colours and its unlock.png.
    OmarchyTheme(String),
    /// The screens that boot now: the installed Plymouth theme's colours
    /// and logo, or, when omaboot installed them, a copy of the theme they
    /// came from.
    Current,
}

/// What was created, so the caller can say what happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    pub dir: PathBuf,
    /// The image copied in as logo.png, if one was.
    pub logo_from: Option<PathBuf>,
}

/// Check a name before anything is written.
pub fn check_name(layout: &Layout, name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(Error::Environment {
            what: "a theme needs a name".to_string(),
            suggestion: "type one, for example my-theme".to_string(),
        });
    }
    if trimmed
        .chars()
        .any(|c| c.is_control() || c == '/' || c == '\\')
    {
        return Err(Error::Environment {
            what: format!("{trimmed} cannot be a directory name"),
            suggestion: "use letters, digits, dashes and dots".to_string(),
        });
    }
    if trimmed == "." || trimmed == ".." {
        return Err(Error::Environment {
            what: format!("{trimmed} cannot be a directory name"),
            suggestion: "pick a name".to_string(),
        });
    }
    if layout.theme_dir(trimmed).exists() {
        return Err(Error::Environment {
            what: format!("{} already exists", layout.theme_dir(trimmed).display()),
            suggestion: "pick another name, or edit the theme that is there".to_string(),
        });
    }
    Ok(())
}

/// Create a theme directory. Everything that can fail is resolved first, so a
/// failure leaves no half-made theme behind.
pub fn create(layout: &Layout, name: &str, source: &Source) -> Result<Created> {
    let name = name.trim();
    check_name(layout, name)?;

    // Everything to write, resolved before the directory exists.
    let (manifest, files): (String, Vec<(PathBuf, String)>) = match source {
        Source::Blank => (manifest_text(name, None), Vec::new()),
        Source::OmarchyTheme(theme) => {
            let themes = OmarchyThemes::discover(layout);
            let palette = themes.palette(theme)?;
            let unlock = themes.unlock_image(theme)?;
            (
                manifest_text(name, Some(&palette)),
                vec![(unlock, "logo.png".to_string())],
            )
        }
        Source::Current => {
            let derived = Snapshot::read(layout).derive(name)?;
            let text =
                toml::to_string_pretty(&derived.manifest).map_err(|error| Error::Environment {
                    what: format!("the derived theme could not be written as TOML: {error}"),
                    suggestion: "this is a bug; report it".to_string(),
                })?;
            (text, derived.files)
        }
    };
    let mut loaded = Vec::with_capacity(files.len());
    for (from, to) in files {
        let bytes = std::fs::read(&from).map_err(|source| Error::read(from.clone(), source))?;
        loaded.push((from, to, bytes));
    }

    let dir = layout.theme_dir(name);
    std::fs::create_dir_all(&dir).map_err(|source| Error::write(dir.clone(), source))?;
    state::write_atomic(&dir.join(MANIFEST), manifest.as_bytes())?;

    let mut logo_from = None;
    for (from, to, bytes) in loaded {
        state::write_atomic(&dir.join(&to), &bytes)?;
        if to == "logo.png" {
            logo_from = Some(from);
        }
    }

    Ok(Created { dir, logo_from })
}

/// What the current screens would give a new theme, without writing it.
pub fn describe_current(layout: &Layout, name: &str) -> Result<Derived> {
    Snapshot::read(layout).derive(name)
}

/// The manifest a new theme starts from.
pub fn manifest_text(name: &str, palette: Option<&Palette>) -> String {
    let default = Palette::fallback();
    let palette = palette.unwrap_or(&default);
    let background = &palette.background;
    let foreground = &palette.foreground;
    let accent = &palette.accent;
    let error = &palette.error;
    format!(
        r##"[meta]
name = "{name}"
author = ""
version = "0.1.0"

[colors]
background = "{background}"
foreground = "{foreground}"
accent = "{accent}"
error = "{error}"

[logo]
source = "logo.png"
width = 0.42
position = "center"

[unlock]
prompt = "bullets"
progress = "bar"
message = ""

[shutdown]
logo = "inherit"
message = ""
progress = "spinner"

[login]
layout = "centered"
clock = true
show_session_picker = false
background = "color"
"##
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn world() -> (tempfile::TempDir, Layout) {
        let tmp = tempfile::tempdir().unwrap();
        let layout = Layout::with_dirs(
            Some(tmp.path().join("root")),
            tmp.path().join("config/omaboot"),
            tmp.path().join("state"),
        );
        let dir = tmp.path().join("root/usr/share/omarchy/themes/nord");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("colors.toml"),
            "background = \"#2e3440\"\nforeground = \"#d8dee9\"\naccent = \"#88c0d0\"\nred = \"#bf616a\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("unlock.png"), b"nord unlock image").unwrap();
        (tmp, layout)
    }

    #[test]
    fn a_theme_from_an_omarchy_theme_is_complete_on_its_own() {
        let (_tmp, layout) = world();
        let created = create(&layout, "mine", &Source::OmarchyTheme("nord".to_string())).unwrap();
        assert!(created.logo_from.is_some());

        let theme = Theme::load(&created.dir).expect("it must validate unaided");
        assert_eq!(theme.manifest().colors.background, "#2e3440");
        assert_eq!(std::fs::read(theme.logo()).unwrap(), b"nord unlock image");
    }

    #[test]
    fn a_blank_theme_parses_but_waits_for_a_logo() {
        let (_tmp, layout) = world();
        let created = create(&layout, "blank", &Source::Blank).unwrap();
        assert!(created.logo_from.is_none());
        Theme::read(&created.dir).expect("the manifest parses");
        let error = Theme::load(&created.dir).unwrap_err().to_string();
        assert!(error.contains("logo"), "{error}");
    }

    #[test]
    fn a_name_that_cannot_be_a_directory_is_refused_before_anything_is_written() {
        let (_tmp, layout) = world();
        for name in ["", "   ", "with/slash", ".", ".."] {
            assert!(
                check_name(&layout, name).is_err(),
                "{name:?} should be refused"
            );
        }
        assert!(!layout.themes_dir().exists(), "nothing was created");
    }

    #[test]
    fn an_existing_name_is_refused() {
        let (_tmp, layout) = world();
        create(&layout, "mine", &Source::Blank).unwrap();
        let error = create(&layout, "mine", &Source::Blank)
            .unwrap_err()
            .to_string();
        assert!(error.contains("already exists"), "{error}");
    }

    #[test]
    fn a_theme_from_the_current_screens_is_complete_on_its_own() {
        let (tmp, layout) = world();
        std::fs::create_dir_all(layout.plymouthd_conf().parent().unwrap()).unwrap();
        std::fs::write(layout.plymouthd_conf(), "[Daemon]\nTheme=omarchy\n").unwrap();
        let installed = layout.plymouth_theme_root().join("omarchy");
        std::fs::create_dir_all(&installed).unwrap();
        std::fs::write(
            installed.join("omarchy.script"),
            "Window.SetBackgroundTopColor(0.180, 0.204, 0.251);\n",
        )
        .unwrap();
        std::fs::write(
            installed.join("bullet.png"),
            crate::generate::fixture::png(4, 4, [0xd8, 0xde, 0xe9, 255]),
        )
        .unwrap();
        std::fs::write(
            installed.join("logo.png"),
            crate::generate::fixture::png(30, 10, [255, 255, 255, 255]),
        )
        .unwrap();
        let _ = tmp;

        let created = create(&layout, "now", &Source::Current).unwrap();
        assert!(created.logo_from.unwrap().ends_with("omarchy/logo.png"));
        let theme = Theme::load(&created.dir).expect("it must validate unaided");
        assert_eq!(theme.manifest().colors.background, "#2e3440");
        assert_eq!(theme.manifest().colors.foreground, "#d8dee9");
    }

    #[test]
    fn the_current_screens_cannot_be_copied_when_nothing_is_installed() {
        let (_tmp, layout) = world();
        assert!(create(&layout, "now", &Source::Current).is_err());
        assert!(!layout.theme_dir("now").exists(), "nothing was created");
    }

    #[test]
    fn an_unknown_omarchy_theme_leaves_nothing_behind() {
        let (_tmp, layout) = world();
        let error = create(&layout, "mine", &Source::OmarchyTheme("nope".to_string()))
            .unwrap_err()
            .to_string();
        assert!(error.contains("nord"), "{error}");
        assert!(!layout.theme_dir("mine").exists());
    }
}
