//! Reading the Omarchy tree.
//!
//! Everything here is read-only. omaboot borrows two things from an Omarchy
//! theme when scaffolding: its palette from `colors.toml`, and its `unlock.png`
//! as a starting logo. Both are read the way `omarchy-plymouth-set-by-theme`
//! reads them, so a theme that works there works here.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::paths::Layout;
use crate::theme::Rgb;

/// Where the packaged tree lives when `$OMARCHY_PATH` is unset.
pub const DEFAULT_OMARCHY_PATH: &str = "/usr/share/omarchy";

/// The file every Omarchy theme carries its colours in.
pub const COLORS: &str = "colors.toml";
/// The image `omarchy-plymouth-set-by-theme` installs as the boot logo.
pub const UNLOCK: &str = "unlock.png";

/// Where the Omarchy tree is for this run.
///
/// Under a `--root` prefix the tree is `<prefix>/usr/share/omarchy`, whatever
/// the environment says: a sandboxed run reads nothing outside its prefix, and
/// a test is then hermetic in a login shell that sets `OMARCHY_PATH` (Omarchy
/// sets it for every session, and `omarchy dev link` points it at a checkout).
/// Without a prefix, `$OMARCHY_PATH` wins over the packaged location, the way
/// every Omarchy script reads it.
pub fn omarchy_path(layout: &Layout) -> PathBuf {
    omarchy_path_from(layout, std::env::var_os("OMARCHY_PATH"))
}

/// `omarchy_path` with the environment passed in, so the rule can be tested
/// without touching the process environment.
fn omarchy_path_from(layout: &Layout, env: Option<std::ffi::OsString>) -> PathBuf {
    if layout.is_prefixed() {
        return layout.system(DEFAULT_OMARCHY_PATH);
    }
    match env {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => PathBuf::from(DEFAULT_OMARCHY_PATH),
    }
}

/// The four colours omaboot takes from an Omarchy theme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    pub background: String,
    pub foreground: String,
    pub accent: String,
    pub error: String,
}

impl Palette {
    /// The palette a theme gets when it is scaffolded from nothing. It is the
    /// one `docs/SPEC.md` sketches, and it matches the stock Omarchy colours.
    pub fn fallback() -> Self {
        Self {
            background: "#1a1b26".to_string(),
            foreground: "#c0caf5".to_string(),
            accent: "#7aa2f7".to_string(),
            error: "#f7768e".to_string(),
        }
    }
}

/// The two places an Omarchy theme can live, in the order
/// `omarchy-theme-dir` prefers them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmarchyThemes {
    user: PathBuf,
    packaged: PathBuf,
    /// How the packaged location was chosen, for the sentence that says no
    /// theme was found there.
    prefixed: bool,
}

impl OmarchyThemes {
    pub fn discover(layout: &Layout) -> Self {
        Self {
            user: layout.config_base().join("omarchy/themes"),
            packaged: omarchy_path(layout).join("themes"),
            prefixed: layout.is_prefixed(),
        }
    }

    /// A user-installed theme wins over the packaged one of the same name,
    /// which is what `omarchy-theme-dir` does.
    pub fn dir(&self, name: &str) -> Result<PathBuf> {
        let candidate = self.user.join(name);
        if candidate.is_dir() {
            return Ok(candidate);
        }
        let candidate = self.packaged.join(name);
        if candidate.is_dir() {
            return Ok(candidate);
        }
        let known = self.list();
        Err(Error::Environment {
            what: format!("there is no Omarchy theme called {name}"),
            suggestion: if known.is_empty() {
                format!(
                    "no themes were found in {} or {}; {}",
                    self.user.display(),
                    self.packaged.display(),
                    if self.prefixed {
                        "under --root the Omarchy tree is read at <prefix>/usr/share/omarchy, so put or link one there"
                    } else {
                        "set OMARCHY_PATH if the tree is elsewhere"
                    }
                )
            } else {
                format!("the themes on this system are: {}", known.join(", "))
            },
        })
    }

    /// Every theme name, from both locations, without duplicates.
    pub fn list(&self) -> Vec<String> {
        let mut names: Vec<String> = [&self.user, &self.packaged]
            .into_iter()
            .filter_map(|dir| fs::read_dir(dir).ok())
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// The palette of one theme.
    pub fn palette(&self, name: &str) -> Result<Palette> {
        read_palette(&self.dir(name)?.join(COLORS))
    }

    /// The image to start a logo from.
    pub fn unlock_image(&self, name: &str) -> Result<PathBuf> {
        let path = self.dir(name)?.join(UNLOCK);
        let metadata = fs::symlink_metadata(&path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                Error::Environment {
                    what: format!("the Omarchy theme {name} has no {UNLOCK}"),
                    suggestion:
                        "pick a theme that has one, or scaffold without --from-omarchy-theme and add your own logo.png"
                            .to_string(),
                }
            } else {
                Error::read(path.clone(), source)
            }
        })?;
        if metadata.file_type().is_symlink() {
            return Err(Error::Environment {
                what: format!("{} is a symlink", path.display()),
                suggestion: "copy the image into place yourself; omaboot does not follow symlinks into a theme"
                    .to_string(),
            });
        }
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(Error::Environment {
                what: format!("{} is not a usable image", path.display()),
                suggestion: "pick another theme, or add your own logo.png afterwards".to_string(),
            });
        }
        Ok(path)
    }
}

