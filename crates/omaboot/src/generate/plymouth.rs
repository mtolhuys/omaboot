//! The Plymouth half: `omaboot.plymouth`, `omaboot.script`, and the images.

use crate::error::Result;
use crate::paths::PLYMOUTH_THEME_DIR;
use crate::render::Screen;
use crate::theme::{Position, Progress, Prompt, Rgb, ShutdownLogo, ValidTheme};

use super::template::{Values, plymouth_string, render};
use super::{AssetSource, GeneratedFile, PLYMOUTH_GLYPHS, logo_file, recoloured, text};
use crate::render::layout;

const THEME_TEMPLATE: &str = include_str!("templates/omaboot.plymouth.tmpl");
const SCRIPT_TEMPLATE: &str = include_str!("templates/omaboot.script.tmpl");

/// The glyphs that follow the theme's foreground colour, which is the same set
/// `omarchy-plymouth-set` recolours.
const RECOLOURED_GLYPHS: [&str; 4] = ["bullet.png", "entry.png", "lock.png", "progress_bar.png"];

/// The image directory the installed theme reads from. Note that this is the
/// destination on the running system, not a staged path: Plymouth resolves it
/// at boot, when the prefix used for testing no longer exists.
const IMAGE_DIR: &str = PLYMOUTH_THEME_DIR;

/// The `.plymouth` file for a live preview: the same theme, read from the
/// preview directory under `/run` instead of the installed one, so plymouthd
/// can be pointed at it with `plymouth.splash=omaboot-preview` while the
/// installed theme stays exactly as it is.
pub fn preview_theme_file(theme: &ValidTheme) -> Result<String> {
    theme_file(theme, crate::paths::PREVIEW_THEME_DIR)
}

fn theme_file(theme: &ValidTheme, image_dir: &str) -> Result<String> {
    let manifest = theme.manifest();
    let background = Rgb::parse(&manifest.colors.background).expect("validated");
    let mut theme_values = Values::new();
    theme_values
        .set("THEME_ID", theme.id())
        .set(
            "THEME_NAME",
            super::template::desktop_value(&manifest.meta.name, "meta.name")?,
        )
        .set("IMAGE_DIR", image_dir)
        .set(
            "BG_HEX_BARE",
            background.hex().trim_start_matches('#').to_string(),
        )
        .set("MONOSPACE_FONT", layout::PLYMOUTH_FONT)
        .set("FONT", layout::PLYMOUTH_FONT);
    render("omaboot.plymouth", THEME_TEMPLATE, &theme_values)
}

