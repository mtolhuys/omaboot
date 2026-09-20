//! What the Quattro plugin talks to.
//!
//! The plugin is QML and runs inside `omarchy-shell`; it never touches a
//! theme file or the system itself. Everything it shows comes from these
//! commands as JSON on stdout, and everything it changes goes through them
//! as argv. That keeps one implementation of every rule (validation,
//! ownership, the pipeline) and makes the plugin a thin, reviewable view.
//!
//! The shapes here are the contract with `plugin/Engine.qml`. Change both.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::exec::Tools;
use crate::generate::AssetSource;
use crate::omarchy::OmarchyThemes;
use crate::paths::Layout;
use crate::render::Screen;
use crate::state;
use crate::system::Snapshot;
use crate::theme::fields::{FieldId, Kind};
use crate::theme::{LoginBackground, MANIFEST, Rgb, ShutdownLogo, Theme};

/// The three screens, in the order the plugin shows them.
pub const SCREENS: [Screen; 3] = [Screen::Unlock, Screen::Login, Screen::Shutdown];

fn screen_key(screen: Screen) -> &'static str {
    match screen {
        Screen::Unlock => "unlock",
        Screen::Login => "login",
        Screen::Shutdown => "shutdown",
    }
}

/// Everything the plugin needs to draw its first frame: what boots now,
/// your themes, the Omarchy themes to borrow from, and the tools found.
pub fn inspect(layout: &Layout) -> Value {
    let snapshot = Snapshot::read(layout);
    let assets = AssetSource::discover(layout);
    let tools = Tools::detect(layout);
    let now = state::now_unix();

    let facts: serde_json::Map<String, Value> = SCREENS
        .into_iter()
        .map(|screen| {
            (
                screen_key(screen).to_string(),
                Value::Array(
                    snapshot
                        .facts(screen)
                        .into_iter()
                        .map(|fact| json!({"label": fact.label, "value": fact.value}))
                        .collect(),
                ),
            )
        })
        .collect();
    let pictures: serde_json::Map<String, Value> = SCREENS
        .into_iter()
        .map(|screen| {
            (
                screen_key(screen).to_string(),
                Value::Bool(snapshot.picture(screen, &assets).is_some()),
            )
        })
        .collect();

    let omarchy = OmarchyThemes::discover(layout);
    let omarchy_themes: Vec<Value> = omarchy
        .list()
        .into_iter()
        .map(|name| {
            let palette = omarchy.palette(&name).ok();
            json!({
                "name": name,
                // False for a theme without unlock.png: a theme made from it
                // starts with Omarchy's default logo, and `new` says so.
                "has_logo": omarchy.has_unlock_image(&name),
                "palette": palette.map(|p| json!({
                    "background": p.background,
                    "foreground": p.foreground,
                    "accent": p.accent,
                    "error": p.error,
                })),
            })
        })
        .collect();

    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "headline": snapshot.headline(),
        "sandbox": layout.root().map(|root| root.display().to_string()),
        "plymouth": {
            "theme": snapshot.plymouth.theme,
            "conf": snapshot.show(&snapshot.plymouth.conf),
            "dir": snapshot.plymouth.dir.as_ref().map(|dir| snapshot.show(dir)),
            "owner": owner_key(snapshot.plymouth.owner),
            "styled_by": snapshot.plymouth.styled_by,
            "background": snapshot.plymouth.background.map(|c| c.hex()),
            "foreground": snapshot.plymouth.foreground.map(|c| c.hex()),
            "logo": snapshot.plymouth.logo.as_ref().map(|logo| snapshot.show(logo)),
            // Set when something else puts its theme over this one at boot
            // (Lock Screen Explorer's boot screen); an apply is refused then.
            "overridden_by": snapshot.plymouth.overridden_by.as_ref().map(|over| json!({
                "plugin": over.plugin,
                "setting": over.setting,
                "state_file": snapshot.show(&over.state_file),
                "way_out": over.way_out,
            })),
        },
        "login": {
            "theme": snapshot.login.theme.as_ref().map(|theme| theme.name.clone()),
            "decided_by": snapshot.login.theme.as_ref().map(|theme| snapshot.show(&theme.decided_by)),
            "dir": snapshot.login.dir.as_ref().map(|dir| snapshot.show(dir)),
            "owner": owner_key(snapshot.login.owner),
            "background": snapshot.login.background.map(|c| c.hex()),
            "foreground": snapshot.login.foreground.map(|c| c.hex()),
            "accent": snapshot.login.accent.map(|c| c.hex()),
            "error": snapshot.login.error.map(|c| c.hex()),
            // True when omaboot's own drop-in is the one deciding, whichever
            // theme it names: the login screen can then be given back.
            "by_omaboot": snapshot.login.theme.as_ref().is_some_and(|t| t.decided_by == layout.sddm_dropin()),
            // SDDM's autologin as configured; whether the greeter is skipped
            // at boot is SDDM's to decide.
            "autologin": snapshot.login.autologin.as_ref().map(|auto| json!({
                "user": auto.user,
                "decided_by": snapshot.show(&auto.decided_by),
            })),
        },
        "prefs": prefs(layout),
        "facts": facts,
        "pictures": pictures,
        "warnings": snapshot.warnings(),
        "applied": snapshot.applied.as_ref().map(|applied| json!({
            "theme": applied.theme,
            "at": applied.applied_at_unix,
            "age": state::describe_age(applied.applied_at_unix, now),
            "current": snapshot.applied_is_current(),
        })),
        "can_copy_current": snapshot.derive("copy").is_ok(),
        "rollback": state::read_rollback(layout).ok().flatten().map(|point| json!({
            "previous_plymouth_theme": point.previous_plymouth_theme,
            "recorded_at": point.recorded_at_unix,
            "age": state::describe_age(point.recorded_at_unix, now),
        })),
        "themes": themes(layout, &snapshot),
        "themes_dir": layout.themes_dir().display().to_string(),
        "omarchy_themes": omarchy_themes,
        "active_omarchy_theme": active_omarchy_theme(layout),
        "tools": {
            "greeter": tools.greeter.as_ref().map(|g| g.name.clone()),
            "initramfs": tools.initramfs.as_ref().map(|i| i.path.display().to_string()),
            "helper": tools.helper.as_ref().map(|h| h.display().to_string()),
            "plymouth_set_default": tools.plymouth_set_default.as_ref().map(|p| p.display().to_string()),
        },
    })
}