/// Read `background`, `foreground`, `accent` and `red` out of an Omarchy
/// `colors.toml`.
///
/// The file is Omarchy's, not omaboot's, so unknown keys are ignored rather
/// than refused: it carries two dozen keys today and may carry more tomorrow,
/// and refusing one of them would make omaboot break on an Omarchy update.
/// The keys it does read are validated, because a colour that is not `#rrggbb`
/// cannot be written into a Plymouth script.
pub fn read_palette(path: &Path) -> Result<Palette> {
    let text = fs::read_to_string(path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            Error::Environment {
                what: format!("{} does not exist", path.display()),
                suggestion: "pick a theme that has a colors.toml".to_string(),
            }
        } else {
            Error::read(path.to_path_buf(), source)
        }
    })?;
    let table: toml::Table = toml::from_str(&text).map_err(|source| Error::ParseToml {
        path: path.to_path_buf(),
        source,
    })?;

    let colour = |key: &str, fallback: Option<&str>| -> Result<String> {
        let value = match table.get(key).and_then(|value| value.as_str()) {
            Some(value) => value.to_string(),
            None => match fallback {
                Some(fallback) => return Ok(fallback.to_string()),
                None => {
                    return Err(Error::Environment {
                        what: format!("{} has no {key}", path.display()),
                        suggestion:
                            "pick another theme, or scaffold without --from-omarchy-theme and set the colours yourself"
                                .to_string(),
                    });
                }
            },
        };
        Rgb::parse(&value).map_err(|what| Error::Environment {
            what: format!("{} has {key} = {value:?}: {what}", path.display()),
            suggestion: "pick another theme, or set the colours yourself".to_string(),
        })?;
        Ok(value)
    };

    Ok(Palette {
        background: colour("background", None)?,
        foreground: colour("foreground", None)?,
        accent: colour("accent", Some(&Palette::fallback().accent))?,
        // Omarchy themes have no "error" colour. `red` is what every one of
        // them uses for exactly this, and it is the closest thing to the
        // hard coded #f7768e that upstream paints failed-login assets with.
        error: colour("red", Some(&Palette::fallback().error))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> (tempfile::TempDir, Layout) {
        let tmp = tempfile::tempdir().unwrap();
        let layout = Layout::with_dirs(
            Some(tmp.path().join("root")),
            tmp.path().join("config/omaboot"),
            tmp.path().join("state"),
        );
        (tmp, layout)
    }

    /// A packaged Omarchy theme, as the installed tree has them.
    fn packaged_theme(root: &Path, name: &str, colors: &str) -> PathBuf {
        let dir = root.join("usr/share/omarchy/themes").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(COLORS), colors).unwrap();
        fs::write(dir.join(UNLOCK), format!("unlock image of {name}")).unwrap();
        dir
    }

    const CATPPUCCIN: &str = r##"mode = "dark"

accent = "#89b4fa"
background = "#1e1e2e"
foreground = "#cdd6f4"
red = "#f38ba8"
bright_magenta = "#f5c2e7"
"##;

    #[test]
    fn a_prefix_wins_over_omarchy_path_in_the_environment() {
        let (tmp, layout) = world();
        let elsewhere = std::ffi::OsString::from("/somewhere/else/omarchy");
        assert_eq!(
            omarchy_path_from(&layout, Some(elsewhere)),
            tmp.path().join("root/usr/share/omarchy"),
            "a sandboxed run reads nothing outside its prefix"
        );
    }

    #[test]
    fn without_a_prefix_the_environment_wins_over_the_packaged_tree() {
        let layout = Layout::with_dirs(None, PathBuf::from("/c"), PathBuf::from("/s"));
        let elsewhere = std::ffi::OsString::from("/somewhere/else/omarchy");
        assert_eq!(
            omarchy_path_from(&layout, Some(elsewhere)),
            PathBuf::from("/somewhere/else/omarchy")
        );
        assert_eq!(
            omarchy_path_from(&layout, Some(std::ffi::OsString::new())),
            PathBuf::from(DEFAULT_OMARCHY_PATH),
            "an empty variable means unset"
        );
        assert_eq!(
            omarchy_path_from(&layout, None),
            PathBuf::from(DEFAULT_OMARCHY_PATH)
        );
    }

    #[test]
    fn the_palette_comes_from_the_named_keys() {
        let (tmp, layout) = world();
        packaged_theme(&tmp.path().join("root"), "catppuccin", CATPPUCCIN);
        let palette = OmarchyThemes::discover(&layout)
            .palette("catppuccin")
            .unwrap();
        assert_eq!(
            palette,
            Palette {
                background: "#1e1e2e".to_string(),
                foreground: "#cdd6f4".to_string(),
                accent: "#89b4fa".to_string(),
                error: "#f38ba8".to_string(),
            }
        );
    }

    #[test]
    fn keys_omaboot_does_not_use_are_ignored_not_refused() {
        let (tmp, layout) = world();
        packaged_theme(
            &tmp.path().join("root"),
            "future",
            "background = \"#000000\"\nforeground = \"#ffffff\"\nsomething_new_in_omarchy = \"#123456\"\n",
        );
        OmarchyThemes::discover(&layout)
            .palette("future")
            .expect("an unknown key in Omarchy's own file must not break omaboot");
    }

    #[test]
    fn a_missing_accent_or_red_falls_back() {
        let (tmp, layout) = world();
        packaged_theme(
            &tmp.path().join("root"),
            "spare",
            "background = \"#000000\"\nforeground = \"#ffffff\"\n",
        );
        let palette = OmarchyThemes::discover(&layout).palette("spare").unwrap();
        assert_eq!(palette.accent, "#7aa2f7");
        assert_eq!(palette.error, "#f7768e");
    }

    #[test]
    fn a_missing_background_is_an_error_that_names_the_file() {
        let (tmp, layout) = world();
        packaged_theme(
            &tmp.path().join("root"),
            "thin",
            "foreground = \"#ffffff\"\n",
        );
        let error = OmarchyThemes::discover(&layout)
            .palette("thin")
            .unwrap_err()
            .to_string();
        assert!(error.contains("background"), "{error}");
        assert!(error.contains("colors.toml"), "{error}");
    }

    #[test]
    fn a_colour_that_is_not_rrggbb_is_refused() {
        let (tmp, layout) = world();
        packaged_theme(
            &tmp.path().join("root"),
            "odd",
            "background = \"rebeccapurple\"\nforeground = \"#ffffff\"\n",
        );
        let error = OmarchyThemes::discover(&layout)
            .palette("odd")
            .unwrap_err()
            .to_string();
        assert!(error.contains("rebeccapurple"), "{error}");
    }

    #[test]
    fn a_user_theme_wins_over_the_packaged_one() {
        let (tmp, layout) = world();
        packaged_theme(&tmp.path().join("root"), "nord", CATPPUCCIN);
        let user = tmp.path().join("config/omarchy/themes/nord");
        fs::create_dir_all(&user).unwrap();
        fs::write(
            user.join(COLORS),
            "background = \"#111111\"\nforeground = \"#eeeeee\"\n",
        )
        .unwrap();

        let palette = OmarchyThemes::discover(&layout).palette("nord").unwrap();
        assert_eq!(palette.background, "#111111");
    }

    #[test]
    fn an_unknown_theme_lists_the_ones_there_are() {
        let (tmp, layout) = world();
        packaged_theme(&tmp.path().join("root"), "gruvbox", CATPPUCCIN);
        packaged_theme(&tmp.path().join("root"), "nord", CATPPUCCIN);
        let error = OmarchyThemes::discover(&layout)
            .palette("tokyo-night")
            .unwrap_err()
            .to_string();
        assert!(error.contains("gruvbox, nord"), "{error}");
    }

    #[test]
    fn a_theme_without_an_unlock_image_says_so() {
        let (tmp, layout) = world();
        let dir = packaged_theme(&tmp.path().join("root"), "bare", CATPPUCCIN);
        fs::remove_file(dir.join(UNLOCK)).unwrap();
        let error = OmarchyThemes::discover(&layout)
            .unlock_image("bare")
            .unwrap_err()
            .to_string();
        assert!(error.contains("unlock.png"), "{error}");
    }

    #[test]
    fn a_symlinked_unlock_image_is_refused() {
        let (tmp, layout) = world();
        let dir = packaged_theme(&tmp.path().join("root"), "linked", CATPPUCCIN);
        fs::remove_file(dir.join(UNLOCK)).unwrap();
        fs::write(tmp.path().join("elsewhere.png"), b"bytes").unwrap();
        std::os::unix::fs::symlink(tmp.path().join("elsewhere.png"), dir.join(UNLOCK)).unwrap();
        let error = OmarchyThemes::discover(&layout)
            .unlock_image("linked")
            .unwrap_err()
            .to_string();
        assert!(error.contains("symlink"), "{error}");
    }
}
