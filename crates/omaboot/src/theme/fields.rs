//! The properties the inspector shows, one enum rather than a form.
//!
//! Each screen shows only its own fields, which is what `docs/UI.md` asks for:
//! no mega-form containing everything. Reading and writing both go through
//! here, so the inspector cannot show a value the manifest does not have.

use crate::render::Screen;
use crate::theme::{
    LoginBackground, LoginLayout, LogoOverride, Manifest, Position, Progress, Prompt, Rgb,
    ShutdownLogo,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldId {
    Background,
    Foreground,
    Accent,
    ErrorColour,
    LogoSource,
    LogoWidth,
    LogoPosition,
    LogoOffsetX,
    LogoOffsetY,
    /// The logo's size on the login screen, when it differs from unlock.
    LoginLogoWidth,
    /// The logo's size and place on the shutdown screen, when they differ.
    ShutdownLogoWidth,
    ShutdownLogoPosition,
    ShutdownLogoOffsetX,
    ShutdownLogoOffsetY,
    UnlockPrompt,
    UnlockProgress,
    UnlockMessage,
    ShutdownLogo,
    ShutdownMessage,
    ShutdownProgress,
    LoginLayout,
    LoginClock,
    LoginSessionPicker,
    LoginBackgroundKind,
}

/// How a value is edited. The kind decides which keys do something.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Colour,
    /// A file in the theme directory. The inspector offers the images that
    /// are actually there, so nobody has to guess a file name.
    File,
    Text {
        max: usize,
    },
    Number {
        min: f64,
        max: f64,
        fine: f64,
        coarse: f64,
        integer: bool,
    },
    Toggle,
    Choice(&'static [&'static str]),
}

impl FieldId {
    /// The fields of one tab, in the order they are shown.
    pub fn for_screen(screen: Screen) -> Vec<FieldId> {
        let mut fields = vec![
            FieldId::Background,
            FieldId::Foreground,
            FieldId::Accent,
            FieldId::ErrorColour,
        ];
        // The logo's size and place are per screen: what is set on the unlock
        // tab is the default, the other tabs show their own value.
        match screen {
            Screen::Unlock => fields.extend([
                FieldId::LogoSource,
                FieldId::LogoWidth,
                FieldId::LogoPosition,
                FieldId::LogoOffsetX,
                FieldId::LogoOffsetY,
                FieldId::UnlockPrompt,
                FieldId::UnlockProgress,
                FieldId::UnlockMessage,
            ]),
            Screen::Shutdown => fields.extend([
                FieldId::ShutdownLogo,
                FieldId::ShutdownLogoWidth,
                FieldId::ShutdownLogoPosition,
                FieldId::ShutdownLogoOffsetX,
                FieldId::ShutdownLogoOffsetY,
                FieldId::ShutdownProgress,
                FieldId::ShutdownMessage,
            ]),
            Screen::Login => fields.extend([
                FieldId::LogoSource,
                FieldId::LoginLogoWidth,
                FieldId::LoginLayout,
                FieldId::LoginBackgroundKind,
                FieldId::LoginClock,
                FieldId::LoginSessionPicker,
            ]),
        }
        fields
    }

    /// Every field, in inspector order.
    pub const ALL: [FieldId; 24] = [
        FieldId::Background,
        FieldId::Foreground,
        FieldId::Accent,
        FieldId::ErrorColour,
        FieldId::LogoSource,
        FieldId::LogoWidth,
        FieldId::LogoPosition,
        FieldId::LogoOffsetX,
        FieldId::LogoOffsetY,
        FieldId::LoginLogoWidth,
        FieldId::ShutdownLogo,
        FieldId::ShutdownLogoWidth,
        FieldId::ShutdownLogoPosition,
        FieldId::ShutdownLogoOffsetX,
        FieldId::ShutdownLogoOffsetY,
        FieldId::UnlockPrompt,
        FieldId::UnlockProgress,
        FieldId::UnlockMessage,
        FieldId::ShutdownProgress,
        FieldId::ShutdownMessage,
        FieldId::LoginLayout,
        FieldId::LoginBackgroundKind,
        FieldId::LoginClock,
        FieldId::LoginSessionPicker,
    ];

