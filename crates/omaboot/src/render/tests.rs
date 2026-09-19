//! Image pipeline tests.
//!
//! The compositor is checked by reading pixels back out of the canvas, which
//! is the only honest way to test a renderer without committing screenshots.

use image::Rgba;

use super::*;
use crate::generate::{AssetSource, fixture, generate};
use crate::theme::Rgb;

fn colour(hex: &str) -> Rgb {
    Rgb::parse(hex).unwrap()
}

fn generated(manifest: &str) -> (tempfile::TempDir, crate::generate::GeneratedTheme, Manifest) {
    let tmp = tempfile::tempdir().unwrap();
    let omarchy = fixture::omarchy_tree(&tmp.path().join("omarchy"));
    let theme = fixture::theme(&tmp.path().join("t"), manifest);
    let generated = generate(&theme, &AssetSource::at(&omarchy)).unwrap();
    let manifest = theme.manifest().clone();
    (tmp, generated, manifest)
}

fn pixel(canvas: &RgbaImage, x: i64, y: i64) -> Rgba<u8> {
    *canvas.get_pixel(x as u32, y as u32)
}

/// Canvases are compared by digest: a failed comparison of the raw buffers
/// would print several megabytes of pixels.
fn digest(canvas: &RgbaImage) -> String {
    crate::hash::sha256_hex(canvas.as_raw())
}

/// How many pixels differ, which is what a useful failure message says.
fn differing(a: &RgbaImage, b: &RgbaImage) -> usize {
    a.pixels().zip(b.pixels()).filter(|(x, y)| x != y).count()
}

// ------------------------------------------------------------ image basics

#[test]
fn recolouring_replaces_the_colour_and_keeps_the_alpha() {
    let mut source = RgbaImage::new(2, 1);
    source.put_pixel(0, 0, Rgba([10, 20, 30, 255]));
    source.put_pixel(1, 0, Rgba([10, 20, 30, 0]));

    let out = recolour(&source, colour("#ff0000"));
    assert_eq!(*out.get_pixel(0, 0), Rgba([255, 0, 0, 255]));
    assert_eq!(*out.get_pixel(1, 0), Rgba([255, 0, 0, 0]));
}

#[test]
fn a_png_survives_a_round_trip() {
    let source = RgbaImage::from_pixel(4, 3, Rgba([1, 2, 3, 200]));
    let bytes = encode_png(&source).unwrap();
    let back = decode(&bytes, "round trip").unwrap();
    assert_eq!(back.dimensions(), (4, 3));
    assert_eq!(*back.get_pixel(0, 0), Rgba([1, 2, 3, 200]));
}

#[test]
fn something_that_is_not_an_image_says_so_by_name() {
    let error = decode(b"this is not a png", "logo.png")
        .unwrap_err()
        .to_string();
    assert!(error.contains("logo.png"), "{error}");
}

#[test]
fn an_svg_is_rasterised_at_its_own_size_by_default() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="60"><rect width="120" height="60" fill="#00ff00"/></svg>"##;
    let image = rasterise_svg(svg, None, "logo.svg").unwrap();
    assert_eq!(image.dimensions(), (120, 60));
    assert_eq!(image.get_pixel(60, 30).0[1], 255);
}

#[test]
fn an_svg_can_be_rasterised_at_a_chosen_width_keeping_its_ratio() {
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"></svg>"#;
    let image = rasterise_svg(svg, Some(300), "logo.svg").unwrap();
    assert_eq!(image.dimensions(), (300, 150));
}

#[test]
fn an_absurdly_large_svg_is_clamped() {
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="9000" height="9000"></svg>"#;
    let image = rasterise_svg(svg, None, "logo.svg").unwrap();
    assert_eq!(image.width(), MAX_SVG_WIDTH);
}

#[test]
fn a_malformed_svg_is_an_error_that_names_the_file() {
    let error = rasterise_svg(b"<svg", None, "logo.svg")
        .unwrap_err()
        .to_string();
    assert!(error.contains("logo.svg"), "{error}");
}

#[test]
fn text_is_empty_for_an_empty_string() {
    assert!(text("", 20, colour("#ffffff")).is_none());
}

#[test]
fn text_that_renders_has_ink_in_it() {
    // A system with no fonts returns None, which is a supported outcome.
    if let Some(rendered) = text("See you", 28, colour("#ffffff")) {
        assert!(rendered.width() > 0 && rendered.height() > 0);
        assert!(
            rendered.pixels().any(|pixel| pixel.0[3] > 0),
            "the crop should have left only ink"
        );
    }
}

// -------------------------------------------------------------- compositing

