//! The SDDM half: `Main.qml`, `theme.conf`, `metadata.desktop`, and the images.

use crate::error::Result;
use crate::theme::{LoginBackground, LoginLayout, Prompt, Rgb, ValidTheme};

#[cfg(test)]
use super::template::qml_string;
use super::template::{Values, desktop_value, render};
use super::{AssetSource, GeneratedFile, SDDM_GLYPHS, copy_of, logo_file, recoloured, text};

const MAIN_TEMPLATE: &str = include_str!("templates/Main.qml.tmpl");
const CONF_TEMPLATE: &str = include_str!("templates/theme.conf.tmpl");
const METADATA_TEMPLATE: &str = include_str!("templates/metadata.desktop.tmpl");

pub(super) fn generate(theme: &ValidTheme, assets: &AssetSource) -> Result<Vec<GeneratedFile>> {
    let manifest = theme.manifest();
    let background = Rgb::parse(&manifest.colors.background).expect("validated");
    let foreground = Rgb::parse(&manifest.colors.foreground).expect("validated");
    let accent = Rgb::parse(&manifest.colors.accent).expect("validated");
    let error = Rgb::parse(&manifest.colors.error).expect("validated");

    let mut values = Values::new();
    values
        .set("THEME_ID", theme.id())
        .set("BG_HEX", background.hex())
        .set("FG_HEX", foreground.hex())
        .set("ACCENT_HEX", accent.hex())
        .set("ERROR_HEX", error.hex())
        .set(
            "LOGO_WIDTH",
            format!(
                "{:.3}",
                manifest.placement(crate::render::Screen::Login).width
            ),
        )
        .set(
            "BACKGROUND_ITEM",
            background_item(manifest.login.background, &background),
        )
        .set("PANEL_ANCHORS", panel_anchors(manifest.login.layout))
        .set("BULLETS_VISIBLE", bullets_visible(manifest.unlock.prompt))
        .set("TYPED_TEXT_VISIBLE", typed_visible(manifest.unlock.prompt))
        .set(
            "TYPED_TEXT_EXPRESSION",
            typed_expression(manifest.unlock.prompt),
        )
        .set(
            "SESSION_PICKER",
            session_picker(manifest.login.show_session_picker),
        )
        .set("CLOCK_ITEM", clock_item(manifest.login.clock));

    let main = render("Main.qml", MAIN_TEMPLATE, &values)?;

    let mut conf_values = Values::new();
    conf_values.set(
        "BACKGROUND_CONF",
        if manifest.login.background.needs_image() {
            "background.png"
        } else {
            ""
        },
    );
    let conf = render("theme.conf", CONF_TEMPLATE, &conf_values)?;

    let mut metadata_values = Values::new();
    metadata_values
        .set("THEME_ID", theme.id())
        .set(
            "THEME_NAME",
            desktop_value(&manifest.meta.name, "meta.name")?,
        )
        .set(
            "THEME_AUTHOR",
            desktop_value(&manifest.meta.author, "meta.author")?,
        )
        .set(
            "THEME_VERSION",
            desktop_value(&manifest.meta.version, "meta.version")?,
        );
    let metadata = render("metadata.desktop", METADATA_TEMPLATE, &metadata_values)?;

    let mut files = vec![
        text("Main.qml", main),
        text("theme.conf", conf),
        text("metadata.desktop", metadata),
        logo_file(theme.logo(), "logo.png")?,
    ];

    if let Some(image) = theme.login_background() {
        files.push(copy_of(image, "background.png"));
    }

    for (name, fallback) in SDDM_GLYPHS {
        let path = assets.resolve(theme, name, &assets.omarchy_sddm, fallback)?;
        // Upstream paints the failed-login assets a hard coded #f7768e.
        // omaboot paints them the theme's own error colour, which defaults to
        // that same value.
        let colour = if name.ends_with("-failed.png") {
            error
        } else {
            foreground
        };
        files.push(recoloured(&path, name, colour)?);
    }

    Ok(files)
}

