//! Images: decoding, recolouring, rasterising, and drawing a whole screen.
//!
//! Two things live here. The first is the small set of image operations the
//! generator needs, including the recolour that `omarchy-plymouth-set` does
//! with ImageMagick. The second is the compositor, which draws what a screen
//! will look like.
//!
//! The compositor draws from the generated theme, meaning the exact bytes that
//! would be installed, so a preview cannot show something other than what an
//! apply would produce.

pub mod layout;

use std::io::Cursor;

use image::imageops::FilterType;
use image::{ImageFormat, ImageReader, Rgba, RgbaImage};

use crate::error::{Error, Result};
use crate::generate::GeneratedTheme;
use crate::theme::{LoginBackground, Manifest, Position, Progress, Prompt, Rgb};

pub use layout::Screen;

/// A screen size to draw at. The preview states which one it used, because
/// nobody should have to wonder what they are looking at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub width: u32,
    pub height: u32,
    /// How many characters of a password to draw, so the preview can show the
    /// state that actually matters.
    pub bullets: u32,
}

impl Default for Geometry {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            bullets: 5,
        }
    }
}

impl Geometry {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            ..Self::default()
        }
    }
}

/// Decode PNG (or anything else the build supports) into RGBA.
pub fn decode(bytes: &[u8], what: &str) -> Result<RgbaImage> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|source| Error::Unrepresentable {
            value: what.to_string(),
            target: "an image".to_string(),
            what: format!("a format that could not be recognised ({source})"),
        })?;
    let image = reader.decode().map_err(|source| Error::Unrepresentable {
        value: what.to_string(),
        target: "an image".to_string(),
        what: format!("image data that could not be decoded ({source})"),
    })?;
    Ok(image.to_rgba8())
}

pub fn encode_png(image: &RgbaImage) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .map_err(|source| Error::Unrepresentable {
            value: "a generated image".to_string(),
            target: "a PNG".to_string(),
            what: format!("an encoding failure ({source})"),
        })?;
    Ok(out)
}

/// Replace every pixel's colour while keeping its alpha.
///
/// This is what `magick -channel RGB +level-colors "#c","#c"` does: with the
/// same colour as both the black and the white point, every input level maps
/// onto that one colour, and the alpha channel is untouched. Doing it here
/// rather than shelling out to ImageMagick means one less runtime dependency
/// and a result that can be unit tested.
pub fn recolour(image: &RgbaImage, colour: Rgb) -> RgbaImage {
    let mut out = image.clone();
    for pixel in out.pixels_mut() {
        let alpha = pixel.0[3];
        *pixel = Rgba([colour.r, colour.g, colour.b, alpha]);
    }
    out
}

pub fn scale(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    if width == 0 || height == 0 {
        return RgbaImage::new(1, 1);
    }
    image::imageops::resize(image, width, height, FilterType::Lanczos3)
}

/// Rasterise an SVG, keeping its aspect ratio.
///
/// With no target width it is drawn at its own size, clamped to something a
/// boot screen can use, so a logo that declares 8000 pixels does not become an
/// 8000 pixel PNG in the initramfs.
pub fn rasterise_svg(bytes: &[u8], target_width: Option<u32>, what: &str) -> Result<RgbaImage> {
    use resvg::{tiny_skia, usvg};

    let unrepresentable = |detail: String| Error::Unrepresentable {
        value: what.to_string(),
        target: "a raster image".to_string(),
        what: detail,
    };

    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_data(bytes, &options)
        .map_err(|source| unrepresentable(format!("SVG that could not be parsed ({source})")))?;

    let size = tree.size();
    if size.width() <= 0.0 || size.height() <= 0.0 {
        return Err(unrepresentable("an SVG with no size".to_string()));
    }
    let target_width = target_width
        .unwrap_or_else(|| (size.width().round() as u32).clamp(MIN_SVG_WIDTH, MAX_SVG_WIDTH))
        .max(1);
    let factor = f32::from(u16::try_from(target_width).unwrap_or(u16::MAX)) / size.width();
    let target_height = ((size.height() * factor).round() as u32).max(1);

    let mut pixmap = tiny_skia::Pixmap::new(target_width, target_height)
        .ok_or_else(|| unrepresentable("a target size that is too large".to_string()))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(factor, factor),
        &mut pixmap.as_mut(),
    );

    // tiny-skia stores premultiplied alpha; image wants it straight.
    let mut out = RgbaImage::new(target_width, target_height);
    for (pixel, source) in out.pixels_mut().zip(pixmap.pixels()) {
        let colour = source.demultiply();
        *pixel = Rgba([colour.red(), colour.green(), colour.blue(), colour.alpha()]);
    }
    Ok(out)
}

