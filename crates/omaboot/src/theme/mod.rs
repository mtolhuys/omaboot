//! The theme model and its strict reader.
//!
//! Parsing and validation are separate on purpose. Parsing rejects anything
//! the format does not define, including unknown keys. Validation then checks
//! the things a type cannot express: colour syntax, numeric ranges, and the
//! assets on disk.

mod validate;

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub use validate::{MAX_ASSET_BYTES, MAX_MESSAGE_CHARS};

/// The file name every theme directory must contain.
pub mod fields;

pub const MANIFEST: &str = "theme.toml";
/// The optional login background, referenced by `login.background`.
pub const LOGIN_BACKGROUND: &str = "background.png";

/// A theme directory that has been read but not yet validated.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// The directory the theme was read from.
    pub dir: PathBuf,
    /// The directory name, which is how the user refers to the theme.
    pub id: String,
    pub manifest: Manifest,
}

impl Theme {
    /// Read and parse `<dir>/theme.toml`. Unknown keys are an error.
    pub fn read(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        let manifest_path = dir.join(MANIFEST);
        let text = fs::read_to_string(&manifest_path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                Error::InvalidTheme {
                    path: dir.clone(),
                    what: format!("there is no {MANIFEST} in this directory"),
                    suggestion: format!(
                        "run `omaboot new <name>` to scaffold a theme, or add {MANIFEST}"
                    ),
                }
            } else {
                Error::read(manifest_path.clone(), source)
            }
        })?;
        let mut manifest: Manifest = toml::from_str(&text).map_err(|source| Error::ParseToml {
            path: manifest_path.clone(),
            source,
        })?;
        manifest.normalise();
        let id = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| manifest.meta.name.clone());
        Ok(Self { dir, id, manifest })
    }

    /// Read, then validate. This is what every command uses.
    pub fn load(dir: impl AsRef<Path>) -> Result<ValidTheme> {
        Self::read(dir)?.validate()
    }

    /// Check everything the type system cannot.
    pub fn validate(self) -> Result<ValidTheme> {
        validate::validate(self)
    }

    pub fn asset_path(&self, reference: &str) -> Result<PathBuf> {
        Ok(self.dir.join(crate::paths::relative_asset(reference)?))
    }
}

/// A theme whose manifest parsed and whose assets are present and usable.
///
/// Only a `ValidTheme` can be generated from, which is how the pipeline
/// guarantees that step 1 ran before step 2.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidTheme {
    inner: Theme,
    logo: PathBuf,
    login_background: Option<PathBuf>,
    shutdown_logo: PathBuf,
}

impl ValidTheme {
    pub(crate) fn new(
        inner: Theme,
        logo: PathBuf,
        login_background: Option<PathBuf>,
        shutdown_logo: PathBuf,
    ) -> Self {
        Self {
            inner,
            logo,
            login_background,
            shutdown_logo,
        }
    }

    pub fn id(&self) -> &str {
        &self.inner.id
    }

    pub fn dir(&self) -> &Path {
        &self.inner.dir
    }

    pub fn manifest(&self) -> &Manifest {
        &self.inner.manifest
    }

    /// The unlock and login logo, as an absolute path on disk.
    pub fn logo(&self) -> &Path {
        &self.logo
    }

    /// The shutdown logo, which is the unlock logo unless the theme overrides it.
    pub fn shutdown_logo(&self) -> &Path {
        &self.shutdown_logo
    }

    pub fn login_background(&self) -> Option<&Path> {
        self.login_background.as_deref()
    }
}

/// `theme.toml`, exactly as `docs/SPEC.md` defines it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub meta: Meta,
    #[serde(default)]
    pub colors: Colors,
    #[serde(default)]
    pub logo: Logo,
    #[serde(default)]
    pub unlock: Unlock,
    #[serde(default)]
    pub shutdown: Shutdown,
    #[serde(default)]
    pub login: Login,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default = "default_version")]
    pub version: String,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Colors {
    #[serde(default = "Colors::default_background")]
    pub background: String,
    #[serde(default = "Colors::default_foreground")]
    pub foreground: String,
    #[serde(default = "Colors::default_accent")]
    pub accent: String,
    #[serde(default = "Colors::default_error")]
    pub error: String,
}

impl Colors {
    fn default_background() -> String {
        "#1a1b26".to_string()
    }
    fn default_foreground() -> String {
        "#c0caf5".to_string()
    }
    fn default_accent() -> String {
        "#7aa2f7".to_string()
    }
    fn default_error() -> String {
        // The colour upstream hard codes for failed-login assets.
        "#f7768e".to_string()
    }
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            background: Self::default_background(),
            foreground: Self::default_foreground(),
            accent: Self::default_accent(),
            error: Self::default_error(),
        }
    }
}

