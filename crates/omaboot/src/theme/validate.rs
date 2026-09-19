//! Validation of a parsed theme against its directory.
//!
//! The rules here mirror the hardening in `omarchy-plymouth-set`: assets are
//! regular files, never symlinks, never empty, never larger than the ceiling
//! upstream enforces, and always inside the theme directory.

use std::path::PathBuf;

use crate::error::{Error, Result};

use super::{LOGIN_BACKGROUND, Position, Rgb, ShutdownLogo, Theme, ValidTheme};

/// The size ceiling `omarchy-plymouth-set` passes into its privileged section.
pub const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;
/// A message has to fit on a boot screen, so it is bounded here rather than
/// discovered to be too long at 3am on a console.
pub const MAX_MESSAGE_CHARS: usize = 120;
/// Anything longer is a description, not a name.
pub const MAX_NAME_CHARS: usize = 64;
/// Beyond this the logo is either invisible or off screen.
/// The logo's share of the screen width: from a small mark to nearly edge
/// to edge.
pub const MIN_WIDTH: f64 = 0.02;
pub const MAX_WIDTH: f64 = 0.95;
/// No display omaboot supports is wider than this, so an offset past it is a
/// typo rather than an intention.
pub const MAX_OFFSET: i64 = 4000;

pub(super) fn validate(theme: Theme) -> Result<ValidTheme> {
    let manifest = theme.manifest.clone();
    let dir = theme.dir.clone();

    let invalid = |what: String, suggestion: &str| Error::InvalidTheme {
        path: dir.join(super::MANIFEST),
        what,
        suggestion: suggestion.to_string(),
    };

    // meta
    let name = manifest.meta.name.trim();
    if name.is_empty() {
        return Err(invalid(
            "meta.name is empty".to_string(),
            "give the theme a name, it is what the boot menu and the theme list show",
        ));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(invalid(
            format!(
                "meta.name is {} characters, the limit is {MAX_NAME_CHARS}",
                name.chars().count()
            ),
            "shorten the name",
        ));
    }
    printable("meta.name", name).map_err(|what| {
        invalid(
            what,
            "remove the control character; names are written into desktop files",
        )
    })?;
    printable("meta.author", &manifest.meta.author)
        .map_err(|what| invalid(what, "remove the control character"))?;
    if !is_version(&manifest.meta.version) {
        return Err(invalid(
            format!(
                "meta.version {:?} is not a major.minor.patch version",
                manifest.meta.version
            ),
            "write a version such as 1.0.0",
        ));
    }

    // colors
    for (key, value) in [
        ("background", &manifest.colors.background),
        ("foreground", &manifest.colors.foreground),
        ("accent", &manifest.colors.accent),
        ("error", &manifest.colors.error),
    ] {
        Rgb::parse(value).map_err(|what| {
            invalid(
                format!("colors.{key}: {what}"),
                "write the colour as #rrggbb",
            )
        })?;
    }

    // logo, on the unlock screen and wherever a screen overrides it
    let base = (
        "logo",
        manifest.logo.width,
        manifest.logo.position,
        manifest.logo.offset,
    );
    let overrides = [
        ("logo.login", &manifest.logo.login),
        ("logo.shutdown", &manifest.logo.shutdown),
    ]
    .into_iter()
    .map(|(name, over)| {
        (
            name,
            over.width.unwrap_or(manifest.logo.width),
            over.position.unwrap_or(manifest.logo.position),
            over.offset.unwrap_or(manifest.logo.offset),
        )
    });
    for (name, width, position, offset) in std::iter::once(base).chain(overrides) {
        if !width.is_finite() || !(MIN_WIDTH..=MAX_WIDTH).contains(&width) {
            return Err(invalid(
                format!("{name}.width {width} is outside {MIN_WIDTH}..={MAX_WIDTH}"),
                "the width is the logo's share of the screen width, for example 0.4",
            ));
        }
        for (axis, value) in ["x", "y"].iter().zip(offset) {
            if value.abs() > MAX_OFFSET {
                return Err(invalid(
                    format!("{name}.offset {axis} is {value}, the limit is +/-{MAX_OFFSET}"),
                    "use an offset that keeps the logo on the screen",
                ));
            }
        }
        if position != Position::Custom && offset != [0, 0] {
            return Err(invalid(
                format!(
                    "{name}.offset is set but {name}.position is {position:?}, so the offset would be ignored"
                ),
                "set position = \"custom\" to use the offset, or remove the offset",
            ));
        }
    }
    // An SVG is accepted here and rasterised during generation, which is also
    // where a malformed one is reported: parsing it twice would be the only
    // way to report it earlier.
    let logo = asset(&theme, &manifest.logo.source, "logo.source")?;

    // unlock and shutdown messages
    message(&manifest.unlock.message, "unlock.message")
        .map_err(|what| invalid(what, "shorten the message, or remove the control character"))?;
    message(&manifest.shutdown.message, "shutdown.message")
        .map_err(|what| invalid(what, "shorten the message, or remove the control character"))?;

    let shutdown_logo = match &manifest.shutdown.logo {
        ShutdownLogo::Inherit => logo.clone(),
        ShutdownLogo::Source(source) => asset(&theme, source, "shutdown.logo")?,
    };

    // login
    let login_background = if manifest.login.background.needs_image() {
        let path = theme.dir.join(LOGIN_BACKGROUND);
        if !path.exists() {
            return Err(invalid(
                format!(
                    "login.background is {:?} but {LOGIN_BACKGROUND} is not in the theme directory",
                    manifest.login.background
                ),
                "add background.png, or set login.background = \"color\"",
            ));
        }
        Some(asset(&theme, LOGIN_BACKGROUND, "login.background")?)
    } else {
        None
    };

    Ok(ValidTheme::new(
        theme,
        logo,
        login_background,
        shutdown_logo,
    ))
}

