//! Filesystem layout.
//!
//! Every path omaboot reads or writes is derived here, so that a test can run
//! the whole pipeline against a temporary prefix and so that the two
//! directories Omarchy owns can be refused in exactly one place.

use std::env;
use std::path::{Component, Path, PathBuf};

use crate::error::{Error, Result};

/// Directories owned by Omarchy. omaboot never writes inside these, at any
/// depth, for any reason. This is the project's central promise.
pub const OMARCHY_OWNED: [&str; 2] = [
    "/usr/share/plymouth/themes/omarchy",
    "/usr/share/sddm/themes/omarchy",
];

/// Absolute system paths omaboot owns. Kept as string constants so the
/// privileged helper can repeat the same list verbatim.
pub const PLYMOUTH_THEME_DIR: &str = "/usr/share/plymouth/themes/omaboot";
pub const SDDM_THEME_DIR: &str = "/usr/share/sddm/themes/omaboot";
/// Where every Plymouth and SDDM theme lives, read to describe what boots now.
pub const PLYMOUTH_THEMES_ROOT: &str = "/usr/share/plymouth/themes";
pub const SDDM_THEMES_ROOT: &str = "/usr/share/sddm/themes";
/// SDDM reads `sddm.conf.d` in file-name order and the last `Current=` wins.
/// Omarchy itself ships `99-omarchy-login.conf`, and third-party login themes
/// install `99-z-…` drop-ins to beat that, so omaboot's file sorts after every
/// digit-prefixed name there is. A `90-` name, which the first draft of the
/// architecture chose, would have been silently overridden.
pub const SDDM_DROPIN: &str = "/etc/sddm.conf.d/zz-omaboot.conf";
/// The directory those drop-ins live in, read to find the effective theme.
pub const SDDM_CONF_DIR: &str = "/etc/sddm.conf.d";
pub const SDDM_CONF: &str = "/etc/sddm.conf";
pub const PLYMOUTHD_CONF: &str = "/etc/plymouth/plymouthd.conf";
/// Where a live preview theme is put for plymouthd. `/run` is a tmpfs, and
/// plymouthd searches `/run/plymouth/themes` before the installed themes.
pub const PREVIEW_THEME_DIR: &str = "/run/plymouth/themes/omaboot-preview";
/// The name plymouthd is asked for on its fake kernel command line.
pub const PREVIEW_THEME_ID: &str = "omaboot-preview";

/// The Plymouth theme name omaboot installs and activates.
pub const THEME_ID: &str = "omaboot";
/// The theme omaboot hands the system back to on reset.
pub const STOCK_THEME_ID: &str = "omarchy";

/// Resolved locations for one run.
///
/// `root` prefixes system paths only. The user's configuration and state
/// directories follow the XDG variables, so a test points `HOME` and
/// `XDG_*_HOME` at a temporary directory and `--root` at another, and nothing
/// reaches the real system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    root: Option<PathBuf>,
    /// The home everything user-owned hangs off: the `~/.local/bin` links
    /// and, through `data_dir`, the desktop entry and the icons. Resolved
    /// once, here, because code that reads `$HOME` deep in a call stack
    /// reaches the home of whoever is running it — including a test.
    home: PathBuf,
    config_dir: PathBuf,
    state_dir: PathBuf,
    data_dir: PathBuf,
}