#[test]
fn the_canvas_is_the_size_it_was_asked_for_and_starts_as_the_background() {
    let (_tmp, generated, manifest) =
        generated("[meta]\nname = \"T\"\n[colors]\nbackground = \"#101020\"\n");
    let canvas = composite(
        &generated,
        &manifest,
        Screen::Unlock,
        Geometry::new(1280, 720),
    )
    .unwrap();

    assert_eq!(canvas.dimensions(), (1280, 720));
    assert_eq!(pixel(&canvas, 0, 0), Rgba([0x10, 0x10, 0x20, 255]));
    assert_eq!(pixel(&canvas, 1279, 719), Rgba([0x10, 0x10, 0x20, 255]));
}

#[test]
fn the_logo_lands_where_the_layout_says_it_does() {
    let (_tmp, generated, manifest) = generated("[meta]\nname = \"T\"\n");
    let geometry = Geometry::new(1920, 1080);
    let canvas = composite(&generated, &manifest, Screen::Unlock, geometry).unwrap();

    // The fixture logo is opaque white; the background is not.
    let sizes = layout::Sizes {
        logo: (300, 100),
        entry: (400, 50),
        lock: (84, 96),
        progress_box: (400, 20),
        progress_bar: (396, 16),
    };
    let places = layout::plymouth(1920, 1080, &sizes, &manifest, Screen::Unlock);
    let centre = pixel(
        &canvas,
        places.logo.x + places.logo.width / 2,
        places.logo.y + places.logo.height / 2,
    );
    assert_eq!(centre.0[0], 255, "the logo should be drawn here");
    let outside = pixel(&canvas, 20, 20);
    assert_ne!(outside.0[0], 255, "the corner should still be background");
}

#[test]
fn the_glyphs_on_the_canvas_carry_the_theme_foreground() {
    let (_tmp, generated, manifest) =
        generated("[meta]\nname = \"T\"\n[colors]\nforeground = \"#00ff88\"\n");
    let canvas = composite(
        &generated,
        &manifest,
        Screen::Unlock,
        Geometry::new(1920, 1080),
    )
    .unwrap();

    // The fixture logo is 300x100; at the default width it is drawn 806
    // pixels wide on a 1920 screen, and the entry hangs under that.
    let sizes = layout::Sizes {
        logo: (806, 269),
        entry: (400, 50),
        lock: (84, 96),
        progress_box: (400, 20),
        progress_bar: (396, 16),
    };
    let places = layout::plymouth(1920, 1080, &sizes, &manifest, Screen::Unlock);
    // The entry is an outline in the fixture, so its top row is its ink.
    let found = (0..places.entry.width).any(|dx| {
        let p = pixel(&canvas, places.entry.x + dx, places.entry.y);
        p.0[..3] == [0x00, 0xff, 0x88]
    });
    assert!(
        found,
        "the recoloured entry should be visible on the canvas"
    );
}

#[test]
fn a_hidden_prompt_draws_no_bullets() {
    let shown = generated("[meta]\nname = \"T\"\n");
    let hidden = generated("[meta]\nname = \"T\"\n[unlock]\nprompt = \"hidden\"\n");
    let geometry = Geometry::new(1920, 1080);

    let with = composite(&shown.1, &shown.2, Screen::Unlock, geometry).unwrap();
    let without = composite(&hidden.1, &hidden.2, Screen::Unlock, geometry).unwrap();
    assert!(
        differing(&with, &without) > 0,
        "the bullets should be the only difference, and there should be one"
    );
}

#[test]
fn the_shutdown_screen_is_not_the_unlock_screen() {
    let (_tmp, generated, manifest) =
        generated("[meta]\nname = \"T\"\n[shutdown]\nmessage = \"See you\"\n");
    let geometry = Geometry::new(1920, 1080);
    let unlock = composite(&generated, &manifest, Screen::Unlock, geometry).unwrap();
    let shutdown = composite(&generated, &manifest, Screen::Shutdown, geometry).unwrap();
    assert_ne!(
        digest(&unlock),
        digest(&shutdown),
        "the two Plymouth screens must differ, that is the point of the feature"
    );
}

#[test]
fn a_separate_shutdown_logo_is_the_one_drawn_on_the_shutdown_screen() {
    let tmp = tempfile::tempdir().unwrap();
    let omarchy = fixture::omarchy_tree(&tmp.path().join("omarchy"));
    let dir = tmp.path().join("t");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("bye.png"), fixture::png(600, 40, [255, 0, 0, 255])).unwrap();
    let theme = fixture::theme(
        &dir,
        "[meta]\nname = \"T\"\n[shutdown]\nlogo = \"bye.png\"\n",
    );
    let generated = generate(&theme, &AssetSource::at(&omarchy)).unwrap();

    let canvas = composite(
        &generated,
        theme.manifest(),
        Screen::Shutdown,
        Geometry::new(1920, 1080),
    )
    .unwrap();
    // The shutdown logo is 600 wide, so it reaches further out than the 300
    // wide unlock logo does.
    assert_eq!(pixel(&canvas, 960 - 280, 540).0[0], 255);
}