/// Resolve and check one asset: inside the theme, a regular file, not a
/// symlink, not empty, not oversized.
fn asset(theme: &Theme, reference: &str, key: &str) -> Result<PathBuf> {
    let path = theme.asset_path(reference)?;
    let invalid = |what: String, suggestion: &str| Error::InvalidTheme {
        path: theme.dir.join(super::MANIFEST),
        what,
        suggestion: suggestion.to_string(),
    };

    let metadata = std::fs::symlink_metadata(&path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            invalid(
                format!("{key} points at {reference}, which is not in the theme directory"),
                "add the file, or correct the reference",
            )
        } else {
            Error::read(path.clone(), source)
        }
    })?;

    if metadata.file_type().is_symlink() {
        return Err(invalid(
            format!("{key} points at {reference}, which is a symlink"),
            "replace the symlink with the file itself; a symlink could be swapped between validation and install",
        ));
    }
    if !metadata.is_file() {
        return Err(invalid(
            format!("{key} points at {reference}, which is not a regular file"),
            "point the reference at a file",
        ));
    }
    if metadata.len() == 0 {
        return Err(invalid(
            format!("{key} points at {reference}, which is empty"),
            "replace it with the real asset",
        ));
    }
    if metadata.len() > MAX_ASSET_BYTES {
        return Err(invalid(
            format!(
                "{key} points at {reference}, which is {} bytes; the limit is {MAX_ASSET_BYTES}",
                metadata.len()
            ),
            "shrink the asset; the limit matches the one the Omarchy publisher enforces",
        ));
    }
    Ok(path)
}

fn printable(key: &str, value: &str) -> std::result::Result<(), String> {
    if let Some(bad) = value.chars().find(|c| c.is_control()) {
        return Err(format!("{key} contains the control character {:?}", bad));
    }
    Ok(())
}

fn message(value: &str, key: &str) -> std::result::Result<(), String> {
    printable(key, value)?;
    if value.chars().count() > MAX_MESSAGE_CHARS {
        return Err(format!(
            "{key} is {} characters, the limit is {MAX_MESSAGE_CHARS}",
            value.chars().count()
        ));
    }
    Ok(())
}