/// `[logo]`: the logo file, and where and how big it is on the unlock
/// screen. `[logo.login]` and `[logo.shutdown]` override the size and place
/// on those screens; anything they leave out follows `[logo]`.
///
/// `width` is the share of the screen's width the logo takes, so a theme
/// looks the same on a 1080p and a 4K display and whatever the size of the
/// file that was dropped in. Omarchy's own logo is 800 pixels on a 1920
/// pixel screen, which is where the default comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Logo {
    #[serde(default = "Logo::default_source")]
    pub source: String,
    #[serde(default = "Logo::default_width")]
    pub width: f64,
    /// The first format's `scale`, a multiplier of the file's own pixels.
    /// Read so an old theme still parses, dropped on the next save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(default)]
    pub position: Position,
    #[serde(default)]
    pub offset: [i64; 2],
    #[serde(default, skip_serializing_if = "LogoOverride::is_empty")]
    pub login: LogoOverride,
    #[serde(default, skip_serializing_if = "LogoOverride::is_empty")]
    pub shutdown: LogoOverride,
}

/// The size and place of the logo on one screen, where it differs from the
/// unlock screen. Every field is optional; an absent one inherits.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogoOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    /// The first format's `scale`; read so an old theme still parses,
    /// ignored, dropped on the next save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<[i64; 2]>,
}

impl LogoOverride {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The logo's size and place on one screen, with the overrides resolved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// The share of the screen's width the logo takes.
    pub width: f64,
    pub position: Position,
    /// Zero unless the position is `custom`, since that is the only position
    /// the offset applies to.
    pub offset: [i64; 2],
}

impl Manifest {
    /// Forget what an older format said and this one no longer uses, so the
    /// next save writes the current format only.
    pub fn normalise(&mut self) {
        self.logo.scale = None;
        self.logo.login.scale = None;
        self.logo.shutdown.scale = None;
    }

    /// Where the logo goes on `screen`: `[logo]`, then that screen's override.
    pub fn placement(&self, screen: crate::render::Screen) -> Placement {
        use crate::render::Screen;
        let over = match screen {
            Screen::Unlock => LogoOverride::default(),
            Screen::Login => self.logo.login,
            Screen::Shutdown => self.logo.shutdown,
        };
        let position = over.position.unwrap_or(self.logo.position);
        let offset = over.offset.unwrap_or(self.logo.offset);
        Placement {
            width: over.width.unwrap_or(self.logo.width),
            position,
            offset: if position == Position::Custom {
                offset
            } else {
                [0, 0]
            },
        }
    }
}

impl Logo {
    fn default_source() -> String {
        "logo.png".to_string()
    }
    pub fn default_width() -> f64 {
        0.42
    }
}

