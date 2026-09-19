//! What boots now.
//!
//! Everything the terminal interface and `omaboot status` say about the
//! current screens comes from here, and all of it is read from the system
//! rather than from anything omaboot cached: `plymouthd.conf` names the
//! Plymouth theme, the SDDM configuration files decide the login theme, and
//! the installed theme directories supply the colours and the logo. The
//! applied-state record is shown next to that, never instead of it, so a
//! record that no longer matches the system is visible as exactly that.
//!
//! Nothing here writes.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::generate::{AssetSource, Content, GeneratedFile, GeneratedTheme, generate};
use crate::omarchy::{self, OmarchyThemes, Palette};
use crate::paths::Layout;
use crate::render::{self, Screen};
use crate::state::{self, AppliedState, SddmTheme};
use crate::theme::{
    Login, LoginBackground, LoginLayout, Manifest, Meta, Progress, Prompt, Rgb, Shutdown,
    ShutdownLogo, Theme, Unlock,
};

/// The theme directory name omaboot installs under, for both Plymouth and SDDM.
pub const OMABOOT: &str = "omaboot";
/// The stock theme directory name Omarchy installs under, for both.
pub const OMARCHY: &str = "omarchy";

/// Who put the installed theme there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// Omarchy's own theme, kept up to date by `omarchy-plymouth-set`.
    Omarchy,
    /// omaboot's theme, from `omaboot apply`.
    Omaboot,
    /// Anything else: a package, a hand-installed theme, a third-party one.
    Other,
}

impl Owner {
    pub fn of(theme_name: &str) -> Self {
        match theme_name {
            OMABOOT => Self::Omaboot,
            OMARCHY => Self::Omarchy,
            _ => Self::Other,
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Self::Omarchy => "Omarchy's own theme",
            Self::Omaboot => "installed by omaboot",
            Self::Other => "not from Omarchy and not from omaboot",
        }
    }
}

/// The boot and shutdown screens: one Plymouth theme serves both.
#[derive(Debug, Clone, PartialEq)]
pub struct PlymouthNow {
    /// `Theme=` from plymouthd.conf, or nothing when the file has none.
    pub theme: Option<String>,
    pub conf: PathBuf,
    /// The theme directory, when it exists.
    pub dir: Option<PathBuf>,
    pub owner: Owner,
    /// For Omarchy's theme: which Omarchy theme styled it, found the way
    /// `omarchy-plymouth-current` finds it, by comparing the installed logo.
    pub styled_by: Option<String>,
    pub background: Option<Rgb>,
    pub foreground: Option<Rgb>,
    pub logo: Option<PathBuf>,
}

/// The login screen.
#[derive(Debug, Clone, PartialEq)]
pub struct LoginNow {
    /// The theme SDDM will use and the file that decided it.
    pub theme: Option<SddmTheme>,
    pub dir: Option<PathBuf>,
    pub owner: Owner,
    pub background: Option<Rgb>,
    pub foreground: Option<Rgb>,
    pub accent: Option<Rgb>,
    pub error: Option<Rgb>,
}

/// One line of the read-only inspector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    pub label: String,
    pub value: String,
}

impl Fact {
    fn new(label: &str, value: impl Into<String>) -> Self {
        Self {
            label: label.to_string(),
            value: value.into(),
        }
    }
}

/// Everything read about the current screens, in one go.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub plymouth: PlymouthNow,
    pub login: LoginNow,
    pub applied: Option<AppliedState>,
    /// The saved theme the record names, when it is still on disk.
    pub applied_theme: Option<Theme>,
    /// The `--root` prefix, stripped from paths when they are shown: the
    /// header already says the run is sandboxed, and the facts read better
    /// as the system paths they stand for.
    root: Option<PathBuf>,
}

impl Snapshot {
    /// Read the system. This never fails: anything that cannot be read is
    /// simply absent, and the facts say so.
    pub fn read(layout: &Layout) -> Self {
        let applied = state::read_applied(layout).ok().flatten();
        let applied_theme = applied
            .as_ref()
            .and_then(|applied| Theme::read(layout.theme_dir(&applied.theme)).ok());

        let plymouth = read_plymouth(layout);
        let login = read_login(layout);

        Self {
            plymouth,
            login,
            applied,
            applied_theme,
            root: layout.root().map(Path::to_path_buf),
        }
    }