fn background_item(background: LoginBackground, color: &Rgb) -> String {
    match background {
        LoginBackground::Color => String::new(),
        LoginBackground::Image => concat!(
            "  Image {\n",
            "    anchors.fill: parent\n",
            "    source: \"background.png\"\n",
            "    fillMode: Image.PreserveAspectCrop\n",
            "  }\n\n"
        )
        .to_string(),
        // A real blur needs Qt5Compat.GraphicalEffects, which is not guaranteed
        // on an Omarchy greeter. A scrim in the background colour is the
        // honest approximation until the preview milestone can verify the
        // effect renders. See docs/DECISIONS.md.
        LoginBackground::Blur => format!(
            concat!(
                "  Image {{\n",
                "    anchors.fill: parent\n",
                "    source: \"background.png\"\n",
                "    fillMode: Image.PreserveAspectCrop\n",
                "  }}\n\n",
                "  Rectangle {{\n",
                "    anchors.fill: parent\n",
                "    color: \"{hex}\"\n",
                "    opacity: 0.72\n",
                "  }}\n\n"
            ),
            hex = color.hex()
        ),
    }
}

fn panel_anchors(layout: LoginLayout) -> String {
    match layout {
        LoginLayout::Centered => "    anchors.centerIn: parent".to_string(),
        LoginLayout::Left => concat!(
            "    anchors.verticalCenter: parent.verticalCenter\n",
            "    anchors.left: parent.left\n",
            "    anchors.leftMargin: Math.round(root.width * 0.12)"
        )
        .to_string(),
        LoginLayout::Right => concat!(
            "    anchors.verticalCenter: parent.verticalCenter\n",
            "    anchors.right: parent.right\n",
            "    anchors.rightMargin: Math.round(root.width * 0.12)"
        )
        .to_string(),
    }
}

fn bullets_visible(prompt: Prompt) -> &'static str {
    match prompt {
        Prompt::Bullets => "true",
        _ => "false",
    }
}

fn typed_visible(prompt: Prompt) -> &'static str {
    match prompt {
        Prompt::Asterisks | Prompt::Counter => "true",
        _ => "false",
    }
}

fn typed_expression(prompt: Prompt) -> String {
    match prompt {
        Prompt::Asterisks => "\"*\".repeat(Math.min(password.text.length, 21))".to_string(),
        Prompt::Counter => {
            "password.text.length > 0 ? password.text.length + \" characters\" : \"\"".to_string()
        }
        _ => "\"\"".to_string(),
    }
}

fn session_picker(show: bool) -> String {
    if !show {
        return String::new();
    }
    concat!(
        "    Text {\n",
        "      anchors.horizontalCenter: parent.horizontalCenter\n",
        "      color: root.accentColor\n",
        "      font.family: \"JetBrainsMono Nerd Font\"\n",
        "      font.pixelSize: 16\n",
        "      text: (sessionModel.data(sessionModel.index(root.sessionIndex, 0), Qt.DisplayRole) || \"\").toString()\n",
        "\n",
        "      MouseArea {\n",
        "        anchors.fill: parent\n",
        "        cursorShape: Qt.PointingHandCursor\n",
        "        onClicked: root.sessionIndex = (root.sessionIndex + 1) % Math.max(sessionModel.rowCount(), 1)\n",
        "      }\n",
        "    }\n"
    )
    .to_string()
}

fn clock_item(show: bool) -> String {
    if !show {
        return String::new();
    }
    concat!(
        "  Text {\n",
        "    id: clock\n",
        "    anchors.top: parent.top\n",
        "    anchors.right: parent.right\n",
        "    anchors.margins: 40\n",
        "    color: root.foregroundColor\n",
        "    opacity: 0.7\n",
        "    font.family: \"JetBrainsMono Nerd Font\"\n",
        "    font.pixelSize: 20\n",
        "    text: Qt.formatDateTime(new Date(), \"HH:mm\")\n",
        "\n",
        "    Timer {\n",
        "      interval: 1000\n",
        "      repeat: true\n",
        "      running: true\n",
        "      onTriggered: clock.text = Qt.formatDateTime(new Date(), \"HH:mm\")\n",
        "    }\n",
        "  }\n\n"
    )
    .to_string()
}

