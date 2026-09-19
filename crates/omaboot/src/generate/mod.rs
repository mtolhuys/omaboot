//! Turning a validated theme into the files that get installed.
//!
//! Generation is pure: it reads the theme directory and the packaged Omarchy
//! assets, and returns bytes. It writes nothing. That is what lets the whole
//! step be unit tested without a prefix, a root, or a system call.

mod plymouth;
mod sddm;

pub use plymouth::preview_theme_file;
pub mod template;

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::hash::{sha256_hex, sha256_manifest};
use crate::theme::ValidTheme;

/// Glyph assets the Plymouth theme needs beyond the logo.
pub const PLYMOUTH_GLYPHS: [&str; 5] = [
    "bullet.png",
    "entry.png",
    "lock.png",
    "progress_bar.png",
    "progress_box.png",
];

/// Glyph assets the SDDM theme needs beyond the logo, with the fallback used
/// when the theme and the packaged tree both lack the failed-state variant.
pub const SDDM_GLYPHS: [(&str, Option<&str>); 5] = [
    ("bullet.png", None),
    ("entry.png", None),
    ("lock.png", None),
    ("entry-failed.png", Some("entry.png")),
    ("lock-failed.png", Some("lock.png")),
];

/// One file to install, either generated text or a byte-for-byte copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Text(String),
    /// Bytes omaboot produced from a source file: a recoloured glyph, or a
    /// rasterised SVG. The origin is kept so the plan can say where it came
    /// from.
    Derived {
        bytes: Vec<u8>,
        origin: String,
    },
    CopyOf(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    /// The name inside the installed theme directory.
    pub name: String,
    pub content: Content,
}

impl GeneratedFile {
    pub fn bytes(&self) -> Result<Vec<u8>> {
        match &self.content {
            Content::Text(text) => Ok(text.as_bytes().to_vec()),
            Content::Derived { bytes, .. } => Ok(bytes.clone()),
            Content::CopyOf(path) => {
                fs::read(path).map_err(|source| Error::read(path.clone(), source))
            }
        }
    }

    pub fn origin(&self) -> String {
        match &self.content {
            Content::Text(_) => "generated".to_string(),
            Content::Derived { origin, .. } => origin.clone(),
            Content::CopyOf(path) => format!("copy of {}", path.display()),
        }
    }
}

/// Everything one apply installs, split by destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedTheme {
    pub plymouth: Vec<GeneratedFile>,
    pub sddm: Vec<GeneratedFile>,
}

impl GeneratedTheme {
    /// A digest over every file that will be installed, in a stable order.
    /// This is the value recorded in the rollback point and re-checked by the
    /// verify step.
    pub fn hash(&self) -> Result<String> {
        let mut loaded: Vec<(String, Vec<u8>)> = Vec::new();
        for (prefix, files) in [("plymouth", &self.plymouth), ("sddm", &self.sddm)] {
            for file in files {
                loaded.push((format!("{prefix}/{}", file.name), file.bytes()?));
            }
        }
        loaded.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(sha256_manifest(
            loaded
                .iter()
                .map(|(name, bytes)| (name.as_str(), bytes.as_slice())),
        ))
    }

    /// Per-file digests, for the installed-state record.
    pub fn digests(&self) -> Result<Vec<(String, String, u64)>> {
        let mut out = Vec::new();
        for (prefix, files) in [("plymouth", &self.plymouth), ("sddm", &self.sddm)] {
            for file in files {
                let bytes = file.bytes()?;
                out.push((
                    format!("{prefix}/{}", file.name),
                    sha256_hex(&bytes),
                    bytes.len() as u64,
                ));
            }
        }
        Ok(out)
    }
}

/// Where glyph assets come from when a theme does not ship its own.
///
/// The packaged Omarchy tree is read, never written. Reading it is how omaboot
/// stays visually consistent with the stock screens without copying assets
/// into the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetSource {
    omarchy_plymouth: PathBuf,
    omarchy_sddm: PathBuf,
}

impl AssetSource {
    /// The packaged tree for this run: `<prefix>/usr/share/omarchy` under
    /// `--root`, otherwise `$OMARCHY_PATH` (see `omarchy::omarchy_path`).
    pub fn discover(layout: &crate::paths::Layout) -> Self {
        Self::at(crate::omarchy::omarchy_path(layout))
    }

    pub fn at(omarchy_path: impl AsRef<Path>) -> Self {
        let base = omarchy_path.as_ref();
        Self {
            omarchy_plymouth: base.join("default/plymouth"),
            omarchy_sddm: base.join("default/sddm/omarchy"),
        }
    }