    /// A path as it is shown: without the sandbox prefix, if there is one.
    pub fn show(&self, path: &Path) -> String {
        match &self.root {
            Some(root) => match path.strip_prefix(root) {
                Ok(rest) => format!("/{}", rest.display()),
                Err(_) => path.display().to_string(),
            },
            None => path.display().to_string(),
        }
    }

    /// True when the record says a saved theme is applied and the system
    /// agrees, so that theme is what boots now.
    pub fn applied_is_current(&self) -> bool {
        self.applied.is_some() && self.plymouth.owner == Owner::Omaboot
    }

    /// The saved theme that is what boots now, if any.
    pub fn applied_theme_id(&self) -> Option<&str> {
        if self.applied_is_current() {
            self.applied.as_ref().map(|applied| applied.theme.as_str())
        } else {
            None
        }
    }

    /// One line for the header and the status bar.
    pub fn headline(&self) -> String {
        let boot = match (&self.plymouth.theme, &self.plymouth.styled_by) {
            (Some(theme), Some(styled)) if self.plymouth.owner == Owner::Omarchy => {
                format!("{theme} ({styled})")
            }
            (Some(theme), _) => theme.clone(),
            (None, _) => "no Plymouth theme set".to_string(),
        };
        let login = self
            .login
            .theme
            .as_ref()
            .map(|theme| theme.name.clone())
            .unwrap_or_else(|| "no SDDM theme set".to_string());
        format!("boot and shutdown: {boot} · login: {login}")
    }