/// Exposed so a test can show that a name QML cannot hold is refused, even
/// though the validator already rejects control characters.
#[cfg(test)]
pub(crate) fn quoted(value: &str, target: &str) -> Result<String> {
    qml_string(value, target)
}

#[cfg(test)]
mod tests {
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
    fn colours_reach_the_qml_as_literals() {
        let (_tmp, files) = generated(
            "[meta]\nname = \"T\"\n[colors]\nbackground = \"#101010\"\nforeground = \"#fefefe\"\n",
        );
        let main = body(&files, "Main.qml");
        assert!(main.contains("color: \"#101010\""), "{main}");
        assert!(
            main.contains("readonly property color foregroundColor: \"#fefefe\""),
            "{main}"
        );
    }

    #[test]
    fn a_centred_layout_has_no_side_margin() {
        let (_tmp, files) = generated("[meta]\nname = \"T\"\n");
        let main = body(&files, "Main.qml");
        assert!(main.contains("anchors.centerIn: parent"), "{main}");
        assert!(!main.contains("leftMargin: Math.round"), "{main}");
    }

    #[test]
    fn a_left_layout_anchors_left() {
        let (_tmp, files) = generated("[meta]\nname = \"T\"\n[login]\nlayout = \"left\"\n");
        assert!(body(&files, "Main.qml").contains("anchors.left: parent.left"));
    }

    #[test]
    fn the_clock_can_be_switched_off() {
        let (_tmp, with) = generated("[meta]\nname = \"T\"\n");
        assert!(body(&with, "Main.qml").contains("id: clock"));
        let (_tmp, without) = generated("[meta]\nname = \"T\"\n[login]\nclock = false\n");
        assert!(!body(&without, "Main.qml").contains("id: clock"));
    }

    #[test]
    fn the_session_picker_is_off_by_default() {
        let (_tmp, files) = generated("[meta]\nname = \"T\"\n");
        assert!(!body(&files, "Main.qml").contains("sessionModel.rowCount(), 1"));
        let (_tmp, files) =
            generated("[meta]\nname = \"T\"\n[login]\nshow_session_picker = true\n");
        assert!(body(&files, "Main.qml").contains("sessionModel.rowCount(), 1"));
    }

    #[test]
    fn the_prompt_style_is_shared_with_the_unlock_screen() {
        let (_tmp, files) = generated("[meta]\nname = \"T\"\n[unlock]\nprompt = \"asterisks\"\n");
        let main = body(&files, "Main.qml");
        assert!(
            main.contains("visible: false\n\n          Repeater")
                || main.contains("visible: false")
        );
        assert!(main.contains("\"*\".repeat("), "{main}");
    }

    #[test]
    fn a_background_image_is_installed_and_referenced() {
        let tmp = tempfile::tempdir().unwrap();
        let omarchy = fixture::omarchy_tree(&tmp.path().join("omarchy"));
        let dir = tmp.path().join("t");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("background.png"), b"bg bytes").unwrap();
        let theme = fixture::theme(
            &dir,
            "[meta]\nname = \"T\"\n[login]\nbackground = \"image\"\n",
        );
        let files = generate(&theme, &AssetSource::at(&omarchy)).unwrap();

        assert_eq!(body(&files, "background.png"), "bg bytes");
        assert!(body(&files, "Main.qml").contains("source: \"background.png\""));
        assert!(body(&files, "theme.conf").contains("background=background.png"));
    }

    #[test]
    fn metadata_declares_qt6_and_the_theme_name() {
        let (_tmp, files) =
            generated("[meta]\nname = \"Tokyo Night Boot\"\nauthor = \"mtolhuijs\"\n");
        let metadata = body(&files, "metadata.desktop");
        assert!(metadata.contains("Name=Tokyo Night Boot"), "{metadata}");
        assert!(metadata.contains("Author=mtolhuijs"), "{metadata}");
        assert!(metadata.contains("QtVersion=6"), "{metadata}");
    }

    #[test]
    fn a_quote_in_the_name_cannot_break_out_of_qml() {
        assert!(quoted("a\"b", "meta.name").unwrap().contains("\\\""));
    }
}