pub(super) fn generate(theme: &ValidTheme, assets: &AssetSource) -> Result<Vec<GeneratedFile>> {
    let manifest = theme.manifest();
    let background = Rgb::parse(&manifest.colors.background).expect("validated");
    let foreground = Rgb::parse(&manifest.colors.foreground).expect("validated");

    let (bg_r, bg_g, bg_b) = background.plymouth_triplet();
    let (fg_r, fg_g, fg_b) = foreground.plymouth_triplet();

    let shutdown_logo_file = match &manifest.shutdown.logo {
        ShutdownLogo::Inherit => "logo.png".to_string(),
        ShutdownLogo::Source(_) => "logo-shutdown.png".to_string(),
    };

    let unlock = manifest.placement(Screen::Unlock);
    let shutdown = manifest.placement(Screen::Shutdown);
    let mut values = Values::new();
    values
        .set("THEME_ID", theme.id())
        .set("BG_R", bg_r.clone())
        .set("BG_G", bg_g.clone())
        .set("BG_B", bg_b.clone())
        .set("FG_R", fg_r)
        .set("FG_G", fg_g)
        .set("FG_B", fg_b)
        .set("UNLOCK_LOGO_WIDTH", format!("{:.3}", unlock.width))
        .set("UNLOCK_LOGO_POSITION", position(unlock.position))
        .set("UNLOCK_LOGO_OFFSET_X", unlock.offset[0].to_string())
        .set("UNLOCK_LOGO_OFFSET_Y", unlock.offset[1].to_string())
        .set("SHUTDOWN_LOGO_WIDTH", format!("{:.3}", shutdown.width))
        .set("SHUTDOWN_LOGO_POSITION", position(shutdown.position))
        .set("SHUTDOWN_LOGO_OFFSET_X", shutdown.offset[0].to_string())
        .set("SHUTDOWN_LOGO_OFFSET_Y", shutdown.offset[1].to_string())
        .set("PROMPT_STYLE", prompt(manifest.unlock.prompt))
        .set("UNLOCK_PROGRESS", progress(manifest.unlock.progress))
        .set("SHUTDOWN_PROGRESS", progress(manifest.shutdown.progress))
        .set(
            "UNLOCK_MESSAGE",
            plymouth_string(&manifest.unlock.message, "unlock.message")?,
        )
        .set(
            "SHUTDOWN_MESSAGE",
            plymouth_string(&manifest.shutdown.message, "shutdown.message")?,
        )
        .set("SHUTDOWN_LOGO_FILE", shutdown_logo_file.clone())
        // The numbers below are the layout, and they come from one place, so
        // the script and the composited preview cannot drift apart.
        .set("LOGO_ENTRY_GAP", layout::LOGO_ENTRY_GAP.to_string())
        .set("LOCK_ENTRY_GAP", layout::LOCK_ENTRY_GAP.to_string())
        .set(
            "LOCK_HEIGHT_RATIO",
            format!("{:.3}", layout::LOCK_HEIGHT_RATIO),
        )
        .set("BULLET_SIZE", layout::BULLET_SIZE.to_string())
        .set("BULLET_PITCH", layout::BULLET_PITCH.to_string())
        .set("ENTRY_INSET", layout::ENTRY_INSET.to_string())
        .set("MESSAGE_GAP", layout::MESSAGE_GAP.to_string())
        .set("MAX_BULLETS", layout::MAX_BULLETS.to_string())
        .set(
            "TOP_POSITION_DIVISOR",
            layout::TOP_POSITION_DIVISOR.to_string(),
        );

    let script = render("omaboot.script", SCRIPT_TEMPLATE, &values)?;

    let mut files = vec![
        text("omaboot.plymouth", theme_file(theme, IMAGE_DIR)?),
        text("omaboot.script", script),
        logo_file(theme.logo(), "logo.png")?,
    ];

    if shutdown_logo_file != "logo.png" {
        files.push(logo_file(theme.shutdown_logo(), &shutdown_logo_file)?);
    }

    for name in PLYMOUTH_GLYPHS {
        let path = assets.resolve(theme, name, &assets.omarchy_plymouth, None)?;
        // Upstream recolours exactly these four with ImageMagick and leaves
        // progress_box.png alone, because the box is the unfilled outline.
        if RECOLOURED_GLYPHS.contains(&name) {
            files.push(recoloured(&path, name, foreground)?);
        } else {
            files.push(super::copy_of(&path, name));
        }
    }

    Ok(files)
}

fn position(position: Position) -> &'static str {
    match position {
        Position::Center | Position::Custom => "center",
        Position::Top => "top",
    }
}

fn prompt(prompt: Prompt) -> &'static str {
    match prompt {
        Prompt::Bullets => "bullets",
        Prompt::Asterisks => "asterisks",
        Prompt::Hidden => "hidden",
        Prompt::Counter => "counter",
    }
}