    /// What is wrong, if anything, said plainly.
    pub fn warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(applied) = &self.applied {
            if self.plymouth.owner != Owner::Omaboot {
                out.push(format!(
                    "the record says {} is applied, but {} names {}: something else changed the boot screen since; omaboot reset clears the record",
                    applied.theme,
                    self.show(&self.plymouth.conf),
                    self.plymouth.theme.as_deref().unwrap_or("no theme")
                ));
            }
            match &self.login.theme {
                Some(theme) if theme.name != OMABOOT => out.push(format!(
                    "the record says {} is applied, but the login theme is {}, decided by {}",
                    applied.theme,
                    theme.name,
                    self.show(&theme.decided_by)
                )),
                Some(_) => {}
                None => out.push(format!(
                    "the record says {} is applied, but no SDDM configuration names a theme",
                    applied.theme
                )),
            }
            if self.applied_theme.is_none() {
                out.push(format!(
                    "the applied theme {} is no longer in your themes; the installed copy keeps working, omaboot reset removes it",
                    applied.theme
                ));
            }
        }
        if self.plymouth.theme.is_some() && self.plymouth.dir.is_none() {
            out.push(format!(
                "{} names a Plymouth theme that is not installed",
                self.show(&self.plymouth.conf)
            ));
        }
        if let Some(theme) = &self.login.theme
            && self.login.dir.is_none()
        {
            out.push(format!(
                "{} names an SDDM theme that is not installed",
                self.show(&theme.decided_by)
            ));
        }
        out
    }

    /// The inspector for one screen: where the facts come from, and the
    /// facts themselves.
    pub fn facts(&self, screen: Screen) -> Vec<Fact> {
        let mut facts = Vec::new();
        let hex = |colour: &Option<Rgb>| {
            colour
                .map(|c| c.hex())
                .unwrap_or_else(|| "unknown".to_string())
        };
        match screen {
            Screen::Unlock | Screen::Shutdown => {
                let now = &self.plymouth;
                facts.push(Fact::new(
                    "theme",
                    now.theme.clone().unwrap_or_else(|| "none set".to_string()),
                ));
                facts.push(Fact::new("set in", self.show(&now.conf)));
                facts.push(Fact::new("owner", now.owner.describe()));
                if let Some(styled) = &now.styled_by {
                    facts.push(Fact::new("styled by", format!("Omarchy theme {styled}")));
                }
                if let Some(id) = self.applied_theme_id() {
                    facts.push(Fact::new("your theme", id.to_string()));
                }
                facts.push(Fact::new(
                    "files",
                    now.dir
                        .as_ref()
                        .map(|dir| self.show(dir))
                        .unwrap_or_else(|| "not installed".to_string()),
                ));
                facts.push(Fact::new("background", hex(&now.background)));
                facts.push(Fact::new("text", hex(&now.foreground)));
                facts.push(Fact::new(
                    "logo",
                    now.logo
                        .as_ref()
                        .map(|logo| self.show(logo))
                        .unwrap_or_else(|| "none".to_string()),
                ));
            }
            Screen::Login => {
                let now = &self.login;
                match &now.theme {
                    Some(theme) => {
                        facts.push(Fact::new("theme", theme.name.clone()));
                        facts.push(Fact::new("decided by", self.show(&theme.decided_by)));
                    }
                    None => facts.push(Fact::new("theme", "none set")),
                }
                facts.push(Fact::new("owner", now.owner.describe()));
                facts.push(Fact::new(
                    "files",
                    now.dir
                        .as_ref()
                        .map(|dir| self.show(dir))
                        .unwrap_or_else(|| "not installed".to_string()),
                ));
                facts.push(Fact::new("background", hex(&now.background)));
                facts.push(Fact::new("text", hex(&now.foreground)));
                facts.push(Fact::new("accent", hex(&now.accent)));
                facts.push(Fact::new("error", hex(&now.error)));
            }
        }
        if let Some(applied) = &self.applied {
            facts.push(Fact::new(
                "record",
                format!(
                    "{} applied {}",
                    applied.theme,
                    state::describe_age(applied.applied_at_unix, state::now_unix())
                ),
            ));
        }
        facts
    }

    /// The files and values a composite of this screen is drawn from, when
    /// there is enough to draw one honestly. The login screen of a theme
    /// omaboot does not know the layout of gets nothing: its facts are shown
    /// instead of a picture that would be a guess.
    pub fn picture(
        &self,
        screen: Screen,
        assets: &AssetSource,
    ) -> Option<(GeneratedTheme, Manifest)> {
        // What omaboot installed is best drawn from the theme it came from.
        if self.plymouth.owner == Owner::Omaboot
            && (screen.is_plymouth() || self.login.owner == Owner::Omaboot)
            && let Some(theme) = &self.applied_theme
            && let Ok(valid) = theme.clone().validate()
            && let Ok(generated) = generate(&valid, assets)
        {
            return Some((generated, theme.manifest.clone()));
        }

        let manifest = self.derived_manifest();
        let copies = |dir: &Path, names: &[&str]| -> Option<Vec<GeneratedFile>> {
            names
                .iter()
                .map(|name| {
                    let path = dir.join(name);
                    path.is_file().then(|| GeneratedFile {
                        name: (*name).to_string(),
                        content: Content::CopyOf(path),
                    })
                })
                .collect()
        };
        match screen {
            Screen::Unlock | Screen::Shutdown => {
                let dir = self.plymouth.dir.as_ref()?;
                let plymouth = copies(
                    dir,
                    &[
                        "logo.png",
                        "entry.png",
                        "lock.png",
                        "bullet.png",
                        "progress_box.png",
                        "progress_bar.png",
                    ],
                )?;
                Some((
                    GeneratedTheme {
                        plymouth,
                        sddm: Vec::new(),
                    },
                    manifest,
                ))
            }
            Screen::Login => {
                if self.login.owner == Owner::Other {
                    return None;
                }
                let dir = self.login.dir.as_ref()?;
                let sddm = copies(dir, &["logo.png", "entry.png", "lock.png", "bullet.png"])?;
                Some((
                    GeneratedTheme {
                        plymouth: Vec::new(),
                        sddm,
                    },
                    manifest,
                ))
            }
        }
    }

    /// A manifest describing the installed screens, read from their files.
    /// Omarchy's script shows bullets, a progress bar while booting and a
    /// bare logo while shutting down; its greeter is centred, has no clock
    /// and no session picker. Those are the values here; the colours come
    /// from the files themselves.
    pub fn derived_manifest(&self) -> Manifest {
        let fallback = Palette::fallback();
        let background = self
            .plymouth
            .background
            .or(self.login.background)
            .map(|c| c.hex())
            .unwrap_or(fallback.background);
        let foreground = self
            .plymouth
            .foreground
            .or(self.login.foreground)
            .map(|c| c.hex())
            .unwrap_or(fallback.foreground);
        let accent = self
            .login
            .accent
            .map(|c| c.hex())
            .unwrap_or(fallback.accent);
        let error = self.login.error.map(|c| c.hex()).unwrap_or(fallback.error);
        Manifest {
            meta: Meta {
                name: "Current screens".to_string(),
                author: String::new(),
                version: "0.1.0".to_string(),
            },
            colors: crate::theme::Colors {
                background,
                foreground,
                accent,
                error,
            },
            logo: crate::theme::Logo::default(),
            unlock: Unlock {
                prompt: Prompt::Bullets,
                progress: Progress::Bar,
                message: String::new(),
            },
            shutdown: Shutdown {
                logo: ShutdownLogo::Inherit,
                message: String::new(),
                progress: Progress::None,
            },
            login: Login {
                layout: LoginLayout::Centered,
                clock: false,
                show_session_picker: false,
                background: LoginBackground::Color,
            },
        }
    }

    /// What a saved theme made from the current screens starts with.
    pub fn derive(&self, name: &str) -> Result<Derived> {
        // The theme omaboot installed is the exact description of what boots,
        // so a copy of it beats a reconstruction from the installed files.
        if self.plymouth.owner == Owner::Omaboot
            && let Some(theme) = &self.applied_theme
        {
            let mut manifest = theme.manifest.clone();
            manifest.meta.name = name.to_string();
            let mut files = Vec::new();
            for reference in [
                Some(manifest.logo.source.as_str()),
                match &manifest.shutdown.logo {
                    ShutdownLogo::Inherit => None,
                    ShutdownLogo::Source(source) => Some(source.as_str()),
                },
                manifest
                    .login
                    .background
                    .needs_image()
                    .then_some(crate::theme::LOGIN_BACKGROUND),
            ]
            .into_iter()
            .flatten()
            {
                let path = theme.asset_path(reference)?;
                if path.is_file() {
                    files.push((path, reference.to_string()));
                }
            }
            return Ok(Derived {
                manifest,
                files,
                from: format!("your theme {}, which is what boots now", theme.id),
            });
        }

        let dir = self
            .plymouth
            .dir
            .as_ref()
            .ok_or_else(|| Error::Environment {
                what: match &self.plymouth.theme {
                    Some(theme) => format!("the Plymouth theme {theme} is not installed"),
                    None => format!("{} names no Plymouth theme", self.plymouth.conf.display()),
                },
                suggestion: "start from an Omarchy theme instead".to_string(),
            })?;
        let logo = dir.join("logo.png");
        if !logo.is_file() {
            return Err(Error::Environment {
                what: format!("{} has no logo.png", dir.display()),
                suggestion:
                    "start from an Omarchy theme instead, or make a blank theme and add a logo"
                        .to_string(),
            });
        }
        let mut manifest = self.derived_manifest();
        manifest.meta.name = name.to_string();
        let from = match (&self.plymouth.theme, &self.plymouth.styled_by) {
            (Some(theme), Some(styled)) => {
                format!("the installed {theme} theme, styled by {styled}")
            }
            (Some(theme), None) => format!("the installed {theme} theme"),
            (None, _) => "the installed theme".to_string(),
        };
        Ok(Derived {
            manifest,
            files: vec![(logo, "logo.png".to_string())],
            from,
        })
    }
}