impl Layout {
    /// Build a layout from the environment, with an optional system prefix.
    pub fn discover(root: Option<PathBuf>) -> Result<Self> {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| Error::Environment {
                what: "HOME is not set".to_string(),
                suggestion: "run omaboot as a normal login user".to_string(),
            })?;
        let config_base = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        let state_base = env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".local/state"));

        if let Some(root) = &root
            && !root.is_absolute()
        {
            return Err(Error::Environment {
                what: format!("--root {} is not an absolute path", root.display()),
                suggestion: "pass an absolute prefix, for example --root /tmp/omaboot-test"
                    .to_string(),
            });
        }

        let data_dir = env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".local/share"));

        Ok(Self {
            root,
            home,
            config_dir: config_base.join("omaboot"),
            state_dir: state_base.join("omaboot"),
            data_dir,
        })
    }

    /// A layout for tests and for `--root` runs, with all three roots given.
    ///
    /// The home is the configuration directory's parent, so everything
    /// derived from it — `~/.local/bin`, the desktop entry, the icons — lands
    /// in the caller's temporary tree. It must never be the home of whoever
    /// is running the suite: `app::tests` used to call `uninstall` with a
    /// temporary layout while `uninstall` read `$HOME` itself, and so removed
    /// the real launcher entry of the machine it ran on, every time the
    /// suite ran.
    pub fn with_dirs(root: Option<PathBuf>, config_dir: PathBuf, state_dir: PathBuf) -> Self {
        let home = config_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| config_dir.clone());
        let data_dir = home.join(".local/share");
        Self {
            root,
            home,
            config_dir,
            state_dir,
            data_dir,
        }
    }

    /// The same, with the data directory said explicitly: the desktop entry
    /// and the icons go under it.
    pub fn with_data_dir(mut self, data_dir: PathBuf) -> Self {
        self.data_dir = data_dir;
        self
    }

    /// The home the user-owned paths hang off.
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// `~/.local/share`: the desktop entry and the icons.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// True when system paths are redirected under a prefix. A prefixed run
    /// never executes a real system command.
    pub fn is_prefixed(&self) -> bool {
        self.root.is_some()
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// Join an absolute system path onto the prefix, if any.
    pub fn system(&self, absolute: &str) -> PathBuf {
        debug_assert!(absolute.starts_with('/'));
        match &self.root {
            None => PathBuf::from(absolute),
            Some(root) => root.join(absolute.trim_start_matches('/')),
        }
    }

    pub fn plymouth_theme_dir(&self) -> PathBuf {
        self.system(PLYMOUTH_THEME_DIR)
    }

    pub fn sddm_theme_dir(&self) -> PathBuf {
        self.system(SDDM_THEME_DIR)
    }

    pub fn sddm_dropin(&self) -> PathBuf {
        self.system(SDDM_DROPIN)
    }

    /// The directory all Plymouth themes are installed in.
    pub fn plymouth_theme_root(&self) -> PathBuf {
        self.system(PLYMOUTH_THEMES_ROOT)
    }

    /// The directory all SDDM themes are installed in.
    pub fn sddm_theme_root(&self) -> PathBuf {
        self.system(SDDM_THEMES_ROOT)
    }

    pub fn sddm_conf_dir(&self) -> PathBuf {
        self.system(SDDM_CONF_DIR)
    }

    pub fn sddm_conf(&self) -> PathBuf {
        self.system(SDDM_CONF)
    }

    pub fn plymouthd_conf(&self) -> PathBuf {
        self.system(PLYMOUTHD_CONF)
    }

    /// The directory omaboot's own config directory sits in, which is also
    /// where Omarchy keeps a user's themes.
    pub fn config_base(&self) -> PathBuf {
        self.config_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.config_dir.clone())
    }

    /// `~/.config/omaboot`: your themes, and the window's preferences.
    pub fn config_dir(&self) -> PathBuf {
        self.config_dir.clone()
    }

    pub fn themes_dir(&self) -> PathBuf {
        self.config_dir.join("themes")
    }

    pub fn theme_dir(&self, name: &str) -> PathBuf {
        self.themes_dir().join(name)
    }

    /// The directory omaboot's own state directory sits in, which is also
    /// where Omarchy keeps the symlink to the active theme.
    pub fn state_base(&self) -> PathBuf {
        self.state_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.state_dir.clone())
    }

    pub fn state_dir(&self) -> PathBuf {
        self.state_dir.clone()
    }

    pub fn applied_state_file(&self) -> PathBuf {
        self.state_dir.join("applied.toml")
    }

    pub fn rollback_file(&self) -> PathBuf {
        self.state_dir.join("rollback.toml")
    }

    /// Where a staged theme is built before the privileged step sees it.
    pub fn stage_dir(&self) -> PathBuf {
        self.state_dir.join("stage")
    }

    /// The staged Plymouth theme rewritten for a live preview.
    pub fn preview_stage_dir(&self) -> PathBuf {
        self.state_dir.join("stage-preview")
    }
}