    fn resolve(
        &self,
        theme: &ValidTheme,
        name: &str,
        packaged_dir: &Path,
        fallback: Option<&str>,
    ) -> Result<PathBuf> {
        let in_theme = theme.dir().join(name);
        if in_theme.is_file() {
            return Ok(in_theme);
        }
        let packaged = packaged_dir.join(name);
        if packaged.is_file() {
            return Ok(packaged);
        }
        if let Some(fallback) = fallback {
            let in_theme = theme.dir().join(fallback);
            if in_theme.is_file() {
                return Ok(in_theme);
            }
            let packaged = packaged_dir.join(fallback);
            if packaged.is_file() {
                return Ok(packaged);
            }
        }
        Err(Error::step(
            "generate",
            format!(
                "the asset {name} is in neither the theme directory {} nor the packaged tree {}",
                theme.dir().display(),
                packaged_dir.display()
            ),
            format!(
                "add {name} to the theme, or put an Omarchy tree with default/plymouth at {} (OMARCHY_PATH names it; under --root it is <prefix>/usr/share/omarchy)",
                packaged_dir
                    .parent()
                    .and_then(|p| p.parent())
                    .unwrap_or(packaged_dir)
                    .display()
            ),
        ))
    }
}

/// Build every file for one theme.
pub fn generate(theme: &ValidTheme, assets: &AssetSource) -> Result<GeneratedTheme> {
    let plymouth = plymouth::generate(theme, assets)?;
    let sddm = sddm::generate(theme, assets)?;
    Ok(GeneratedTheme { plymouth, sddm })
}

pub(crate) fn copy_of(path: &Path, name: &str) -> GeneratedFile {
    GeneratedFile {
        name: name.to_string(),
        content: Content::CopyOf(path.to_path_buf()),
    }
}

pub(crate) fn derived(name: &str, bytes: Vec<u8>, origin: String) -> GeneratedFile {
    GeneratedFile {
        name: name.to_string(),
        content: Content::Derived { bytes, origin },
    }
}

/// Load an image, recolour it the way `omarchy-plymouth-set` does, and encode
/// it as a PNG.
pub(crate) fn recoloured(
    source: &Path,
    name: &str,
    colour: crate::theme::Rgb,
) -> Result<GeneratedFile> {
    let bytes =
        fs::read(source).map_err(|source_error| Error::read(source.to_path_buf(), source_error))?;
    let image = crate::render::decode(&bytes, &source.display().to_string())?;
    let recoloured = crate::render::recolour(&image, colour);
    Ok(derived(
        name,
        crate::render::encode_png(&recoloured)?,
        format!("{} recoloured to {}", source.display(), colour.hex()),
    ))
}

/// Copy a PNG logo, or rasterise an SVG one.
pub(crate) fn logo_file(source: &Path, name: &str) -> Result<GeneratedFile> {
    if source.extension().and_then(|extension| extension.to_str()) != Some("svg") {
        return Ok(copy_of(source, name));
    }
    let bytes =
        fs::read(source).map_err(|source_error| Error::read(source.to_path_buf(), source_error))?;
    let raster = crate::render::rasterise_svg(&bytes, None, &source.display().to_string())?;
    Ok(derived(
        name,
        crate::render::encode_png(&raster)?,
        format!(
            "{} rasterised to {}x{}",
            source.display(),
            raster.width(),
            raster.height()
        ),
    ))
}

pub(crate) fn text(name: &str, body: String) -> GeneratedFile {
    GeneratedFile {
        name: name.to_string(),
        content: Content::Text(body),
    }
}

#[cfg(test)]
pub(crate) mod fixture {
    use std::fs;
    use std::path::{Path, PathBuf};

    use crate::theme::{Theme, ValidTheme};

    /// The natural size of each packaged glyph, close enough to the real ones
    /// that a layout test means something.
    pub fn glyph_size(name: &str) -> (u32, u32) {
        match name {
            "bullet.png" => (7, 7),
            "entry.png" => (400, 50),
            "lock.png" | "lock-failed.png" => (84, 96),
            "entry-failed.png" => (400, 50),
            "progress_bar.png" => (396, 16),
            "progress_box.png" => (400, 20),
            _ => (32, 32),
        }
    }