/// The range an SVG logo is rasterised into when it does not say otherwise.
pub const MIN_SVG_WIDTH: u32 = 16;
pub const MAX_SVG_WIDTH: u32 = 2048;

/// Render a line of text to an image, through a one-line SVG.
///
/// Returns `None` when no font could be used, which happens on a system with
/// no fonts installed. A preview without its caption is better than a preview
/// that refuses to draw.
pub fn text(content: &str, pixel_size: u32, colour: Rgb) -> Option<RgbaImage> {
    text_in(content, pixel_size, colour, layout::PLYMOUTH_FONT_FAMILY)
}

/// The same, in a named family, so the greeter's monospace clock and
/// Plymouth's proportional messages each look like their own render.
pub fn text_in(content: &str, pixel_size: u32, colour: Rgb, family: &str) -> Option<RgbaImage> {
    if content.is_empty() {
        return None;
    }
    let escaped = content
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let width = (content.chars().count() as u32 * pixel_size).max(pixel_size) + pixel_size;
    let height = pixel_size * 2;
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}">
<text x="0" y="{baseline}" font-family="{family}, DejaVu Sans, sans-serif" font-size="{pixel_size}" fill="{fill}">{escaped}</text>
</svg>"#,
        baseline = pixel_size + pixel_size / 4,
        fill = colour.hex(),
        family = family
    );
    let rendered = rasterise_svg(svg.as_bytes(), Some(width), "a line of text").ok()?;
    let cropped = crop_to_content(&rendered)?;
    Some(cropped)
}