fn progress(progress: Progress) -> &'static str {
    match progress {
        Progress::Bar => "bar",
        Progress::Spinner => "spinner",
        Progress::None => "none",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::fixture;
    use super::*;

    fn generated(manifest: &str) -> (tempfile::TempDir, Vec<GeneratedFile>) {
        let tmp = tempfile::tempdir().unwrap();
        let omarchy = fixture::omarchy_tree(&tmp.path().join("omarchy"));
        let assets = AssetSource::at(&omarchy);
        let theme = fixture::theme(&tmp.path().join("t"), manifest);
        let files = generate(&theme, &assets).unwrap();
        (tmp, files)
    }

    fn body(files: &[GeneratedFile], name: &str) -> String {
        let file = files.iter().find(|f| f.name == name).expect(name);
        String::from_utf8(file.bytes().unwrap()).unwrap()
    }

    #[test]
    fn colours_are_injected_as_plymouth_floats() {
        let (_tmp, files) = generated("[meta]\nname = \"T\"\n[colors]\nbackground = \"#1a1b26\"\n");
        let script = body(&files, "omaboot.script");
        assert!(
            script.contains("global.background_red = 0.102;"),
            "{script}"
        );
        assert!(
            script.contains("Window.SetBackgroundTopColor(global.background_red"),
            "{script}"
        );
        // The value is data in a variable, not a line edited by sed.
        assert!(!script.contains("__"), "{script}");
    }

    #[test]
    fn the_console_colour_follows_the_theme() {
        let (_tmp, files) = generated("[meta]\nname = \"T\"\n[colors]\nbackground = \"#223344\"\n");
        assert!(body(&files, "omaboot.plymouth").contains("ConsoleLogBackgroundColor=0x223344"));
    }

    #[test]
    fn the_script_reads_its_images_from_the_omaboot_directory_only() {
        let (_tmp, files) = generated("[meta]\nname = \"T\"\n");
        let theme_file = body(&files, "omaboot.plymouth");
        assert!(theme_file.contains("/usr/share/plymouth/themes/omaboot"));
        assert!(!theme_file.contains("themes/omarchy"));
    }

    #[test]
    fn prompt_and_progress_reach_the_script() {
        let (_tmp, files) = generated(
            "[meta]\nname = \"T\"\n[unlock]\nprompt = \"counter\"\nprogress = \"none\"\n[shutdown]\nprogress = \"spinner\"\n",
        );
        let script = body(&files, "omaboot.script");
        assert!(
            script.contains("global.prompt_style = \"counter\";"),
            "{script}"
        );
        assert!(
            script.contains("global.unlock_progress = \"none\";"),
            "{script}"
        );
        assert!(
            script.contains("global.shutdown_progress = \"spinner\";"),
            "{script}"
        );
    }

    #[test]
    fn a_quote_in_a_message_is_escaped_not_dropped() {
        let (_tmp, files) =
            generated("[meta]\nname = \"T\"\n[shutdown]\nmessage = \"See you, \\\"friend\\\"\"\n");
        let script = body(&files, "omaboot.script");
        assert!(
            script.contains("global.shutdown_message = \"See you, \\\"friend\\\"\";"),
            "{script}"
        );
    }

    #[test]
    fn a_separate_shutdown_logo_is_installed_under_its_own_name() {
        let tmp = tempfile::tempdir().unwrap();
        let omarchy = fixture::omarchy_tree(&tmp.path().join("omarchy"));
        let dir = tmp.path().join("t");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("bye.png"), b"bye bytes").unwrap();
        let theme = fixture::theme(
            &dir,
            "[meta]\nname = \"T\"\n[shutdown]\nlogo = \"bye.png\"\n",
        );
        let files = generate(&theme, &AssetSource::at(&omarchy)).unwrap();

        assert_eq!(body(&files, "logo-shutdown.png"), "bye bytes");
        assert!(body(&files, "omaboot.script").contains("Image(\"logo-shutdown.png\")"));
    }

    #[test]
    fn an_inherited_shutdown_logo_installs_one_file() {
        let (_tmp, files) = generated("[meta]\nname = \"T\"\n");
        assert!(files.iter().all(|f| f.name != "logo-shutdown.png"));
    }
}