    /// The dotted key of this field in theme.toml, which is also how the
    /// command line and the plugin name it.
    pub fn key(self) -> &'static str {
        match self {
            FieldId::Background => "colors.background",
            FieldId::Foreground => "colors.foreground",
            FieldId::Accent => "colors.accent",
            FieldId::ErrorColour => "colors.error",
            FieldId::LogoSource => "logo.source",
            FieldId::LogoWidth => "logo.width",
            FieldId::LogoPosition => "logo.position",
            FieldId::LogoOffsetX => "logo.offset.x",
            FieldId::LogoOffsetY => "logo.offset.y",
            FieldId::LoginLogoWidth => "logo.login.width",
            FieldId::ShutdownLogoWidth => "logo.shutdown.width",
            FieldId::ShutdownLogoPosition => "logo.shutdown.position",
            FieldId::ShutdownLogoOffsetX => "logo.shutdown.offset.x",
            FieldId::ShutdownLogoOffsetY => "logo.shutdown.offset.y",
            FieldId::UnlockPrompt => "unlock.prompt",
            FieldId::UnlockProgress => "unlock.progress",
            FieldId::UnlockMessage => "unlock.message",
            FieldId::ShutdownLogo => "shutdown.logo",
            FieldId::ShutdownProgress => "shutdown.progress",
            FieldId::ShutdownMessage => "shutdown.message",
            FieldId::LoginLayout => "login.layout",
            FieldId::LoginBackgroundKind => "login.background",
            FieldId::LoginClock => "login.clock",
            FieldId::LoginSessionPicker => "login.show_session_picker",
        }
    }

    pub fn parse(key: &str) -> Option<FieldId> {
        FieldId::ALL.into_iter().find(|field| field.key() == key)
    }

    /// The heading this field sits under.
    pub fn group(self) -> &'static str {
        match self {
            Self::Background | Self::Foreground | Self::Accent | Self::ErrorColour => "Colours",
            Self::LogoSource
            | Self::LogoWidth
            | Self::LogoPosition
            | Self::LogoOffsetX
            | Self::LogoOffsetY
            | Self::LoginLogoWidth
            | Self::ShutdownLogo
            | Self::ShutdownLogoWidth
            | Self::ShutdownLogoPosition
            | Self::ShutdownLogoOffsetX
            | Self::ShutdownLogoOffsetY => "Logo",
            Self::UnlockPrompt | Self::UnlockProgress | Self::UnlockMessage => "Unlock",
            Self::ShutdownProgress | Self::ShutdownMessage => "Shutdown",
            Self::LoginLayout
            | Self::LoginBackgroundKind
            | Self::LoginClock
            | Self::LoginSessionPicker => "Login",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Background => "Background",
            Self::Foreground => "Foreground",
            Self::Accent => "Accent",
            Self::ErrorColour => "Error",
            Self::LogoSource => "Source",
            Self::LogoWidth | Self::LoginLogoWidth | Self::ShutdownLogoWidth => "Width",
            Self::LogoPosition | Self::ShutdownLogoPosition => "Position",
            Self::LogoOffsetX | Self::ShutdownLogoOffsetX => "Offset X",
            Self::LogoOffsetY | Self::ShutdownLogoOffsetY => "Offset Y",
            Self::UnlockPrompt => "Prompt",
            Self::UnlockProgress => "Progress",
            Self::UnlockMessage => "Message",
            Self::ShutdownLogo => "Source",
            Self::ShutdownProgress => "Progress",
            Self::ShutdownMessage => "Message",
            Self::LoginLayout => "Layout",
            Self::LoginBackgroundKind => "Background",
            Self::LoginClock => "Clock",
            Self::LoginSessionPicker => "Session picker",
        }
    }

    pub fn kind(self) -> Kind {
        match self {
            Self::Background | Self::Foreground | Self::Accent | Self::ErrorColour => Kind::Colour,
            Self::LogoSource | Self::ShutdownLogo => Kind::File,
            Self::UnlockMessage | Self::ShutdownMessage => Kind::Text {
                max: crate::theme::MAX_MESSAGE_CHARS,
            },
            Self::LogoWidth | Self::LoginLogoWidth | Self::ShutdownLogoWidth => Kind::Number {
                min: crate::theme::validate::MIN_WIDTH,
                max: crate::theme::validate::MAX_WIDTH,
                fine: 0.01,
                coarse: 0.05,
                integer: false,
            },
            Self::LogoOffsetX
            | Self::LogoOffsetY
            | Self::ShutdownLogoOffsetX
            | Self::ShutdownLogoOffsetY => Kind::Number {
                min: -4000.0,
                max: 4000.0,
                fine: 1.0,
                coarse: 10.0,
                integer: true,
            },
            Self::LogoPosition | Self::ShutdownLogoPosition => {
                Kind::Choice(&["center", "top", "custom"])
            }
            Self::UnlockPrompt => Kind::Choice(&["bullets", "asterisks", "hidden", "counter"]),
            Self::UnlockProgress | Self::ShutdownProgress => {
                Kind::Choice(&["bar", "spinner", "none"])
            }
            Self::LoginLayout => Kind::Choice(&["centered", "left", "right"]),
            Self::LoginBackgroundKind => Kind::Choice(&["color", "image", "blur"]),
            Self::LoginClock | Self::LoginSessionPicker => Kind::Toggle,
        }
    }

    pub fn read(self, manifest: &Manifest) -> String {
        match self {
            Self::Background => manifest.colors.background.clone(),
            Self::Foreground => manifest.colors.foreground.clone(),
            Self::Accent => manifest.colors.accent.clone(),
            Self::ErrorColour => manifest.colors.error.clone(),
            Self::LogoSource => manifest.logo.source.clone(),
            Self::LogoWidth => format!("{:.2}", manifest.logo.width),
            Self::LogoPosition => position_name(manifest.logo.position).to_string(),
            Self::LogoOffsetX => manifest.logo.offset[0].to_string(),
            Self::LogoOffsetY => manifest.logo.offset[1].to_string(),
            Self::LoginLogoWidth => format!("{:.2}", manifest.placement(Screen::Login).width),
            Self::ShutdownLogoWidth => {
                format!("{:.2}", manifest.placement(Screen::Shutdown).width)
            }
            Self::ShutdownLogoPosition => {
                position_name(manifest.placement(Screen::Shutdown).position).to_string()
            }
            Self::ShutdownLogoOffsetX => manifest.placement(Screen::Shutdown).offset[0].to_string(),
            Self::ShutdownLogoOffsetY => manifest.placement(Screen::Shutdown).offset[1].to_string(),
            Self::UnlockPrompt => match manifest.unlock.prompt {
                Prompt::Bullets => "bullets",
                Prompt::Asterisks => "asterisks",
                Prompt::Hidden => "hidden",
                Prompt::Counter => "counter",
            }
            .to_string(),
            Self::UnlockProgress => progress_name(manifest.unlock.progress).to_string(),
            Self::UnlockMessage => manifest.unlock.message.clone(),
            Self::ShutdownLogo => match &manifest.shutdown.logo {
                ShutdownLogo::Inherit => "inherit".to_string(),
                ShutdownLogo::Source(source) => source.clone(),
            },
            Self::ShutdownProgress => progress_name(manifest.shutdown.progress).to_string(),
            Self::ShutdownMessage => manifest.shutdown.message.clone(),
            Self::LoginLayout => match manifest.login.layout {
                LoginLayout::Centered => "centered",
                LoginLayout::Left => "left",
                LoginLayout::Right => "right",
            }
            .to_string(),
            Self::LoginBackgroundKind => match manifest.login.background {
                LoginBackground::Color => "color",
                LoginBackground::Image => "image",
                LoginBackground::Blur => "blur",
            }
            .to_string(),
            Self::LoginClock => bool_name(manifest.login.clock).to_string(),
            Self::LoginSessionPicker => bool_name(manifest.login.show_session_picker).to_string(),
        }
    }

    /// Write a value, or say why it cannot be written.
    ///
    /// The message is shown on the status line, so it is a sentence a person
    /// can act on rather than a parser's complaint.
    pub fn write(self, manifest: &mut Manifest, value: &str) -> Result<(), String> {
        let value = value.trim();
        match self {
            Self::Background | Self::Foreground | Self::Accent | Self::ErrorColour => {
                let normalised = normalise_colour(value)?;
                match self {
                    Self::Background => manifest.colors.background = normalised,
                    Self::Foreground => manifest.colors.foreground = normalised,
                    Self::Accent => manifest.colors.accent = normalised,
                    _ => manifest.colors.error = normalised,
                }
            }
            Self::LogoSource => {
                text(value, 120)?;
                if value.is_empty() {
                    return Err("the logo needs a file name".to_string());
                }
                manifest.logo.source = value.to_string();
            }
            Self::LogoWidth => {
                manifest.logo.width = width(value)?;
            }
            Self::LogoPosition => {
                manifest.logo.position = position_value(value)?;
                if manifest.logo.position != Position::Custom {
                    // An offset that cannot take effect is refused by the
                    // validator, so changing away from custom clears it
                    // rather than leaving a theme that will not apply.
                    manifest.logo.offset = [0, 0];
                }
            }
            Self::LogoOffsetX | Self::LogoOffsetY => {
                let parsed = number(value, -4000.0, 4000.0, true)? as i64;
                if parsed != 0 && manifest.logo.position != Position::Custom {
                    return Err(
                        "set Position to custom first, an offset is ignored otherwise".to_string(),
                    );
                }
                if self == Self::LogoOffsetX {
                    manifest.logo.offset[0] = parsed;
                } else {
                    manifest.logo.offset[1] = parsed;
                }
            }
            Self::LoginLogoWidth => {
                manifest.logo.login.width = Some(width(value)?);
            }
            Self::ShutdownLogoWidth => {
                manifest.logo.shutdown.width = Some(width(value)?);
            }
            Self::ShutdownLogoPosition => {
                let position = position_value(value)?;
                let over: &mut LogoOverride = &mut manifest.logo.shutdown;
                over.position = Some(position);
                if position != Position::Custom {
                    over.offset = Some([0, 0]);
                }
            }
            Self::ShutdownLogoOffsetX | Self::ShutdownLogoOffsetY => {
                let parsed = number(value, -4000.0, 4000.0, true)? as i64;
                let effective = manifest.placement(Screen::Shutdown);
                if parsed != 0 && effective.position != Position::Custom {
                    return Err(
                        "set Position to custom first, an offset is ignored otherwise".to_string(),
                    );
                }
                let mut offset = effective.offset;
                if self == Self::ShutdownLogoOffsetX {
                    offset[0] = parsed;
                } else {
                    offset[1] = parsed;
                }
                manifest.logo.shutdown.offset = Some(offset);
            }
            Self::UnlockPrompt => {
                manifest.unlock.prompt = match value {
                    "bullets" => Prompt::Bullets,
                    "asterisks" => Prompt::Asterisks,
                    "hidden" => Prompt::Hidden,
                    "counter" => Prompt::Counter,
                    other => return Err(format!("{other} is not a prompt style")),
                }
            }
            Self::UnlockProgress => manifest.unlock.progress = progress(value)?,
            Self::ShutdownProgress => manifest.shutdown.progress = progress(value)?,
            Self::UnlockMessage => {
                text(value, crate::theme::MAX_MESSAGE_CHARS)?;
                manifest.unlock.message = value.to_string();
            }
            Self::ShutdownMessage => {
                text(value, crate::theme::MAX_MESSAGE_CHARS)?;
                manifest.shutdown.message = value.to_string();
            }
            Self::ShutdownLogo => {
                text(value, 120)?;
                manifest.shutdown.logo = if value.is_empty() || value == "inherit" {
                    ShutdownLogo::Inherit
                } else {
                    ShutdownLogo::Source(value.to_string())
                };
            }
            Self::LoginLayout => {
                manifest.login.layout = match value {
                    "centered" => LoginLayout::Centered,
                    "left" => LoginLayout::Left,
                    "right" => LoginLayout::Right,
                    other => return Err(format!("{other} is not a layout")),
                }
            }
            Self::LoginBackgroundKind => {
                manifest.login.background = match value {
                    "color" => LoginBackground::Color,
                    "image" => LoginBackground::Image,
                    "blur" => LoginBackground::Blur,
                    other => return Err(format!("{other} is not a background")),
                }
            }
            Self::LoginClock => manifest.login.clock = boolean(value)?,
            Self::LoginSessionPicker => manifest.login.show_session_picker = boolean(value)?,
        }
        Ok(())
    }

    /// Move a choice or a toggle along by one, which is what the arrow keys do
    /// without opening an editor.
    pub fn cycle(self, manifest: &mut Manifest, forward: bool) -> Result<(), String> {
        match self.kind() {
            Kind::Choice(options) => {
                let current = self.read(manifest);
                let index = options.iter().position(|o| *o == current).unwrap_or(0);
                let next = if forward {
                    (index + 1) % options.len()
                } else {
                    (index + options.len() - 1) % options.len()
                };
                self.write(manifest, options[next])
            }
            Kind::Toggle => {
                let current = self.read(manifest) == "on";
                self.write(manifest, bool_name(!current))
            }
            Kind::File => Err("pick the file with h and l".to_string()),
            Kind::Number {
                min,
                max,
                fine,
                coarse,
                integer,
            } => {
                let step = if forward { fine } else { -fine };
                let _ = coarse;
                let current: f64 = self.read(manifest).parse().unwrap_or(0.0);
                let next = (current + step).clamp(min, max);
                self.write(manifest, &format_number(next, integer))
            }
            _ => Err(format!("{} is typed, not cycled", self.label())),
        }
    }

    /// The same, in coarse steps, for the capital movement keys.
    pub fn nudge(self, manifest: &mut Manifest, forward: bool) -> Result<(), String> {
        match self.kind() {
            Kind::Number {
                min,
                max,
                coarse,
                integer,
                ..
            } => {
                let current: f64 = self.read(manifest).parse().unwrap_or(0.0);
                let step = if forward { coarse } else { -coarse };
                let next = (current + step).clamp(min, max);
                self.write(manifest, &format_number(next, integer))
            }
            _ => self.cycle(manifest, forward),
        }
    }
}