fn owner_key(owner: crate::system::Owner) -> &'static str {
    match owner {
        crate::system::Owner::Omarchy => "omarchy",
        crate::system::Owner::Omaboot => "omaboot",
        crate::system::Owner::Other => "other",
    }
}

fn themes(layout: &Layout, snapshot: &Snapshot) -> Vec<Value> {
    let dir = layout.themes_dir();
    let mut ids: Vec<String> = match fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    };
    ids.sort();
    ids.into_iter()
        .map(|id| {
            let theme_dir = layout.theme_dir(&id);
            let (name, problem) = match Theme::read(&theme_dir) {
                Ok(theme) => (theme.manifest.meta.name, None),
                Err(error) => (id.clone(), Some(error.to_string())),
            };
            let valid = problem.is_none() && Theme::load(&theme_dir).is_ok();
            json!({
                "id": id,
                "name": name,
                "dir": theme_dir.display().to_string(),
                "problem": problem,
                "valid": valid,
                "boots": snapshot.applied_theme_id() == Some(id.as_str()),
            })
        })
        .collect()
}

/// The Omarchy theme the desktop is on.
///
/// Omarchy has kept this in two shapes under `~/.local/state/omarchy/current`
/// (`docs/UPSTREAM.md`): older releases linked `theme` at the theme's
/// directory, and Omarchy 4 stages a copy, so `theme` is a real directory
/// and the name is in `theme.name` beside it. The name is taken, in order,
/// from `theme.name`, from the link target, and finally by matching the
/// directory's `colors.toml` byte for byte against the themes on the system,
/// which is the one thing a staged copy still carries.
pub fn active_omarchy_theme(layout: &Layout) -> Option<String> {
    let current = layout.state_base().join("omarchy/current");
    if let Ok(name) = fs::read_to_string(current.join("theme.name")) {
        let name = name.trim();
        if !name.is_empty() && !name.contains('/') {
            return Some(name.to_string());
        }
    }
    let theme = current.join("theme");
    if let Ok(target) = fs::read_link(&theme) {
        return target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
    }
    let colors = fs::read(theme.join(crate::omarchy::COLORS)).ok()?;
    let themes = crate::omarchy::OmarchyThemes::discover(layout);
    themes.list().into_iter().find(|name| {
        themes
            .dir(name)
            .ok()
            .and_then(|dir| fs::read(dir.join(crate::omarchy::COLORS)).ok())
            .is_some_and(|bytes| bytes == colors)
    })
}