fn is_version(value: &str) -> bool {
    let core = value.split(['-', '+']).next().unwrap_or_default();
    let mut parts = core.split('.');
    let ok = |part: Option<&str>| {
        part.is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) && p.len() <= 9)
    };
    let (major, minor, patch) = (parts.next(), parts.next(), parts.next());
    ok(major) && ok(minor) && ok(patch) && parts.next().is_none()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;
    use crate::theme::{Logo, Manifest, Theme};

    /// Write a theme directory with the given manifest body and a logo.
    fn theme_dir(dir: &Path, manifest: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("theme.toml"), manifest).unwrap();
        fs::write(dir.join("logo.png"), b"not really a png, but non-empty").unwrap();
    }

    fn load(manifest: &str) -> Result<ValidTheme> {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("candidate");
        theme_dir(&dir, manifest);
        Theme::load(&dir)
    }

    const MINIMAL: &str = "[meta]\nname = \"Minimal\"\n";

    #[test]
    fn minimal_theme_is_valid() {
        let theme = load(MINIMAL).expect("minimal theme should validate");
        assert_eq!(theme.manifest().meta.name, "Minimal");
        assert_eq!(theme.logo().file_name().unwrap(), "logo.png");
        // shutdown inherits the unlock logo
        assert_eq!(theme.shutdown_logo(), theme.logo());
        assert!(theme.login_background().is_none());
    }

    #[test]
    fn missing_manifest_names_the_directory() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("empty")).unwrap();
        let error = Theme::load(tmp.path().join("empty"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("theme.toml"), "{error}");
    }

    #[test]
    fn unknown_key_is_an_error_not_a_silent_ignore() {
        let error = load("[meta]\nname = \"x\"\nauthr = \"typo\"\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("authr"), "{error}");
    }

    #[test]
    fn empty_name_is_refused() {
        let error = load("[meta]\nname = \"   \"\n").unwrap_err().to_string();
        assert!(error.contains("meta.name is empty"), "{error}");
    }

    #[test]
    fn control_character_in_name_is_refused() {
        let error = load("[meta]\nname = \"bad\\nname\"\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("control character"), "{error}");
    }

    #[test]
    fn bad_version_is_refused() {
        let error = load("[meta]\nname = \"x\"\nversion = \"one\"\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("meta.version"), "{error}");
        load("[meta]\nname = \"x\"\nversion = \"1.2.3-rc1\"\n").expect("prerelease is fine");
    }

    #[test]
    fn bad_colour_is_refused() {
        let error = load("[meta]\nname = \"x\"\n[colors]\naccent = \"blue\"\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("colors.accent"), "{error}");
    }

    #[test]
    fn width_out_of_range_is_refused() {
        let error = load("[meta]\nname = \"x\"\n[logo]\nwidth = 1.2\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("logo.width"), "{error}");
    }

    #[test]
    fn the_old_scale_key_still_parses_and_is_dropped_on_save() {
        // The first format sized the logo as a multiple of its own pixels.
        // A theme written that way loads with the default width, and the
        // key disappears the first time the theme is saved.
        let mut manifest: Manifest = toml::from_str(
            "[meta]\nname = \"x\"\n[logo]\nscale = 0.5\n[logo.shutdown]\nscale = 0.05\n",
        )
        .unwrap();
        manifest.normalise();
        assert_eq!(manifest.logo.width, Logo::default_width());
        assert!(
            manifest.logo.shutdown.is_empty(),
            "the override is gone too"
        );
        let text = toml::to_string(&manifest).unwrap();
        assert!(!text.contains("scale"), "{text}");
        assert!(text.contains("width = 0.42"), "{text}");
        assert!(!text.contains("[logo.shutdown]"), "{text}");
    }

    #[test]
    fn offset_without_custom_position_is_refused() {
        let error = load("[meta]\nname = \"x\"\n[logo]\noffset = [0, -40]\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("logo.offset is set"), "{error}");
        load("[meta]\nname = \"x\"\n[logo]\nposition = \"custom\"\noffset = [0, -40]\n")
            .expect("custom position accepts an offset");
    }

    #[test]
    fn absurd_offset_is_refused() {
        let error =
            load("[meta]\nname = \"x\"\n[logo]\nposition = \"custom\"\noffset = [0, -99999]\n")
                .unwrap_err()
                .to_string();
        assert!(error.contains("logo.offset"), "{error}");
    }

    #[test]
    fn missing_logo_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("t");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("theme.toml"), MINIMAL).unwrap();
        let error = Theme::load(&dir).unwrap_err().to_string();
        assert!(error.contains("logo.source"), "{error}");
    }

    #[test]
    fn empty_logo_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("t");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("theme.toml"), MINIMAL).unwrap();
        fs::write(dir.join("logo.png"), b"").unwrap();
        let error = Theme::load(&dir).unwrap_err().to_string();
        assert!(error.contains("empty"), "{error}");
    }

    #[test]
    fn symlinked_logo_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("t");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("theme.toml"), MINIMAL).unwrap();
        fs::write(tmp.path().join("real.png"), b"bytes").unwrap();
        std::os::unix::fs::symlink(tmp.path().join("real.png"), dir.join("logo.png")).unwrap();
        let error = Theme::load(&dir).unwrap_err().to_string();
        assert!(error.contains("symlink"), "{error}");
    }

    #[test]
    fn logo_outside_the_theme_is_refused() {
        let error = load("[meta]\nname = \"x\"\n[logo]\nsource = \"../../etc/passwd\"\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("leaves the theme directory"), "{error}");
    }

    #[test]
    fn absolute_logo_reference_is_refused() {
        let error = load("[meta]\nname = \"x\"\n[logo]\nsource = \"/etc/passwd\"\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("absolute"), "{error}");
    }

    #[test]
    fn an_svg_logo_is_accepted_here_and_rasterised_later() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("t");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("theme.toml"),
            "[meta]\nname = \"x\"\n[logo]\nsource = \"logo.svg\"\n",
        )
        .unwrap();
        fs::write(
            dir.join("logo.svg"),
            b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"64\"/>",
        )
        .unwrap();
        let theme = Theme::load(&dir).expect("an SVG logo validates");
        assert_eq!(theme.logo().extension().unwrap(), "svg");
    }

    #[test]
    fn overlong_message_is_refused() {
        let long = "a".repeat(MAX_MESSAGE_CHARS + 1);
        let error = load(&format!(
            "[meta]\nname = \"x\"\n[shutdown]\nmessage = \"{long}\"\n"
        ))
        .unwrap_err()
        .to_string();
        assert!(error.contains("shutdown.message"), "{error}");
    }

    #[test]
    fn image_background_requires_the_file() {
        let error = load("[meta]\nname = \"x\"\n[login]\nbackground = \"image\"\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("background.png"), "{error}");

        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("t");
        theme_dir(
            &dir,
            "[meta]\nname = \"x\"\n[login]\nbackground = \"blur\"\n",
        );
        fs::write(dir.join("background.png"), b"bytes").unwrap();
        let theme = Theme::load(&dir).expect("background present");
        assert!(theme.login_background().is_some());
    }

    #[test]
    fn custom_shutdown_logo_must_exist() {
        let error = load("[meta]\nname = \"x\"\n[shutdown]\nlogo = \"bye.png\"\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("shutdown.logo"), "{error}");
    }

    #[test]
    fn oversized_asset_is_refused() {
        // The ceiling is checked from metadata, so a sparse file proves it
        // without writing 64 MiB.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("t");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("theme.toml"), MINIMAL).unwrap();
        let file = fs::File::create(dir.join("logo.png")).unwrap();
        file.set_len(MAX_ASSET_BYTES + 1).unwrap();
        drop(file);
        let error = Theme::load(&dir).unwrap_err().to_string();
        assert!(error.contains("the limit is"), "{error}");
    }
}