    /// A solid PNG of one colour.
    pub fn png(width: u32, height: u32, colour: [u8; 4]) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba(colour));
        crate::render::encode_png(&image).unwrap()
    }

    /// A PNG that is a two pixel outline with a transparent middle, which is
    /// the shape the real entry and progress box have. Having one of these in
    /// the fixtures means a recolour is tested against an image with alpha in
    /// it, and that anything drawn inside the entry is actually visible.
    pub fn png_frame(width: u32, height: u32, colour: [u8; 4]) -> Vec<u8> {
        let mut image = image::RgbaImage::from_pixel(width, height, image::Rgba(colour));
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let inside = x >= 2 && y >= 2 && x + 2 < width && y + 2 < height;
            if inside {
                pixel.0[3] = 0;
            }
        }
        crate::render::encode_png(&image).unwrap()
    }

    /// Glyphs that are outlines rather than solid shapes.
    fn is_outline(name: &str) -> bool {
        matches!(name, "entry.png" | "entry-failed.png" | "progress_box.png")
    }

    fn glyph_bytes(name: &str) -> Vec<u8> {
        let (width, height) = glyph_size(name);
        if is_outline(name) {
            png_frame(width, height, [200, 200, 200, 255])
        } else {
            png(width, height, [200, 200, 200, 255])
        }
    }

    /// A packaged Omarchy tree with just enough assets to generate from.
    pub fn omarchy_tree(root: &Path) -> PathBuf {
        let plymouth = root.join("default/plymouth");
        let sddm = root.join("default/sddm/omarchy");
        fs::create_dir_all(&plymouth).unwrap();
        fs::create_dir_all(&sddm).unwrap();
        for name in super::PLYMOUTH_GLYPHS {
            fs::write(plymouth.join(name), glyph_bytes(name)).unwrap();
        }
        for (name, _) in super::SDDM_GLYPHS {
            fs::write(sddm.join(name), glyph_bytes(name)).unwrap();
        }
        root.to_path_buf()
    }

    /// A theme directory holding the given manifest plus a logo.
    pub fn theme(dir: &Path, manifest: &str) -> ValidTheme {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("theme.toml"), manifest).unwrap();
        if !dir.join("logo.png").exists() && !dir.join("logo.svg").exists() {
            fs::write(dir.join("logo.png"), png(300, 100, [255, 255, 255, 255])).unwrap();
        }
        Theme::load(dir).expect("fixture theme should validate")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_changes_when_any_file_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let omarchy = fixture::omarchy_tree(&tmp.path().join("omarchy"));
        let assets = AssetSource::at(&omarchy);
        let theme = fixture::theme(&tmp.path().join("t"), "[meta]\nname = \"T\"\n");

        let first = generate(&theme, &assets).unwrap().hash().unwrap();
        let second = generate(&theme, &assets).unwrap().hash().unwrap();
        assert_eq!(first, second, "generation must be deterministic");

        let other = fixture::theme(
            &tmp.path().join("u"),
            "[meta]\nname = \"T\"\n[colors]\nbackground = \"#000000\"\n",
        );
        assert_ne!(first, generate(&other, &assets).unwrap().hash().unwrap());
    }

    #[test]
    fn a_missing_packaged_asset_names_the_file_and_the_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let assets = AssetSource::at(tmp.path().join("no-omarchy-here"));
        let theme = fixture::theme(&tmp.path().join("t"), "[meta]\nname = \"T\"\n");
        let error = generate(&theme, &assets).unwrap_err().to_string();
        assert!(error.contains("bullet.png"), "{error}");
        assert!(error.contains("OMARCHY_PATH"), "{error}");
    }

    /// The decoded size of one generated file, which is how an image is
    /// identified now that generation recolours rather than copies.
    fn size_of(files: &[GeneratedFile], name: &str) -> (u32, u32) {
        let file = files.iter().find(|file| file.name == name).expect(name);
        let image = crate::render::decode(&file.bytes().unwrap(), name).unwrap();
        (image.width(), image.height())
    }

    #[test]
    fn theme_assets_win_over_packaged_assets() {
        let tmp = tempfile::tempdir().unwrap();
        let omarchy = fixture::omarchy_tree(&tmp.path().join("omarchy"));
        let assets = AssetSource::at(&omarchy);
        let dir = tmp.path().join("t");
        let theme = fixture::theme(&dir, "[meta]\nname = \"T\"\n");
        std::fs::write(dir.join("bullet.png"), fixture::png(21, 21, [1, 2, 3, 255])).unwrap();
        let theme = crate::theme::Theme::load(theme.dir()).unwrap();

        let generated = generate(&theme, &assets).unwrap();
        assert_eq!(size_of(&generated.plymouth, "bullet.png"), (21, 21));
    }

    #[test]
    fn failed_state_assets_fall_back_to_their_normal_variant() {
        let tmp = tempfile::tempdir().unwrap();
        let omarchy = tmp.path().join("omarchy");
        fixture::omarchy_tree(&omarchy);
        std::fs::remove_file(omarchy.join("default/sddm/omarchy/lock-failed.png")).unwrap();
        let assets = AssetSource::at(&omarchy);
        let theme = fixture::theme(&tmp.path().join("t"), "[meta]\nname = \"T\"\n");

        let generated = generate(&theme, &assets).unwrap();
        assert_eq!(
            size_of(&generated.sddm, "lock-failed.png"),
            fixture::glyph_size("lock.png")
        );
    }
}