/// One theme of yours, with its manifest, its fields and the images in it.
pub fn show(layout: &Layout, id: &str) -> Result<Value> {
    let dir = layout.theme_dir(id);
    let theme = Theme::read(&dir)?;
    let problem = Theme::load(&dir).err().map(|error| error.to_string());
    Ok(describe(&theme, problem))
}

fn describe(theme: &Theme, problem: Option<String>) -> Value {
    let manifest = &theme.manifest;
    let fields: Vec<Value> = FieldId::ALL
        .into_iter()
        .map(|field| {
            let kind = match field.kind() {
                Kind::Colour => json!({"type": "colour"}),
                Kind::File => json!({"type": "file"}),
                Kind::Text { max } => json!({"type": "text", "max": max}),
                Kind::Number {
                    min,
                    max,
                    fine,
                    coarse,
                    integer,
                } => json!({
                    "type": "number", "min": min, "max": max,
                    "step": fine, "coarse": coarse, "integer": integer,
                }),
                Kind::Choice(options) => json!({"type": "choice", "options": options}),
                Kind::Toggle => json!({"type": "toggle"}),
            };
            let screens: Vec<&str> = SCREENS
                .into_iter()
                .filter(|screen| FieldId::for_screen(*screen).contains(&field))
                .map(screen_key)
                .collect();
            json!({
                "key": field.key(),
                "label": field.label(),
                "group": field.group(),
                "value": field.read(manifest),
                "kind": kind,
                "screens": screens,
            })
        })
        .collect();
    json!({
        "id": theme.id,
        "dir": theme.dir.display().to_string(),
        "name": manifest.meta.name,
        "problem": problem,
        "valid": problem.is_none(),
        "manifest": serde_json::to_value(manifest).unwrap_or(Value::Null),
        "fields": fields,
        "images": crate::theme::fields::image_files(&theme.dir),
    })
}

/// Set one or more `key=value` pairs on a theme and write it back. Every
/// value goes through the same validation the inspector used, so the file
/// never ends up with something the pipeline would refuse.
pub fn set(layout: &Layout, id: &str, assignments: &[String]) -> Result<Value> {
    let dir = layout.theme_dir(id);
    let mut theme = Theme::read(&dir)?;
    for assignment in assignments {
        let (key, value) = assignment
            .split_once('=')
            .ok_or_else(|| Error::Environment {
                what: format!("{assignment} is not key=value"),
                suggestion: "write it as colors.background=#1a1b26".to_string(),
            })?;
        let field = FieldId::parse(key.trim()).ok_or_else(|| Error::Environment {
            what: format!("{key} is not a theme property"),
            suggestion: format!(
                "use one of: {}",
                FieldId::ALL
                    .iter()
                    .map(|f| f.key())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        })?;
        field
            .write(&mut theme.manifest, value.trim())
            .map_err(|problem| Error::Environment {
                what: format!("{key} cannot be {value}: {problem}"),
                suggestion: "pick a value the property accepts".to_string(),
            })?;
    }
    save(&theme)?;
    let problem = Theme::load(&dir).err().map(|error| error.to_string());
    Ok(describe(&theme, problem))
}

/// Rename the theme as it is shown (not its directory).
pub fn rename(layout: &Layout, id: &str, name: &str) -> Result<Value> {
    let dir = layout.theme_dir(id);
    let mut theme = Theme::read(&dir)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Environment {
            what: "a theme needs a name".to_string(),
            suggestion: "type one".to_string(),
        });
    }
    theme.manifest.meta.name = name.to_string();
    save(&theme)?;
    let problem = Theme::load(&dir).err().map(|error| error.to_string());
    Ok(describe(&theme, problem))
}

fn save(theme: &Theme) -> Result<()> {
    let body = toml::to_string_pretty(&theme.manifest).map_err(|error| Error::Environment {
        what: format!("the theme could not be written as TOML: {error}"),
        suggestion: "this is a bug; report it".to_string(),
    })?;
    state::write_atomic(&theme.dir.join(MANIFEST), body.as_bytes())
}

/// Remove one of your themes: the directory under `~/.config/omaboot/themes`
/// and nothing else. What is installed on the system stays as it is.
pub fn delete(layout: &Layout, id: &str) -> Result<Value> {
    let dir = layout.theme_dir(id);
    if !dir.is_dir() {
        return Err(Error::ThemeNotFound {
            name: id.to_string(),
            dir: layout.themes_dir(),
        });
    }
    let snapshot = Snapshot::read(layout);
    let was_current = snapshot.applied_theme_id() == Some(id);
    fs::remove_dir_all(&dir).map_err(|error| Error::write(dir.clone(), error))?;
    Ok(json!({
        "removed": dir.display().to_string(),
        "was_current": was_current,
        "note": if was_current {
            "the installed copy keeps working until you reset or apply another theme"
        } else {
            "nothing on the system was touched"
        },
    }))
}