/// Refuse any destination inside a directory Omarchy owns.
///
/// The check is textual and runs on the path we are about to write, before the
/// path exists, so it cannot be defeated by a destination that is not yet
/// created. Symlinked destinations are refused separately, at write time.
pub fn guard_destination(path: &Path) -> Result<()> {
    let text = path.to_string_lossy();
    for owned in OMARCHY_OWNED {
        let inside = text == owned
            || text.contains(&format!("{owned}/"))
            || text.ends_with(owned)
            || text.contains(&format!("{owned}\0"));
        if inside {
            return Err(Error::OmarchyOwned {
                path: path.to_path_buf(),
                owned: owned.to_string(),
            });
        }
    }
    Ok(())
}

/// Validate a theme-relative asset reference: relative, no traversal, no root.
pub fn relative_asset(reference: &str) -> Result<PathBuf> {
    if reference.is_empty() {
        return Err(Error::AssetReference {
            reference: reference.to_string(),
            why: "the reference is empty".to_string(),
        });
    }
    let path = Path::new(reference);
    if path.is_absolute() {
        return Err(Error::AssetReference {
            reference: reference.to_string(),
            why: "the reference is absolute".to_string(),
        });
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {
                return Err(Error::AssetReference {
                    reference: reference.to_string(),
                    why: "the reference contains a '.' component".to_string(),
                });
            }
            _ => {
                return Err(Error::AssetReference {
                    reference: reference.to_string(),
                    why: "the reference leaves the theme directory".to_string(),
                });
            }
        }
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> Layout {
        Layout::with_dirs(
            Some(PathBuf::from("/tmp/prefix")),
            PathBuf::from("/tmp/cfg"),
            PathBuf::from("/tmp/state"),
        )
    }

    #[test]
    fn system_paths_follow_the_prefix() {
        let layout = layout();
        assert_eq!(
            layout.plymouth_theme_dir(),
            PathBuf::from("/tmp/prefix/usr/share/plymouth/themes/omaboot")
        );
        assert_eq!(
            layout.sddm_dropin(),
            PathBuf::from("/tmp/prefix/etc/sddm.conf.d/zz-omaboot.conf")
        );
    }

    #[test]
    fn user_directories_ignore_the_prefix() {
        let layout = layout();
        assert_eq!(
            layout.theme_dir("nord"),
            PathBuf::from("/tmp/cfg/themes/nord")
        );
        assert_eq!(
            layout.applied_state_file(),
            PathBuf::from("/tmp/state/applied.toml")
        );
    }

    #[test]
    fn omarchy_directories_are_refused() {
        for path in [
            "/usr/share/plymouth/themes/omarchy",
            "/usr/share/plymouth/themes/omarchy/logo.png",
            "/usr/share/sddm/themes/omarchy/Main.qml",
            "/tmp/prefix/usr/share/sddm/themes/omarchy/Main.qml",
        ] {
            assert!(
                guard_destination(Path::new(path)).is_err(),
                "{path} should be refused"
            );
        }
    }

    #[test]
    fn omaboot_directories_are_allowed() {
        for path in [
            "/usr/share/plymouth/themes/omaboot/omaboot.script",
            "/usr/share/sddm/themes/omaboot/Main.qml",
            "/etc/sddm.conf.d/zz-omaboot.conf",
        ] {
            guard_destination(Path::new(path)).expect("should be allowed");
        }
    }

    #[test]
    fn asset_references_may_not_escape_the_theme() {
        for reference in [
            "../logo.png",
            "/etc/passwd",
            "a/../../b.png",
            "",
            "./logo.png",
        ] {
            assert!(
                relative_asset(reference).is_err(),
                "{reference} should be refused"
            );
        }
        relative_asset("logo.png").expect("plain name is fine");
        relative_asset("assets/logo.png").expect("subdirectory is fine");
    }

    #[test]
    fn relative_prefix_is_refused() {
        let error = Layout::discover(Some(PathBuf::from("relative/prefix"))).unwrap_err();
        assert!(error.to_string().contains("absolute"));
    }
}