fn format_number(value: f64, integer: bool) -> String {
    if integer {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.2}")
    }
}

/// The logo's share of the screen width, within the validator's range.
fn width(value: &str) -> Result<f64, String> {
    number(
        value,
        crate::theme::validate::MIN_WIDTH,
        crate::theme::validate::MAX_WIDTH,
        false,
    )
}

fn position_name(position: Position) -> &'static str {
    match position {
        Position::Center => "center",
        Position::Top => "top",
        Position::Custom => "custom",
    }
}

fn position_value(value: &str) -> Result<Position, String> {
    match value {
        "center" => Ok(Position::Center),
        "top" => Ok(Position::Top),
        "custom" => Ok(Position::Custom),
        other => Err(format!("{other} is not a position")),
    }
}

fn progress_name(progress: Progress) -> &'static str {
    match progress {
        Progress::Bar => "bar",
        Progress::Spinner => "spinner",
        Progress::None => "none",
    }
}

fn progress(value: &str) -> Result<Progress, String> {
    match value {
        "bar" => Ok(Progress::Bar),
        "spinner" => Ok(Progress::Spinner),
        "none" => Ok(Progress::None),
        other => Err(format!("{other} is not a progress style")),
    }
}

fn bool_name(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn boolean(value: &str) -> Result<bool, String> {
    match value {
        "on" | "true" | "yes" => Ok(true),
        "off" | "false" | "no" => Ok(false),
        other => Err(format!("{other} is not on or off")),
    }
}

fn text(value: &str, max: usize) -> Result<(), String> {
    if value.chars().any(char::is_control) {
        return Err("that contains a control character".to_string());
    }
    if value.chars().count() > max {
        return Err(format!("that is longer than {max} characters"));
    }
    Ok(())
}

fn number(value: &str, min: f64, max: f64, integer: bool) -> Result<f64, String> {
    let parsed: f64 = value
        .parse()
        .map_err(|_| format!("{value} is not a number"))?;
    if !parsed.is_finite() || parsed < min || parsed > max {
        return Err(format!("{value} is outside {min} to {max}"));
    }
    Ok(if integer { parsed.round() } else { parsed })
}

/// The images in a theme directory, sorted, as the logo fields offer them.
pub fn image_files(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = match std::fs::read_dir(dir) {
        Err(_) => Vec::new(),
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_file())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| {
                let lower = name.to_ascii_lowercase();
                [".png", ".svg", ".jpg", ".jpeg"]
                    .iter()
                    .any(|extension| lower.ends_with(extension))
            })
            .collect(),
    };
    names.sort();
    names
}