/// What a dropped image becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageRole {
    Logo,
    ShutdownLogo,
    Background,
}

impl ImageRole {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "logo" => Ok(Self::Logo),
            "shutdown-logo" => Ok(Self::ShutdownLogo),
            "background" => Ok(Self::Background),
            other => Err(Error::Environment {
                what: format!("{other} is not an image role"),
                suggestion: "use logo, shutdown-logo or background".to_string(),
            }),
        }
    }
}

/// Copy an image into a theme and point the manifest at it. A logo stays a
/// PNG or SVG as it is; anything else, and every background, is decoded and
/// written as PNG, since that is the one format Plymouth and the greeter
/// both read. The source is never modified.
pub fn add_image(layout: &Layout, id: &str, source: &Path, role: ImageRole) -> Result<Value> {
    let dir = layout.theme_dir(id);
    let mut theme = Theme::read(&dir)?;
    let source = source
        .canonicalize()
        .map_err(|error| Error::read(source.to_path_buf(), error))?;
    let bytes = fs::read(&source).map_err(|error| Error::read(source.clone(), error))?;
    let extension = source
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let stem = source
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "image".to_string());
    let stem: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();

    let (name, payload) = match role {
        ImageRole::Background => ("background.png".to_string(), to_png(&bytes, &source)?),
        ImageRole::Logo | ImageRole::ShutdownLogo => match extension.as_str() {
            "png" => {
                // Decode once so a file that only pretends to be a PNG is
                // refused here rather than by plymouthd at boot.
                crate::render::decode(&bytes, &source.display().to_string())?;
                (format!("{stem}.png"), bytes)
            }
            "svg" => {
                crate::render::rasterise_svg(&bytes, Some(64), &source.display().to_string())?;
                (format!("{stem}.svg"), bytes)
            }
            _ => (format!("{stem}.png"), to_png(&bytes, &source)?),
        },
    };
    // The unlock logo and the shutdown logo may not share a file name with
    // each other by accident, so a shutdown logo gets a suffix.
    let name = if role == ImageRole::ShutdownLogo && name == theme.manifest.logo.source {
        let (stem, ext) = name.rsplit_once('.').unwrap_or((&name, "png"));
        format!("{stem}-shutdown.{ext}")
    } else {
        name
    };

    fs::create_dir_all(&dir).map_err(|error| Error::write(dir.clone(), error))?;
    state::write_atomic(&dir.join(&name), &payload)?;
    match role {
        ImageRole::Logo => theme.manifest.logo.source = name,
        ImageRole::ShutdownLogo => theme.manifest.shutdown.logo = ShutdownLogo::Source(name),
        ImageRole::Background => {
            if theme.manifest.login.background == LoginBackground::Color {
                theme.manifest.login.background = LoginBackground::Image;
            }
        }
    }
    save(&theme)?;
    let problem = Theme::load(&dir).err().map(|error| error.to_string());
    Ok(describe(&theme, problem))
}

fn to_png(bytes: &[u8], source: &Path) -> Result<Vec<u8>> {
    let image = crate::render::decode(bytes, &source.display().to_string())?;
    crate::render::encode_png(&image)
}

/// Remove an image from a theme, unless the manifest still points at it.
pub fn remove_image(layout: &Layout, id: &str, name: &str) -> Result<Value> {
    let dir = layout.theme_dir(id);
    let theme = Theme::read(&dir)?;
    let relative = crate::paths::relative_asset(name)?;
    let in_use = theme.manifest.logo.source == name
        || theme.manifest.shutdown.logo == ShutdownLogo::Source(name.to_string())
        || (name == crate::theme::LOGIN_BACKGROUND
            && theme.manifest.login.background.needs_image());
    if in_use {
        return Err(Error::Environment {
            what: format!("{name} is in use by the theme"),
            suggestion: "point the theme at another image first".to_string(),
        });
    }
    let path = dir.join(relative);
    fs::remove_file(&path).map_err(|error| Error::write(path.clone(), error))?;
    let problem = Theme::load(&dir).err().map(|error| error.to_string());
    Ok(describe(&theme, problem))
}