/// A theme derived from the current screens: what to write, and where it
/// came from, so the app can say so.
#[derive(Debug, Clone, PartialEq)]
pub struct Derived {
    pub manifest: Manifest,
    /// Files to copy into the new theme, as (source, name in the theme).
    pub files: Vec<(PathBuf, String)>,
    pub from: String,
}

fn read_plymouth(layout: &Layout) -> PlymouthNow {
    let conf = layout.plymouthd_conf();
    let theme = state::current_plymouth_theme(layout);
    let dir = theme
        .as_ref()
        .map(|theme| layout.plymouth_theme_root().join(theme))
        .filter(|dir| dir.is_dir());
    let owner = theme.as_deref().map(Owner::of).unwrap_or(Owner::Other);

    let script = dir
        .as_ref()
        .zip(theme.as_ref())
        .and_then(|(dir, theme)| fs::read_to_string(dir.join(format!("{theme}.script"))).ok());
    let background = script.as_deref().and_then(parse_background);
    let foreground = dir
        .as_ref()
        .and_then(|dir| fs::read(dir.join("bullet.png")).ok())
        .and_then(|bytes| render::decode(&bytes, "bullet.png").ok())
        .and_then(|image| glyph_colour(&image));
    let logo = dir
        .as_ref()
        .map(|dir| dir.join("logo.png"))
        .filter(|logo| logo.is_file());
    let styled_by = match (owner, &logo) {
        (Owner::Omarchy, Some(logo)) => Some(identify_omarchy_styling(layout, logo)),
        _ => None,
    };

    PlymouthNow {
        theme,
        conf,
        dir,
        owner,
        styled_by,
        background,
        foreground,
        logo,
    }
}