impl Default for Logo {
    fn default() -> Self {
        Self {
            source: Self::default_source(),
            width: Self::default_width(),
            scale: None,
            position: Position::default(),
            offset: [0, 0],
            login: LogoOverride::default(),
            shutdown: LogoOverride::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum Position {
    #[default]
    Center,
    Top,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unlock {
    #[serde(default)]
    pub prompt: Prompt,
    #[serde(default)]
    pub progress: Progress,
    #[serde(default)]
    pub message: String,
}

impl Default for Unlock {
    fn default() -> Self {
        Self {
            prompt: Prompt::default(),
            progress: Progress::Bar,
            message: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum Prompt {
    #[default]
    Bullets,
    Asterisks,
    Hidden,
    Counter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum Progress {
    #[default]
    Bar,
    Spinner,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shutdown {
    #[serde(default)]
    pub logo: ShutdownLogo,
    #[serde(default)]
    pub message: String,
    #[serde(default = "Shutdown::default_progress")]
    pub progress: Progress,
}

impl Shutdown {
    fn default_progress() -> Progress {
        Progress::Spinner
    }
}

impl Default for Shutdown {
    fn default() -> Self {
        Self {
            logo: ShutdownLogo::default(),
            message: String::new(),
            progress: Self::default_progress(),
        }
    }
}

/// `logo = "inherit"` reuses the unlock logo; any other string is a file name.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum ShutdownLogo {
    #[default]
    Inherit,
    Source(String),
}

impl From<String> for ShutdownLogo {
    fn from(value: String) -> Self {
        if value == "inherit" {
            Self::Inherit
        } else {
            Self::Source(value)
        }
    }
}

impl From<ShutdownLogo> for String {
    fn from(value: ShutdownLogo) -> Self {
        match value {
            ShutdownLogo::Inherit => "inherit".to_string(),
            ShutdownLogo::Source(source) => source,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Login {
    #[serde(default)]
    pub layout: LoginLayout,
    #[serde(default = "Login::default_clock")]
    pub clock: bool,
    #[serde(default)]
    pub show_session_picker: bool,
    #[serde(default)]
    pub background: LoginBackground,
}

impl Login {
    fn default_clock() -> bool {
        true
    }
}

impl Default for Login {
    fn default() -> Self {
        Self {
            layout: LoginLayout::default(),
            clock: Self::default_clock(),
            show_session_picker: false,
            background: LoginBackground::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum LoginLayout {
    #[default]
    Centered,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum LoginBackground {
    #[default]
    Color,
    Image,
    Blur,
}

impl LoginBackground {
    /// True when the theme must ship `background.png`.
    pub fn needs_image(self) -> bool {
        matches!(self, Self::Image | Self::Blur)
    }
}

/// A validated `#rrggbb` colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// Parse `#rrggbb`. Anything else, including `#rgb` and named colours, is
    /// refused: Plymouth and QML both need the long form, and guessing is how
    /// a theme ends up a different colour than its author saw.
    pub fn parse(value: &str) -> std::result::Result<Self, String> {
        let hex = value
            .strip_prefix('#')
            .ok_or_else(|| format!("{value} does not start with '#'"))?;
        if hex.len() != 6 {
            return Err(format!(
                "{value} is {} hex digits, expected exactly 6 (#rrggbb)",
                hex.len()
            ));
        }
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!(
                "{value} contains a character that is not a hex digit"
            ));
        }
        let byte = |from: usize| u8::from_str_radix(&hex[from..from + 2], 16).expect("checked");
        Ok(Self {
            r: byte(0),
            g: byte(2),
            b: byte(4),
        })
    }

    /// `#rrggbb`, lowercase, which is the only form written into generated files.
    pub fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// The three components as Plymouth wants them, rounded the way
    /// `omarchy-plymouth-set` rounds them.
    pub fn plymouth_triplet(&self) -> (String, String, String) {
        let convert = |v: u8| format!("{:.3}", f64::from(v) / 255.0);
        (convert(self.r), convert(self.g), convert(self.b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_parses_the_long_form_only() {
        assert_eq!(
            Rgb::parse("#1a1b26").unwrap(),
            Rgb {
                r: 0x1a,
                g: 0x1b,
                b: 0x26
            }
        );
        for bad in ["1a1b26", "#1a1b2", "#1a1b2g", "#fff", "rebeccapurple", "#"] {
            assert!(Rgb::parse(bad).is_err(), "{bad} should not parse");
        }
    }

    #[test]
    fn plymouth_triplet_matches_upstream_rounding() {
        let (r, g, b) = Rgb::parse("#1a1b26").unwrap().plymouth_triplet();
        assert_eq!(
            (r.as_str(), g.as_str(), b.as_str()),
            ("0.102", "0.106", "0.149")
        );
    }

    #[test]
    fn unknown_keys_are_refused() {
        let text = r##"
[meta]
name = "x"
[colors]
background = "#000000"
backgrond = "#000000"
"##;
        let error = toml::from_str::<Manifest>(text).unwrap_err().to_string();
        assert!(error.contains("backgrond"), "{error}");
    }

    #[test]
    fn unknown_section_is_refused() {
        let text = r#"
[meta]
name = "x"
[wallpaper]
source = "x.png"
"#;
        assert!(toml::from_str::<Manifest>(text).is_err());
    }

    #[test]
    fn omitted_sections_take_defaults() {
        let manifest: Manifest = toml::from_str("[meta]\nname = \"x\"\n").unwrap();
        assert_eq!(manifest.colors, Colors::default());
        assert_eq!(manifest.unlock.progress, Progress::Bar);
        assert_eq!(manifest.shutdown.progress, Progress::Spinner);
        assert_eq!(manifest.shutdown.logo, ShutdownLogo::Inherit);
        assert!(manifest.login.clock);
    }

    #[test]
    fn unknown_enum_value_is_refused() {
        let text = "[meta]\nname = \"x\"\n[unlock]\nprompt = \"dots\"\n";
        let error = toml::from_str::<Manifest>(text).unwrap_err().to_string();
        assert!(error.contains("dots"), "{error}");
    }

    #[test]
    fn shutdown_logo_round_trips() {
        let inherit: Manifest =
            toml::from_str("[meta]\nname = \"x\"\n[shutdown]\nlogo = \"inherit\"\n").unwrap();
        assert_eq!(inherit.manifest_shutdown_logo(), &ShutdownLogo::Inherit);
        let custom: Manifest =
            toml::from_str("[meta]\nname = \"x\"\n[shutdown]\nlogo = \"bye.png\"\n").unwrap();
        assert_eq!(
            custom.manifest_shutdown_logo(),
            &ShutdownLogo::Source("bye.png".to_string())
        );
    }

    impl Manifest {
        fn manifest_shutdown_logo(&self) -> &ShutdownLogo {
            &self.shutdown.logo
        }
    }
}