/// Trim fully transparent rows and columns, so text can be positioned by its
/// ink rather than by the box it happened to be drawn in.
fn crop_to_content(image: &RgbaImage) -> Option<RgbaImage> {
    let (mut min_x, mut min_y) = (u32::MAX, u32::MAX);
    let (mut max_x, mut max_y) = (0, 0);
    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel.0[3] > 0 {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if min_x == u32::MAX {
        return None;
    }
    Some(
        image::imageops::crop_imm(image, min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
            .to_image(),
    )
}

/// Draw `source` onto `canvas` at a position, respecting alpha.
fn draw(canvas: &mut RgbaImage, source: &RgbaImage, x: i64, y: i64) {
    image::imageops::overlay(canvas, source, x, y);
}

fn fill(width: u32, height: u32, colour: Rgb) -> RgbaImage {
    RgbaImage::from_pixel(
        width.max(1),
        height.max(1),
        Rgba([colour.r, colour.g, colour.b, 255]),
    )
}

/// Everything the compositor needs, taken from the files that would be
/// installed.
struct Assets {
    logo: RgbaImage,
    entry: RgbaImage,
    lock: RgbaImage,
    bullet: RgbaImage,
    progress_box: RgbaImage,
    progress_bar: RgbaImage,
    background: Option<RgbaImage>,
}

impl Assets {
    fn load(generated: &GeneratedTheme, screen: Screen) -> Result<Self> {
        let files = if screen.is_plymouth() {
            &generated.plymouth
        } else {
            &generated.sddm
        };
        let read = |name: &str| -> Result<RgbaImage> {
            let file = files.iter().find(|file| file.name == name).ok_or_else(|| {
                Error::step(
                    "render",
                    format!("the generated theme has no {name}"),
                    "regenerate the theme; this is a bug if it happens on a valid theme",
                )
            })?;
            decode(&file.bytes()?, name)
        };
        let optional = |name: &str| -> Result<Option<RgbaImage>> {
            match files.iter().find(|file| file.name == name) {
                None => Ok(None),
                Some(file) => Ok(Some(decode(&file.bytes()?, name)?)),
            }
        };

        let logo_name = match screen {
            Screen::Shutdown if files.iter().any(|f| f.name == "logo-shutdown.png") => {
                "logo-shutdown.png"
            }
            _ => "logo.png",
        };

        // The greeter has no progress assets of its own; it never shows one.
        let (progress_box, progress_bar) = if screen.is_plymouth() {
            (read("progress_box.png")?, read("progress_bar.png")?)
        } else {
            (RgbaImage::new(1, 1), RgbaImage::new(1, 1))
        };

        Ok(Self {
            logo: read(logo_name)?,
            entry: read("entry.png")?,
            lock: read("lock.png")?,
            bullet: read("bullet.png")?,
            progress_box,
            progress_bar,
            background: optional("background.png")?,
        })
    }

    fn sizes(&self, manifest: &Manifest, screen: Screen, canvas_width: i64) -> layout::Sizes {
        // The logo's share of the screen width, keeping its aspect ratio,
        // as the script and the greeter both compute it.
        let logo_w = ((canvas_width as f64) * manifest.placement(screen).width).round() as i64;
        let logo_h = if self.logo.width() == 0 {
            0
        } else {
            ((logo_w as f64) * f64::from(self.logo.height()) / f64::from(self.logo.width())).round()
                as i64
        };
        layout::Sizes {
            logo: (logo_w.max(1), logo_h.max(1)),
            entry: (self.entry.width() as i64, self.entry.height() as i64),
            lock: (self.lock.width() as i64, self.lock.height() as i64),
            progress_box: (
                self.progress_box.width() as i64,
                self.progress_box.height() as i64,
            ),
            progress_bar: (
                self.progress_bar.width() as i64,
                self.progress_bar.height() as i64,
            ),
        }
    }
}

/// Draw one screen of a theme.
pub fn composite(
    generated: &GeneratedTheme,
    manifest: &Manifest,
    screen: Screen,
    geometry: Geometry,
) -> Result<RgbaImage> {
    let background = Rgb::parse(&manifest.colors.background).expect("validated");
    let foreground = Rgb::parse(&manifest.colors.foreground).expect("validated");
    let assets = Assets::load(generated, screen)?;
    let sizes = assets.sizes(manifest, screen, i64::from(geometry.width));

    let mut canvas = fill(geometry.width, geometry.height, background);
    let (width, height) = (i64::from(geometry.width), i64::from(geometry.height));

    if screen == Screen::Login
        && manifest.login.background.needs_image()
        && let Some(image) = &assets.background
    {
        let cover = cover_scale(image, geometry.width, geometry.height);
        draw(&mut canvas, &cover, 0, 0);
        if manifest.login.background == LoginBackground::Blur {
            // The greeter approximates a blur with a scrim; the preview shows
            // the same approximation rather than a prettier lie.
            let mut scrim = fill(geometry.width, geometry.height, background);
            for pixel in scrim.pixels_mut() {
                pixel.0[3] = 184;
            }
            draw(&mut canvas, &scrim, 0, 0);
        }
    }

    let logo = scale(
        &assets.logo,
        sizes.logo.0.max(1) as u32,
        sizes.logo.1.max(1) as u32,
    );

    match screen {
        Screen::Login => {
            let places = layout::login(width, height, &sizes, manifest);
            // The greeter caps the logo width, so the preview does as well.
            let logo = scale(
                &assets.logo,
                places.logo.width.max(1) as u32,
                places.logo.height.max(1) as u32,
            );
            draw(&mut canvas, &logo, places.logo.x, places.logo.y);
            let lock = scale(
                &assets.lock,
                places.lock.width as u32,
                places.lock.height as u32,
            );
            draw(&mut canvas, &lock, places.lock.x, places.lock.y);
            draw(&mut canvas, &assets.entry, places.entry.x, places.entry.y);
            draw_prompt(
                &mut canvas,
                &assets.bullet,
                places.bullet_origin,
                geometry.bullets,
                manifest.unlock.prompt,
                foreground,
            );
            if manifest.login.clock
                && let Some(clock) = text_in(
                    "09:41",
                    layout::LOGIN_CLOCK_PX,
                    foreground,
                    layout::LOGIN_FONT_FAMILY,
                )
            {
                draw(
                    &mut canvas,
                    &clock,
                    places.clock.0 - i64::from(clock.width()),
                    places.clock.1,
                );
            }
        }
        Screen::Unlock | Screen::Shutdown => {
            let places = layout::plymouth(width, height, &sizes, manifest, screen);
            draw(&mut canvas, &logo, places.logo.x, places.logo.y);

            let progress = if screen == Screen::Shutdown {
                manifest.shutdown.progress
            } else {
                manifest.unlock.progress
            };

            if screen == Screen::Unlock {
                let lock = scale(
                    &assets.lock,
                    places.lock.width as u32,
                    places.lock.height as u32,
                );
                draw(&mut canvas, &lock, places.lock.x, places.lock.y);
                draw(&mut canvas, &assets.entry, places.entry.x, places.entry.y);
                draw_prompt(
                    &mut canvas,
                    &assets.bullet,
                    places.bullet_origin,
                    geometry.bullets,
                    manifest.unlock.prompt,
                    foreground,
                );
            } else if progress != Progress::None {
                draw(
                    &mut canvas,
                    &assets.progress_box,
                    places.progress_box.x,
                    places.progress_box.y,
                );
                let filled = match progress {
                    // A bar mid-rebuild says more than an empty one.
                    Progress::Bar => (places.progress_bar.width as f64 * 0.45) as u32,
                    _ => (places.progress_bar.width as f64 * 0.25) as u32,
                };
                let bar = scale(
                    &assets.progress_bar,
                    filled.max(1),
                    places.progress_bar.height.max(1) as u32,
                );
                draw(
                    &mut canvas,
                    &bar,
                    places.progress_bar.x,
                    places.progress_bar.y,
                );
            }

            let message = if screen == Screen::Shutdown {
                &manifest.shutdown.message
            } else {
                &manifest.unlock.message
            };
            if let Some(rendered) = text(message, layout::PLYMOUTH_FONT_PX, foreground) {
                draw(
                    &mut canvas,
                    &rendered,
                    places.message.0 - i64::from(rendered.width()) / 2,
                    places.message.1,
                );
            }
        }
    }

    let _ = Position::Center;
    Ok(canvas)
}

fn draw_prompt(
    canvas: &mut RgbaImage,
    bullet: &RgbaImage,
    origin: (i64, i64),
    count: u32,
    prompt: Prompt,
    foreground: Rgb,
) {
    let count = i64::from(count).min(layout::MAX_BULLETS);
    match prompt {
        Prompt::Hidden => {}
        Prompt::Bullets => {
            let bullet = scale(
                bullet,
                layout::BULLET_SIZE as u32,
                layout::BULLET_SIZE as u32,
            );
            for index in 0..count {
                let place = layout::bullet(origin, index);
                draw(canvas, &bullet, place.x, place.y);
            }
        }
        Prompt::Asterisks => {
            if let Some(stars) = text(
                &"*".repeat(count.max(0) as usize),
                layout::PLYMOUTH_FONT_PX,
                foreground,
            ) {
                draw(canvas, &stars, origin.0, origin.1 - 2);
            }
        }
        Prompt::Counter => {
            if let Some(counter) = text(
                &format!("{count} characters"),
                layout::PLYMOUTH_FONT_PX,
                foreground,
            ) {
                draw(canvas, &counter, origin.0, origin.1 - 4);
            }
        }
    }
}

/// Scale an image to cover the canvas, cropping the overflow, which is what
/// `Image.PreserveAspectCrop` does in the greeter.
fn cover_scale(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    if image.width() == 0 || image.height() == 0 {
        return RgbaImage::new(width.max(1), height.max(1));
    }
    let factor = (f64::from(width) / f64::from(image.width()))
        .max(f64::from(height) / f64::from(image.height()));
    let scaled = scale(
        image,
        ((f64::from(image.width()) * factor).ceil() as u32).max(width),
        ((f64::from(image.height()) * factor).ceil() as u32).max(height),
    );
    image::imageops::crop_imm(&scaled, 0, 0, width.max(1), height.max(1)).to_image()
}

#[cfg(test)]
mod tests;