#[test]
fn the_login_screen_can_carry_a_background_image() {
    let tmp = tempfile::tempdir().unwrap();
    let omarchy = fixture::omarchy_tree(&tmp.path().join("omarchy"));
    let dir = tmp.path().join("t");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("background.png"),
        fixture::png(16, 9, [0, 0, 255, 255]),
    )
    .unwrap();
    let theme = fixture::theme(
        &dir,
        "[meta]\nname = \"T\"\n[colors]\nbackground = \"#000000\"\n[login]\nbackground = \"image\"\n",
    );
    let generated = generate(&theme, &AssetSource::at(&omarchy)).unwrap();

    let canvas = composite(
        &generated,
        theme.manifest(),
        Screen::Login,
        Geometry::new(1920, 1080),
    )
    .unwrap();
    // The stretched background reaches the corner; the flat colour would not
    // have been blue.
    assert!(pixel(&canvas, 8, 8).0[2] > 200 || pixel(&canvas, 9, 8).0[2] > 200);
}

#[test]
fn a_blurred_background_is_dimmed_towards_the_background_colour() {
    let tmp = tempfile::tempdir().unwrap();
    let omarchy = fixture::omarchy_tree(&tmp.path().join("omarchy"));
    let dir = tmp.path().join("t");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("background.png"),
        fixture::png(16, 9, [255, 255, 255, 255]),
    )
    .unwrap();
    let theme = fixture::theme(
        &dir,
        "[meta]\nname = \"T\"\n[colors]\nbackground = \"#000000\"\n[login]\nbackground = \"blur\"\n",
    );
    let generated = generate(&theme, &AssetSource::at(&omarchy)).unwrap();
    let canvas = composite(
        &generated,
        theme.manifest(),
        Screen::Login,
        Geometry::new(400, 300),
    )
    .unwrap();

    let corner = pixel(&canvas, 5, 5);
    assert!(
        corner.0[0] < 200,
        "the scrim should have darkened the image, got {corner:?}"
    );
}

#[test]
fn a_left_aligned_greeter_draws_further_left_than_a_centred_one() {
    let centred = generated("[meta]\nname = \"T\"\n");
    let left = generated("[meta]\nname = \"T\"\n[login]\nlayout = \"left\"\n");
    let geometry = Geometry::new(1920, 1080);

    let centred = composite(&centred.1, &centred.2, Screen::Login, geometry).unwrap();
    let left = composite(&left.1, &left.2, Screen::Login, geometry).unwrap();

    let leftmost = |canvas: &RgbaImage| -> u32 {
        canvas
            .enumerate_pixels()
            .filter(|(_, _, pixel)| pixel.0[3] > 0 && pixel.0[..3] != [0x1a, 0x1b, 0x26])
            .map(|(x, _, _)| x)
            .min()
            .unwrap_or(u32::MAX)
    };
    assert!(
        leftmost(&left) < leftmost(&centred),
        "left {} should be further left than centred {}",
        leftmost(&left),
        leftmost(&centred)
    );
}

#[test]
fn compositing_is_deterministic() {
    let (_tmp, generated, manifest) = generated("[meta]\nname = \"T\"\n");
    let geometry = Geometry::new(800, 600);
    let first = composite(&generated, &manifest, Screen::Unlock, geometry).unwrap();
    let second = composite(&generated, &manifest, Screen::Unlock, geometry).unwrap();
    assert_eq!(digest(&first), digest(&second));
}

#[test]
fn a_theme_that_lost_an_asset_says_which_one() {
    let (_tmp, mut generated, manifest) = generated("[meta]\nname = \"T\"\n");
    generated.plymouth.retain(|file| file.name != "entry.png");
    let error = composite(
        &generated,
        &manifest,
        Screen::Unlock,
        Geometry::new(800, 600),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("entry.png"), "{error}");
}

#[test]
fn a_tiny_screen_still_renders() {
    let (_tmp, generated, manifest) = generated("[meta]\nname = \"T\"\n");
    let canvas = composite(&generated, &manifest, Screen::Unlock, Geometry::new(64, 48)).unwrap();
    assert_eq!(canvas.dimensions(), (64, 48));
}