/// The colours an image is made of, for picking a theme's colours from a
/// wallpaper. Median cut over a downscaled copy: deterministic, no
/// dependency, and good enough for a palette of eight.
pub fn palette(source: &Path) -> Result<Value> {
    let bytes = fs::read(source).map_err(|error| Error::read(source.to_path_buf(), error))?;
    let image = crate::render::decode(&bytes, &source.display().to_string())?;
    let small = crate::render::scale(&image, 96, 54);
    let pixels: Vec<[u8; 3]> = small
        .pixels()
        .filter(|p| p.0[3] > 8)
        .map(|p| [p.0[0], p.0[1], p.0[2]])
        .collect();
    let colours = median_cut(&pixels, 8);
    let suggestion = suggest(&colours);
    Ok(json!({
        "colours": colours.iter().map(|c| c.hex()).collect::<Vec<_>>(),
        "suggested": {
            "background": suggestion.0.hex(),
            "foreground": suggestion.1.hex(),
            "accent": suggestion.2.hex(),
        },
    }))
}

fn median_cut(pixels: &[[u8; 3]], count: usize) -> Vec<Rgb> {
    if pixels.is_empty() {
        return vec![Rgb { r: 0, g: 0, b: 0 }];
    }
    let mut buckets: Vec<Vec<[u8; 3]>> = vec![pixels.to_vec()];
    while buckets.len() < count {
        // Split the bucket with the widest channel range.
        let (index, channel) = buckets
            .iter()
            .enumerate()
            .filter(|(_, bucket)| bucket.len() > 1)
            .map(|(index, bucket)| {
                let (channel, range) = (0..3)
                    .map(|c| {
                        let min = bucket.iter().map(|p| p[c]).min().unwrap_or(0);
                        let max = bucket.iter().map(|p| p[c]).max().unwrap_or(0);
                        (c, max - min)
                    })
                    .max_by_key(|(_, range)| *range)
                    .unwrap_or((0, 0));
                (index, channel, range)
            })
            .max_by_key(|(_, _, range)| *range)
            .map(|(index, channel, _)| (index, channel))
            .unwrap_or((usize::MAX, 0));
        if index == usize::MAX {
            break;
        }
        let mut bucket = buckets.remove(index);
        bucket.sort_by_key(|p| p[channel]);
        let half = bucket.len() / 2;
        let right = bucket.split_off(half);
        buckets.push(bucket);
        buckets.push(right);
    }
    let mut colours: Vec<(usize, Rgb)> = buckets
        .iter()
        .filter(|bucket| !bucket.is_empty())
        .map(|bucket| {
            let n = bucket.len() as u64;
            let sum = bucket.iter().fold([0u64; 3], |acc, p| {
                [
                    acc[0] + u64::from(p[0]),
                    acc[1] + u64::from(p[1]),
                    acc[2] + u64::from(p[2]),
                ]
            });
            (
                bucket.len(),
                Rgb {
                    r: (sum[0] / n) as u8,
                    g: (sum[1] / n) as u8,
                    b: (sum[2] / n) as u8,
                },
            )
        })
        .collect();
    // Most common first, so the plugin can show them in that order.
    colours.sort_by_key(|(count, _)| std::cmp::Reverse(*count));
    colours.into_iter().map(|(_, c)| c).collect()
}

fn luminance(c: Rgb) -> f64 {
    0.2126 * f64::from(c.r) + 0.7152 * f64::from(c.g) + 0.0722 * f64::from(c.b)
}

/// How much an accent a colour would make: its chroma, discounted when it
/// is too dark to read as a colour at all.
fn accent_score(c: Rgb) -> f64 {
    let max = f64::from(c.r.max(c.g).max(c.b));
    let min = f64::from(c.r.min(c.g).min(c.b));
    let chroma = max - min;
    if luminance(c) < 50.0 {
        chroma * 0.3
    } else {
        chroma
    }
}

/// A background, a text colour and an accent from a palette: the darkest
/// colour, the lightest, and the most saturated one that is neither.
fn suggest(colours: &[Rgb]) -> (Rgb, Rgb, Rgb) {
    let fallback = crate::omarchy::Palette::fallback();
    let parse = |value: &str| Rgb::parse(value).expect("fallback palette is valid");
    let Some(first) = colours.first().copied() else {
        return (
            parse(&fallback.background),
            parse(&fallback.foreground),
            parse(&fallback.accent),
        );
    };
    let background = colours
        .iter()
        .copied()
        .min_by(|a, b| luminance(*a).total_cmp(&luminance(*b)))
        .unwrap_or(first);
    let foreground = colours
        .iter()
        .copied()
        .max_by(|a, b| luminance(*a).total_cmp(&luminance(*b)))
        .unwrap_or(first);
    // Text on a dark background has to be readable: lift it when the image
    // is dark all over.
    let foreground = if luminance(foreground) - luminance(background) < 96.0 {
        parse(&fallback.foreground)
    } else {
        foreground
    };
    let accent = colours
        .iter()
        .copied()
        .filter(|c| *c != background && *c != foreground)
        .max_by(|a, b| accent_score(*a).total_cmp(&accent_score(*b)))
        .unwrap_or(foreground);
    (background, foreground, accent)
}