fn read_login(layout: &Layout) -> LoginNow {
    let theme = state::effective_sddm_theme(layout);
    let dir = theme
        .as_ref()
        .map(|theme| layout.sddm_theme_root().join(&theme.name))
        .filter(|dir| dir.is_dir());
    let owner = theme
        .as_ref()
        .map(|theme| Owner::of(&theme.name))
        .unwrap_or(Owner::Other);

    let conf = dir
        .as_ref()
        .and_then(|dir| fs::read_to_string(dir.join("theme.conf")).ok())
        .map(|text| parse_theme_conf(&text))
        .unwrap_or_default();
    let qml_background = dir
        .as_ref()
        .and_then(|dir| fs::read_to_string(dir.join("Main.qml")).ok())
        .and_then(|text| parse_qml_colour(&text));
    let foreground = conf.foreground.or_else(|| {
        dir.as_ref()
            .and_then(|dir| fs::read(dir.join("bullet.png")).ok())
            .and_then(|bytes| render::decode(&bytes, "bullet.png").ok())
            .and_then(|image| glyph_colour(&image))
    });

    LoginNow {
        theme,
        dir,
        owner,
        background: conf.background.or(qml_background),
        foreground,
        accent: conf.accent,
        error: conf.error,
    }
}

/// Which Omarchy theme styled the installed Omarchy Plymouth theme, found the
/// way `omarchy-plymouth-current` finds it: the installed logo is a
/// byte-for-byte copy of either the packaged default or a theme's unlock.png.
fn identify_omarchy_styling(layout: &Layout, installed_logo: &Path) -> String {
    let Ok(installed) = fs::read(installed_logo) else {
        return "unknown".to_string();
    };
    let packaged = omarchy::omarchy_path(layout).join("default/plymouth/logo.png");
    if fs::read(&packaged).is_ok_and(|bytes| bytes == installed) {
        return "default".to_string();
    }
    let themes = OmarchyThemes::discover(layout);
    for name in themes.list() {
        if let Ok(unlock) = themes.unlock_image(&name)
            && fs::read(&unlock).is_ok_and(|bytes| bytes == installed)
        {
            return name;
        }
    }
    "unknown".to_string()
}

/// The background colour from a Plymouth script: the three floats of
/// `Window.SetBackgroundTopColor(r, g, b)`.
pub fn parse_background(script: &str) -> Option<Rgb> {
    let start = script.find("Window.SetBackgroundTopColor(")?;
    let rest = &script[start + "Window.SetBackgroundTopColor(".len()..];
    let end = rest.find(')')?;
    let parts: Vec<f64> = rest[..end]
        .split(',')
        .map(|part| part.trim().parse::<f64>())
        .collect::<std::result::Result<_, _>>()
        .ok()?;
    if parts.len() != 3 || parts.iter().any(|v| !(0.0..=1.0).contains(v)) {
        return None;
    }
    let byte = |v: f64| (v * 255.0).round() as u8;
    Some(Rgb {
        r: byte(parts[0]),
        g: byte(parts[1]),
        b: byte(parts[2]),
    })
}

/// The colours of an SDDM theme.conf `[General]` section, by the names
/// Omarchy's themes use.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ThemeConf {
    pub background: Option<Rgb>,
    pub foreground: Option<Rgb>,
    pub accent: Option<Rgb>,
    pub error: Option<Rgb>,
}

pub fn parse_theme_conf(text: &str) -> ThemeConf {
    let mut out = ThemeConf::default();
    let mut in_general = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            in_general = line.eq_ignore_ascii_case("[general]");
            continue;
        }
        if !in_general {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let colour = Rgb::parse(value.trim()).ok();
        match key.trim().to_ascii_lowercase().as_str() {
            "background" => out.background = colour,
            "foreground" => out.foreground = colour,
            "accent" => out.accent = colour,
            "error" => out.error = colour,
            _ => {}
        }
    }
    out
}

/// The first `color: "#rrggbb"` in a QML file, which in Omarchy's Main.qml is
/// the root rectangle: the background.
pub fn parse_qml_colour(text: &str) -> Option<Rgb> {
    text.lines().find_map(|line| {
        let line = line.trim();
        let value = line.strip_prefix("color:")?.trim().trim_matches('"');
        Rgb::parse(value).ok()
    })
}