/// Step through a list of candidates, starting from the current value.
pub fn step(candidates: &[String], current: &str, forward: bool) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }
    let index = candidates.iter().position(|name| name == current);
    let next = match index {
        None => 0,
        Some(index) if forward => (index + 1) % candidates.len(),
        Some(index) => (index + candidates.len() - 1) % candidates.len(),
    };
    Some(candidates[next].clone())
}

/// Accept `1a1b26` as well as `#1a1b26`, because people paste both.
fn normalise_colour(value: &str) -> Result<String, String> {
    let candidate = if value.starts_with('#') {
        value.to_string()
    } else {
        format!("#{value}")
    };
    Rgb::parse(&candidate).map_err(|what| what.to_string())?;
    Ok(candidate.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        toml::from_str("[meta]\nname = \"t\"\n").unwrap()
    }

    #[test]
    fn each_tab_shows_its_own_fields_and_the_shared_ones() {
        let unlock = FieldId::for_screen(Screen::Unlock);
        let login = FieldId::for_screen(Screen::Login);
        assert!(unlock.contains(&FieldId::UnlockPrompt));
        assert!(!unlock.contains(&FieldId::LoginClock));
        assert!(login.contains(&FieldId::LoginClock));
        assert!(!login.contains(&FieldId::ShutdownMessage));
        // Colours are on every tab, because they are the whole theme.
        assert!(login.contains(&FieldId::Background));
    }

    #[test]
    fn a_value_read_back_is_the_value_written() {
        let mut manifest = manifest();
        FieldId::Background.write(&mut manifest, "#abcdef").unwrap();
        assert_eq!(FieldId::Background.read(&manifest), "#abcdef");
    }

    #[test]
    fn a_colour_may_be_pasted_without_its_hash() {
        let mut manifest = manifest();
        FieldId::Accent.write(&mut manifest, "AABBCC").unwrap();
        assert_eq!(FieldId::Accent.read(&manifest), "#aabbcc");
    }

    #[test]
    fn a_colour_that_is_not_a_colour_says_so() {
        let mut manifest = manifest();
        let error = FieldId::Accent.write(&mut manifest, "blue").unwrap_err();
        assert!(error.contains("hex"), "{error}");
        assert_eq!(
            FieldId::Accent.read(&manifest),
            "#7aa2f7",
            "nothing changed"
        );
    }

    #[test]
    fn a_choice_cycles_and_wraps() {
        let mut manifest = manifest();
        assert_eq!(FieldId::UnlockPrompt.read(&manifest), "bullets");
        FieldId::UnlockPrompt.cycle(&mut manifest, true).unwrap();
        assert_eq!(FieldId::UnlockPrompt.read(&manifest), "asterisks");
        for _ in 0..3 {
            FieldId::UnlockPrompt.cycle(&mut manifest, true).unwrap();
        }
        assert_eq!(FieldId::UnlockPrompt.read(&manifest), "bullets");
        FieldId::UnlockPrompt.cycle(&mut manifest, false).unwrap();
        assert_eq!(FieldId::UnlockPrompt.read(&manifest), "counter");
    }

    #[test]
    fn a_toggle_toggles() {
        let mut manifest = manifest();
        assert_eq!(FieldId::LoginClock.read(&manifest), "on");
        FieldId::LoginClock.cycle(&mut manifest, true).unwrap();
        assert_eq!(FieldId::LoginClock.read(&manifest), "off");
    }

    #[test]
    fn a_number_moves_in_fine_and_coarse_steps_and_clamps() {
        let mut manifest = manifest();
        FieldId::LogoWidth.cycle(&mut manifest, true).unwrap();
        assert_eq!(FieldId::LogoWidth.read(&manifest), "0.43");
        FieldId::LogoWidth.nudge(&mut manifest, true).unwrap();
        assert_eq!(FieldId::LogoWidth.read(&manifest), "0.48");
        for _ in 0..50 {
            FieldId::LogoWidth.nudge(&mut manifest, true).unwrap();
        }
        assert_eq!(
            FieldId::LogoWidth.read(&manifest),
            "0.95",
            "clamped at the top"
        );
    }

    #[test]
    fn an_offset_needs_the_custom_position_and_says_which() {
        let mut manifest = manifest();
        let error = FieldId::LogoOffsetY
            .write(&mut manifest, "-40")
            .unwrap_err();
        assert!(error.contains("custom"), "{error}");

        FieldId::LogoPosition
            .write(&mut manifest, "custom")
            .unwrap();
        FieldId::LogoOffsetY.write(&mut manifest, "-40").unwrap();
        assert_eq!(FieldId::LogoOffsetY.read(&manifest), "-40");
    }

    #[test]
    fn leaving_the_custom_position_clears_the_offset_rather_than_stranding_it() {
        let mut manifest = manifest();
        FieldId::LogoPosition
            .write(&mut manifest, "custom")
            .unwrap();
        FieldId::LogoOffsetX.write(&mut manifest, "25").unwrap();
        FieldId::LogoPosition
            .write(&mut manifest, "center")
            .unwrap();
        assert_eq!(FieldId::LogoOffsetX.read(&manifest), "0");
    }

    #[test]
    fn a_message_is_bounded_and_refuses_control_characters() {
        let mut manifest = manifest();
        assert!(
            FieldId::ShutdownMessage
                .write(&mut manifest, &"x".repeat(200))
                .is_err()
        );
        assert!(
            FieldId::ShutdownMessage
                .write(&mut manifest, "two\nlines")
                .is_err()
        );
        FieldId::ShutdownMessage
            .write(&mut manifest, "See you")
            .unwrap();
        assert_eq!(FieldId::ShutdownMessage.read(&manifest), "See you");
    }

    #[test]
    fn the_shutdown_logo_falls_back_to_inherit_when_emptied() {
        let mut manifest = manifest();
        FieldId::ShutdownLogo
            .write(&mut manifest, "bye.png")
            .unwrap();
        assert_eq!(FieldId::ShutdownLogo.read(&manifest), "bye.png");
        FieldId::ShutdownLogo.write(&mut manifest, "").unwrap();
        assert_eq!(FieldId::ShutdownLogo.read(&manifest), "inherit");
    }

    #[test]
    fn the_logo_fields_offer_the_images_that_are_there() {
        let tmp = tempfile::tempdir().unwrap();
        for name in ["logo.png", "bye.svg", "theme.toml", "notes.txt", "alt.PNG"] {
            std::fs::write(tmp.path().join(name), b"x").unwrap();
        }
        let files = image_files(tmp.path());
        assert_eq!(files, vec!["alt.PNG", "bye.svg", "logo.png"]);

        assert_eq!(step(&files, "logo.png", true).as_deref(), Some("alt.PNG"));
        assert_eq!(step(&files, "logo.png", false).as_deref(), Some("bye.svg"));
        // A value that is not among them lands on the first candidate.
        assert_eq!(step(&files, "gone.png", true).as_deref(), Some("alt.PNG"));
        assert_eq!(step(&[], "logo.png", true), None);
    }

    #[test]
    fn the_logo_is_placed_per_screen_and_follows_unlock_until_a_screen_differs() {
        let mut manifest = manifest();
        FieldId::LogoWidth.write(&mut manifest, "0.65").unwrap();
        // The other screens follow the unlock screen...
        assert_eq!(FieldId::LoginLogoWidth.read(&manifest), "0.65");
        assert_eq!(FieldId::ShutdownLogoWidth.read(&manifest), "0.65");
        // ...until they are given their own value, which leaves unlock alone.
        FieldId::ShutdownLogoWidth
            .write(&mut manifest, "0.30")
            .unwrap();
        assert_eq!(FieldId::LogoWidth.read(&manifest), "0.65");
        assert_eq!(FieldId::LoginLogoWidth.read(&manifest), "0.65");
        assert_eq!(FieldId::ShutdownLogoWidth.read(&manifest), "0.30");
        // The file only carries what differs.
        let text = toml::to_string(&manifest).unwrap();
        assert!(text.contains("[logo.shutdown]"), "{text}");
        assert!(!text.contains("[logo.login]"), "{text}");
        let back: Manifest = toml::from_str(&text).unwrap();
        assert_eq!(back, manifest);
    }

    #[test]
    fn a_shutdown_offset_needs_the_shutdown_position_to_be_custom() {
        let mut manifest = manifest();
        FieldId::LogoPosition
            .write(&mut manifest, "custom")
            .unwrap();
        FieldId::LogoOffsetX.write(&mut manifest, "10").unwrap();
        // Inherited: custom with an offset of 10.
        assert_eq!(FieldId::ShutdownLogoOffsetX.read(&manifest), "10");
        FieldId::ShutdownLogoOffsetY
            .write(&mut manifest, "-5")
            .unwrap();
        assert_eq!(FieldId::ShutdownLogoOffsetX.read(&manifest), "10");
        assert_eq!(FieldId::ShutdownLogoOffsetY.read(&manifest), "-5");
        // Leaving custom on the shutdown screen zeroes its offset, and an
        // offset cannot then be set until the position is custom again.
        FieldId::ShutdownLogoPosition
            .write(&mut manifest, "top")
            .unwrap();
        assert_eq!(FieldId::ShutdownLogoOffsetX.read(&manifest), "0");
        assert!(
            FieldId::ShutdownLogoOffsetX
                .write(&mut manifest, "3")
                .is_err()
        );
        assert_eq!(
            FieldId::LogoOffsetX.read(&manifest),
            "10",
            "unlock keeps its own"
        );
    }

    #[test]
    fn every_field_round_trips_through_its_own_reader() {
        // Whatever a field reads must be something it accepts back, or the
        // inspector would show a value it cannot save.
        let manifest = manifest();
        for screen in [Screen::Unlock, Screen::Login, Screen::Shutdown] {
            for field in FieldId::for_screen(screen) {
                let value = field.read(&manifest);
                let mut copy = manifest.clone();
                field
                    .write(&mut copy, &value)
                    .unwrap_or_else(|error| panic!("{:?}: {error}", field));
                assert_eq!(field.read(&copy), value, "{field:?} did not round trip");
            }
        }
    }
}