/// Preferences of the window (its layout, for now): a small TOML file under
/// `~/.config/omaboot/prefs.toml`, read and written only through here so the
/// plugin never opens a file.
pub fn prefs(layout: &Layout) -> Value {
    let path = layout.config_dir().join("prefs.toml");
    let text = fs::read_to_string(&path).unwrap_or_default();
    let table: toml::Table = toml::from_str(&text).unwrap_or_default();
    let mut out = serde_json::Map::new();
    for (key, value) in table {
        out.insert(
            key,
            match value {
                toml::Value::String(s) => Value::String(s),
                toml::Value::Boolean(b) => Value::Bool(b),
                toml::Value::Integer(i) => Value::from(i),
                toml::Value::Float(f) => Value::from(f),
                other => Value::String(other.to_string()),
            },
        );
    }
    Value::Object(out)
}

/// Set `key=value` pairs in the preferences. Keys are plain words; values
/// are kept as strings, which is all the window needs.
pub fn set_prefs(layout: &Layout, assignments: &[String]) -> Result<Value> {
    let path = layout.config_dir().join("prefs.toml");
    let text = fs::read_to_string(&path).unwrap_or_default();
    let mut table: toml::Table = toml::from_str(&text).unwrap_or_default();
    for assignment in assignments {
        let (key, value) = assignment
            .split_once('=')
            .ok_or_else(|| Error::Environment {
                what: format!("{assignment} is not key=value"),
                suggestion: "write it as layout=stacked".to_string(),
            })?;
        let key = key.trim();
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(Error::Environment {
                what: format!("{key} is not a preference name"),
                suggestion: "use letters, digits and underscores".to_string(),
            });
        }
        table.insert(
            key.to_string(),
            toml::Value::String(value.trim().to_string()),
        );
    }
    let body = toml::to_string_pretty(&table).map_err(|error| Error::Environment {
        what: format!("the preferences could not be written: {error}"),
        suggestion: "this is a bug; report it".to_string(),
    })?;
    state::write_atomic(&path, body.as_bytes())?;
    Ok(prefs(layout))
}

/// Where the plugin lives inside the repository or the package, found from
/// the running binary: `<root>/plugin/manifest.json` walking up from the
/// executable, then the packaged location.
pub fn plugin_source() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("OMABOOT_PLUGIN_DIR") {
        let dir = PathBuf::from(dir);
        if dir.join("manifest.json").is_file() {
            return Some(dir);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut cursor = exe.parent();
        while let Some(dir) = cursor {
            let candidate = dir.join("plugin");
            if candidate.join("manifest.json").is_file() {
                return Some(candidate);
            }
            cursor = dir.parent();
        }
    }
    let packaged = PathBuf::from("/usr/share/omaboot/plugin");
    packaged.join("manifest.json").is_file().then_some(packaged)
}