/// The colour a recoloured glyph was painted in: its most opaque pixel.
pub fn glyph_colour(image: &image::RgbaImage) -> Option<Rgb> {
    image
        .pixels()
        .filter(|pixel| pixel.0[3] > 0)
        .max_by_key(|pixel| pixel.0[3])
        .map(|pixel| Rgb {
            r: pixel.0[0],
            g: pixel.0[1],
            b: pixel.0[2],
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::fixture;

    fn world() -> (tempfile::TempDir, Layout) {
        let tmp = tempfile::tempdir().unwrap();
        let layout = Layout::with_dirs(
            Some(tmp.path().join("root")),
            tmp.path().join("config/omaboot"),
            tmp.path().join("state"),
        );
        (tmp, layout)
    }

    /// A stock Omarchy installation: plymouthd.conf names omarchy, the theme
    /// directory holds the packaged files, and SDDM has the drop-ins the
    /// installer writes.
    fn stock(layout: &Layout, omarchy: &Path) {
        // The packaged logo, which the installed theme is a copy of.
        for dir in ["default/plymouth", "default/sddm/omarchy"] {
            fs::write(
                omarchy.join(dir).join("logo.png"),
                fixture::png(300, 100, [255, 255, 255, 255]),
            )
            .unwrap();
        }
        fs::create_dir_all(layout.plymouthd_conf().parent().unwrap()).unwrap();
        fs::write(layout.plymouthd_conf(), "[Daemon]\nTheme=omarchy\n").unwrap();

        let plymouth = layout.plymouth_theme_root().join("omarchy");
        fs::create_dir_all(&plymouth).unwrap();
        for name in [
            "logo.png",
            "entry.png",
            "lock.png",
            "bullet.png",
            "progress_box.png",
            "progress_bar.png",
        ] {
            fs::copy(
                omarchy.join("default/plymouth").join(name),
                plymouth.join(name),
            )
            .unwrap();
        }
        fs::write(
            plymouth.join("bullet.png"),
            fixture::png(8, 8, [0xc0, 0xca, 0xf5, 255]),
        )
        .unwrap();
        fs::write(
            plymouth.join("omarchy.script"),
            "// stock\nWindow.SetBackgroundTopColor(0.101, 0.105, 0.149);\nWindow.SetBackgroundBottomColor(0.101, 0.105, 0.149);\n",
        )
        .unwrap();

        let sddm = layout.sddm_theme_root().join("omarchy");
        fs::create_dir_all(&sddm).unwrap();
        for name in ["logo.png", "entry.png", "lock.png", "bullet.png"] {
            fs::copy(
                omarchy.join("default/sddm/omarchy").join(name),
                sddm.join(name),
            )
            .unwrap();
        }
        fs::write(sddm.join("theme.conf"), "[General]\n").unwrap();
        fs::write(
            sddm.join("Main.qml"),
            "Rectangle {\n  id: root\n  color: \"#1a1b26\"\n}\n",
        )
        .unwrap();

        let conf = layout.sddm_conf_dir();
        fs::create_dir_all(&conf).unwrap();
        fs::write(conf.join("10-theme.conf"), "[Theme]\nCurrent=omarchy\n").unwrap();
        fs::write(
            conf.join("99-omarchy-login.conf"),
            "[Theme]\nCurrent=omarchy\n",
        )
        .unwrap();
    }

    #[test]
    fn the_background_is_read_from_the_script() {
        let script = "x\nWindow.SetBackgroundTopColor(0.101, 0.105, 0.149);\n";
        assert_eq!(parse_background(script).unwrap().hex(), "#1a1b26");
        assert_eq!(parse_background("nothing here"), None);
        assert_eq!(
            parse_background("Window.SetBackgroundTopColor(2, 0, 0)"),
            None
        );
        assert_eq!(
            parse_background("Window.SetBackgroundTopColor(a, b, c)"),
            None
        );
    }

    #[test]
    fn theme_conf_colours_come_from_the_general_section() {
        let conf = parse_theme_conf(
            "[Other]\nbackground=#000000\n[General]\nbackground=#1a1b26\nforeground=#a9b1d6\naccent=#7aa2f7\nerror=#f7768e\nfont=x\n",
        );
        assert_eq!(conf.background.unwrap().hex(), "#1a1b26");
        assert_eq!(conf.foreground.unwrap().hex(), "#a9b1d6");
        assert_eq!(conf.accent.unwrap().hex(), "#7aa2f7");
        assert_eq!(conf.error.unwrap().hex(), "#f7768e");
        assert_eq!(parse_theme_conf("[General]\n"), ThemeConf::default());
    }

    #[test]
    fn the_qml_background_is_the_first_colour() {
        let qml = "Rectangle {\n  id: root\n  color: \"#2e3440\"\n  Text { color: \"#ffffff\" }\n}";
        assert_eq!(parse_qml_colour(qml).unwrap().hex(), "#2e3440");
        assert_eq!(parse_qml_colour("Item {}"), None);
    }

    #[test]
    fn a_glyph_reports_the_colour_it_was_painted() {
        let image =
            render::decode(&fixture::png_frame(8, 8, [0x88, 0xc0, 0xd0, 255]), "x").unwrap();
        assert_eq!(glyph_colour(&image).unwrap().hex(), "#88c0d0");
        assert_eq!(glyph_colour(&image::RgbaImage::new(4, 4)), None);
    }

    #[test]
    fn a_stock_system_is_described_from_its_files() {
        let (tmp, layout) = world();
        let omarchy = fixture::omarchy_tree(&tmp.path().join("root/usr/share/omarchy"));
        stock(&layout, &omarchy);

        let snapshot = Snapshot::read(&layout);
        assert_eq!(snapshot.plymouth.theme.as_deref(), Some("omarchy"));
        assert_eq!(snapshot.plymouth.owner, Owner::Omarchy);
        assert_eq!(snapshot.plymouth.styled_by.as_deref(), Some("default"));
        assert_eq!(snapshot.plymouth.background.unwrap().hex(), "#1a1b26");
        assert_eq!(snapshot.plymouth.foreground.unwrap().hex(), "#c0caf5");
        assert_eq!(snapshot.login.theme.as_ref().unwrap().name, "omarchy");
        assert!(
            snapshot
                .login
                .theme
                .as_ref()
                .unwrap()
                .decided_by
                .ends_with("99-omarchy-login.conf")
        );
        assert_eq!(snapshot.login.background.unwrap().hex(), "#1a1b26");
        assert!(snapshot.warnings().is_empty(), "{:?}", snapshot.warnings());
        assert!(
            snapshot.headline().contains("omarchy (default)"),
            "{}",
            snapshot.headline()
        );

        let manifest = snapshot.derived_manifest();
        assert_eq!(manifest.colors.background, "#1a1b26");
        assert_eq!(manifest.colors.foreground, "#c0caf5");
        assert_eq!(manifest.shutdown.progress, Progress::None);
        assert!(!manifest.login.clock);
    }

    #[test]
    fn every_screen_of_a_stock_system_can_be_drawn() {
        let (tmp, layout) = world();
        let omarchy = fixture::omarchy_tree(&tmp.path().join("root/usr/share/omarchy"));
        stock(&layout, &omarchy);
        let snapshot = Snapshot::read(&layout);
        let assets = AssetSource::at(&omarchy);
        for screen in [Screen::Unlock, Screen::Login, Screen::Shutdown] {
            let (generated, manifest) = snapshot.picture(screen, &assets).expect("drawable");
            let image = render::composite(
                &generated,
                &manifest,
                screen,
                render::Geometry::new(320, 180),
            )
            .expect("composites");
            assert_eq!(image.dimensions(), (320, 180));
        }
    }

    #[test]
    fn a_third_party_login_theme_gets_facts_and_no_picture() {
        let (tmp, layout) = world();
        let omarchy = fixture::omarchy_tree(&tmp.path().join("root/usr/share/omarchy"));
        stock(&layout, &omarchy);
        let osk = layout.sddm_theme_root().join("omarchy-onscreen-keyboard");
        fs::create_dir_all(&osk).unwrap();
        fs::write(
            osk.join("theme.conf"),
            "[General]\nbackground=#1a1b26\nforeground=#a9b1d6\naccent=#7aa2f7\nerror=#f7768e\n",
        )
        .unwrap();
        fs::write(
            layout
                .sddm_conf_dir()
                .join("99-z-omarchy-onscreen-keyboard.conf"),
            "[Theme]\nCurrent=omarchy-onscreen-keyboard\n",
        )
        .unwrap();

        let snapshot = Snapshot::read(&layout);
        assert_eq!(
            snapshot.login.theme.as_ref().unwrap().name,
            "omarchy-onscreen-keyboard"
        );
        assert_eq!(snapshot.login.owner, Owner::Other);
        assert_eq!(snapshot.login.accent.unwrap().hex(), "#7aa2f7");
        assert!(
            snapshot
                .picture(Screen::Login, &AssetSource::at(&omarchy))
                .is_none()
        );
        let facts = snapshot.facts(Screen::Login);
        assert!(
            facts.iter().any(|f| f.label == "decided by"
                && f.value.ends_with("99-z-omarchy-onscreen-keyboard.conf"))
        );
        // The accent still flows into a derived theme.
        assert_eq!(snapshot.derived_manifest().colors.accent, "#7aa2f7");
    }

    #[test]
    fn a_theme_derived_from_a_stock_system_carries_its_logo_and_colours() {
        let (tmp, layout) = world();
        let omarchy = fixture::omarchy_tree(&tmp.path().join("root/usr/share/omarchy"));
        stock(&layout, &omarchy);
        let derived = Snapshot::read(&layout).derive("mine").unwrap();
        assert_eq!(derived.manifest.meta.name, "mine");
        assert_eq!(derived.files.len(), 1);
        assert_eq!(derived.files[0].1, "logo.png");
        assert!(
            derived.from.contains("styled by default"),
            "{}",
            derived.from
        );
    }

    #[test]
    fn an_applied_theme_that_the_system_agrees_with_is_what_boots() {
        let (tmp, layout) = world();
        let omarchy = fixture::omarchy_tree(&tmp.path().join("root/usr/share/omarchy"));
        stock(&layout, &omarchy);
        fs::write(layout.plymouthd_conf(), "[Daemon]\nTheme=omaboot\n").unwrap();
        fs::write(layout.sddm_dropin(), state::sddm_dropin_contents("omaboot")).unwrap();
        fs::create_dir_all(layout.plymouth_theme_dir()).unwrap();
        fs::create_dir_all(layout.sddm_theme_dir()).unwrap();
        fixture::theme(
            &layout.theme_dir("mine"),
            "[meta]\nname = \"Mine\"\n[colors]\nbackground = \"#010203\"\n",
        );
        let applied = AppliedState {
            version: state::STATE_VERSION,
            theme: "mine".to_string(),
            theme_hash: "x".to_string(),
            applied_at_unix: 0,
            files: Vec::new(),
        };
        state::write_atomic(
            &layout.applied_state_file(),
            state::serialize_applied(&applied).as_bytes(),
        )
        .unwrap();

        let snapshot = Snapshot::read(&layout);
        assert_eq!(snapshot.applied_theme_id(), Some("mine"));
        assert!(snapshot.warnings().is_empty(), "{:?}", snapshot.warnings());
        let (_, manifest) = snapshot
            .picture(Screen::Unlock, &AssetSource::at(&omarchy))
            .unwrap();
        assert_eq!(manifest.colors.background, "#010203");
        let derived = snapshot.derive("copy").unwrap();
        assert_eq!(derived.manifest.colors.background, "#010203");
        assert!(derived.from.contains("your theme mine"), "{}", derived.from);
    }

    #[test]
    fn a_record_the_system_disagrees_with_is_a_warning_not_a_fact() {
        let (tmp, layout) = world();
        let omarchy = fixture::omarchy_tree(&tmp.path().join("root/usr/share/omarchy"));
        stock(&layout, &omarchy);
        let applied = AppliedState {
            version: state::STATE_VERSION,
            theme: "gone".to_string(),
            theme_hash: "x".to_string(),
            applied_at_unix: 0,
            files: Vec::new(),
        };
        state::write_atomic(
            &layout.applied_state_file(),
            state::serialize_applied(&applied).as_bytes(),
        )
        .unwrap();

        let snapshot = Snapshot::read(&layout);
        assert_eq!(snapshot.applied_theme_id(), None);
        let warnings = snapshot.warnings();
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("something else changed the boot screen")),
            "{warnings:?}"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("no longer in your themes")),
            "{warnings:?}"
        );
        // The picture is still what is installed, not the record.
        let (_, manifest) = snapshot
            .picture(Screen::Unlock, &AssetSource::at(&omarchy))
            .unwrap();
        assert_eq!(manifest.colors.background, "#1a1b26");
    }

    #[test]
    fn an_empty_system_has_facts_and_nothing_to_derive() {
        let (_tmp, layout) = world();
        let snapshot = Snapshot::read(&layout);
        assert_eq!(snapshot.plymouth.theme, None);
        assert!(
            snapshot
                .facts(Screen::Unlock)
                .iter()
                .any(|f| f.value == "none set")
        );
        assert!(
            snapshot
                .picture(Screen::Unlock, &AssetSource::at("/nowhere"))
                .is_none()
        );
        let error = snapshot.derive("x").unwrap_err().to_string();
        assert!(error.contains("names no Plymouth theme"), "{error}");
    }
}