/// The plugin id, as the manifest declares it.
pub const PLUGIN_ID: &str = "mtolhuys.omaboot";

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
        fixture::theme(&layout.theme_dir("mine"), "[meta]\nname = \"Mine\"\n");
        (tmp, layout)
    }

    #[test]
    fn inspect_describes_an_empty_machine_and_the_themes_on_it() {
        let (_tmp, layout) = world();
        let value = inspect(&layout);
        assert_eq!(value["plymouth"]["theme"], Value::Null);
        assert_eq!(value["themes"][0]["id"], "mine");
        assert_eq!(value["themes"][0]["valid"], true);
        assert_eq!(value["themes"][0]["boots"], false);
        assert_eq!(value["pictures"]["unlock"], false);
        assert_eq!(value["can_copy_current"], false);
        assert!(value["facts"]["login"].as_array().unwrap().len() > 2);
    }

    /// A packaged Omarchy theme in the sandbox, with the colours given.
    fn packaged_theme(tmp: &Path, name: &str, colors: &str) -> PathBuf {
        let dir = tmp.join("root/usr/share/omarchy/themes").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("colors.toml"), colors).unwrap();
        dir
    }

    #[test]
    fn the_active_omarchy_theme_comes_from_theme_name_when_omarchy_writes_one() {
        // Omarchy 4: current/theme is a staged copy, current/theme.name the name.
        let (tmp, layout) = world();
        let current = tmp.path().join("omarchy/current");
        fs::create_dir_all(current.join("theme")).unwrap();
        fs::write(
            current.join("theme/colors.toml"),
            "background = \"#000000\"\n",
        )
        .unwrap();
        fs::write(current.join("theme.name"), "catppuccin\n").unwrap();
        assert_eq!(active_omarchy_theme(&layout).as_deref(), Some("catppuccin"));
    }

    #[test]
    fn the_active_omarchy_theme_comes_from_the_link_when_there_is_no_name_file() {
        // Older Omarchy: current/theme is a link at the theme's directory.
        let (tmp, layout) = world();
        let target = packaged_theme(tmp.path(), "nord", "background = \"#2e3440\"\n");
        let current = tmp.path().join("omarchy/current");
        fs::create_dir_all(&current).unwrap();
        std::os::unix::fs::symlink(&target, current.join("theme")).unwrap();
        assert_eq!(active_omarchy_theme(&layout).as_deref(), Some("nord"));
    }

    #[test]
    fn the_active_omarchy_theme_is_matched_by_its_colours_when_only_the_copy_is_there() {
        // A staged copy without theme.name: the colours say which theme it is.
        let (tmp, layout) = world();
        packaged_theme(tmp.path(), "nord", "background = \"#2e3440\"\n");
        packaged_theme(tmp.path(), "tokyo-night", "background = \"#1a1b26\"\n");
        let current = tmp.path().join("omarchy/current");
        fs::create_dir_all(current.join("theme")).unwrap();
        fs::write(
            current.join("theme/colors.toml"),
            "background = \"#1a1b26\"\n",
        )
        .unwrap();
        assert_eq!(
            active_omarchy_theme(&layout).as_deref(),
            Some("tokyo-night")
        );

        fs::write(
            current.join("theme/colors.toml"),
            "background = \"#123456\"\n",
        )
        .unwrap();
        assert_eq!(
            active_omarchy_theme(&layout),
            None,
            "colours nobody ships match nothing"
        );
    }

    #[test]
    fn no_current_theme_means_no_active_omarchy_theme() {
        let (_tmp, layout) = world();
        assert_eq!(active_omarchy_theme(&layout), None);
    }

    #[test]
    fn show_lists_every_field_with_its_kind_and_screens() {
        let (_tmp, layout) = world();
        let value = show(&layout, "mine").unwrap();
        let fields = value["fields"].as_array().unwrap();
        assert_eq!(fields.len(), FieldId::ALL.len());
        let prompt = fields.iter().find(|f| f["key"] == "unlock.prompt").unwrap();
        assert_eq!(prompt["kind"]["type"], "choice");
        assert_eq!(prompt["screens"], json!(["unlock"]));
        assert_eq!(prompt["value"], "bullets");
        assert_eq!(value["images"], json!(["logo.png"]));
    }

    #[test]
    fn set_validates_and_writes_the_manifest() {
        let (_tmp, layout) = world();
        let value = set(
            &layout,
            "mine",
            &[
                "colors.background=#000000".to_string(),
                "login.clock=false".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(value["manifest"]["colors"]["background"], "#000000");
        assert_eq!(value["manifest"]["login"]["clock"], false);
        let written = fs::read_to_string(layout.theme_dir("mine").join("theme.toml")).unwrap();
        assert!(written.contains("#000000"), "{written}");

        let error = set(&layout, "mine", &["colors.background=purple".to_string()])
            .unwrap_err()
            .to_string();
        assert!(error.contains("cannot be purple"), "{error}");
        let error = set(&layout, "mine", &["nope=1".to_string()])
            .unwrap_err()
            .to_string();
        assert!(error.contains("not a theme property"), "{error}");
    }

    #[test]
    fn a_dropped_jpeg_becomes_a_png_logo_and_the_manifest_follows() {
        let (tmp, layout) = world();
        let image = image::RgbaImage::from_pixel(40, 20, image::Rgba([10, 200, 30, 255]));
        let source = tmp.path().join("My Logo.jpg");
        image::DynamicImage::ImageRgba8(image)
            .to_rgb8()
            .save_with_format(&source, image::ImageFormat::Jpeg)
            .unwrap();

        let value = add_image(&layout, "mine", &source, ImageRole::Logo).unwrap();
        assert_eq!(value["manifest"]["logo"]["source"], "My-Logo.png");
        assert!(layout.theme_dir("mine").join("My-Logo.png").is_file());
        assert_eq!(value["valid"], true);
        assert!(source.is_file(), "the source is never touched");
    }

    #[test]
    fn a_dropped_background_switches_the_login_to_an_image() {
        let (tmp, layout) = world();
        let source = tmp.path().join("wall.png");
        fs::write(&source, fixture::png(64, 32, [40, 40, 90, 255])).unwrap();
        let value = add_image(&layout, "mine", &source, ImageRole::Background).unwrap();
        assert_eq!(value["manifest"]["login"]["background"], "image");
        assert!(layout.theme_dir("mine").join("background.png").is_file());
        assert_eq!(value["valid"], true);

        let error = remove_image(&layout, "mine", "background.png")
            .unwrap_err()
            .to_string();
        assert!(error.contains("in use"), "{error}");
        set(&layout, "mine", &["login.background=color".to_string()]).unwrap();
        remove_image(&layout, "mine", "background.png").unwrap();
        assert!(!layout.theme_dir("mine").join("background.png").exists());
    }

    #[test]
    fn a_file_that_is_not_an_image_is_refused_and_nothing_is_written() {
        let (tmp, layout) = world();
        let source = tmp.path().join("notes.png");
        fs::write(&source, b"not a png at all").unwrap();
        assert!(add_image(&layout, "mine", &source, ImageRole::Logo).is_err());
        assert!(!layout.theme_dir("mine").join("notes.png").exists());
    }

    #[test]
    fn a_palette_finds_the_colours_an_image_is_made_of() {
        let tmp = tempfile::tempdir().unwrap();
        let mut image = image::RgbaImage::from_pixel(64, 64, image::Rgba([20, 20, 40, 255]));
        for (x, _, pixel) in image.enumerate_pixels_mut() {
            if x > 48 {
                *pixel = image::Rgba([240, 120, 30, 255]);
            } else if x > 40 {
                *pixel = image::Rgba([230, 230, 230, 255]);
            }
        }
        let source = tmp.path().join("wall.png");
        fs::write(&source, crate::render::encode_png(&image).unwrap()).unwrap();
        let value = palette(&source).unwrap();
        let colours = value["colours"].as_array().unwrap();
        assert!(colours.len() >= 3 && colours.len() <= 8, "{colours:?}");
        let background = Rgb::parse(value["suggested"]["background"].as_str().unwrap()).unwrap();
        assert!(luminance(background) < 60.0, "{background:?}");
        let accent = Rgb::parse(value["suggested"]["accent"].as_str().unwrap()).unwrap();
        assert!(accent.r > 150 && accent.b < 120, "the orange: {accent:?}");
        let foreground = Rgb::parse(value["suggested"]["foreground"].as_str().unwrap()).unwrap();
        assert!(luminance(foreground) > 150.0, "{foreground:?}");
    }

    #[test]
    fn a_dark_image_still_gets_readable_text() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("dark.png");
        fs::write(&source, fixture::png(32, 32, [10, 12, 20, 255])).unwrap();
        let value = palette(&source).unwrap();
        let foreground = Rgb::parse(value["suggested"]["foreground"].as_str().unwrap()).unwrap();
        assert!(luminance(foreground) > 150.0, "{foreground:?}");
    }

    #[test]
    fn delete_removes_the_directory_and_nothing_else() {
        let (_tmp, layout) = world();
        let value = delete(&layout, "mine").unwrap();
        assert_eq!(value["was_current"], false);
        assert!(!layout.theme_dir("mine").exists());
        assert!(delete(&layout, "mine").is_err());
    }

    #[test]
    fn prefs_round_trip_and_refuse_odd_keys() {
        let (_tmp, layout) = world();
        assert_eq!(prefs(&layout), json!({}));
        let value = set_prefs(&layout, &["layout=stacked".to_string()]).unwrap();
        assert_eq!(value["layout"], "stacked");
        assert_eq!(prefs(&layout)["layout"], "stacked");
        assert!(set_prefs(&layout, &["bad key=1".to_string()]).is_err());
        assert!(set_prefs(&layout, &["nothing".to_string()]).is_err());
    }

    #[test]
    fn rename_changes_the_shown_name_only() {
        let (_tmp, layout) = world();
        let value = rename(&layout, "mine", "  Ocean  ").unwrap();
        assert_eq!(value["name"], "Ocean");
        assert_eq!(value["id"], "mine");
        assert!(rename(&layout, "mine", "  ").is_err());
    }
}
