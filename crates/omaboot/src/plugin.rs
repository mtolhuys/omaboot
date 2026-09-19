//! Installing and opening the Quattro plugin.
//!
//! The plugin is QML that runs inside `omarchy-shell`. It lives in the
//! repository under `plugin/` and is linked, not copied, into the place the
//! shell discovers plugins, so editing the QML is picked up by the shell's
//! hot reload. The two binaries are linked into `~/.local/bin`, which is
//! where the plugin looks for its engine.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::api::{PLUGIN_ID, plugin_source};
use crate::error::{Error, Result};
use crate::exec::which;
use crate::paths::Layout;

/// Where the shell looks for user plugins.
pub fn install_dir(layout: &Layout) -> PathBuf {
    layout.config_base().join("omarchy/plugins").join(PLUGIN_ID)
}

fn home(layout: &Layout) -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| layout.config_base().join(".."))
}

fn bin_dir(layout: &Layout) -> PathBuf {
    home(layout).join(".local/bin")
}

/// The plugin directory to install, or the reason there is none.
pub fn locate() -> Result<PathBuf> {
    plugin_source().ok_or_else(|| Error::Environment {
        what: "the plugin directory was not found next to this binary".to_string(),
        suggestion:
            "run this from the omaboot checkout (target/release/omaboot), or set OMABOOT_PLUGIN_DIR"
                .to_string(),
    })
}

pub fn install(layout: &Layout, source: &Path, dry_run: bool, out: &mut dyn Write) -> Result<()> {
    let target = install_dir(layout);
    let verb = if dry_run { "would link" } else { "linked" };

    link(source, &target, dry_run)?;
    writeln!(
        out,
        "{verb}     {} -> {}",
        target.display(),
        source.display()
    )
    .ok();

    let exe = std::env::current_exe().map_err(|error| Error::Environment {
        what: format!("the running binary could not be located: {error}"),
        suggestion: "run omaboot by its path".to_string(),
    })?;
    let bin = bin_dir(layout);
    for name in ["omaboot", "omaboot-apply"] {
        let binary = exe.with_file_name(name);
        if !binary.is_file() {
            writeln!(
                out,
                "skipped    {name}: {} is not there (cargo build --release builds both)",
                binary.display()
            )
            .ok();
            continue;
        }
        let dest = bin.join(name);
        if dest.exists() && !dest.is_symlink() {
            // A copy left there by hand goes stale the moment the binary is
            // rebuilt, so it gives way to a link, unless it is that binary.
            let same = fs::read(&dest).ok() == fs::read(&binary).ok();
            if same {
                writeln!(out, "kept       {} (identical copy)", dest.display()).ok();
                continue;
            }
            if !dry_run {
                fs::remove_file(&dest).map_err(|error| Error::write(dest.clone(), error))?;
            }
            writeln!(out, "replaced   {} (an older copy)", dest.display()).ok();
        }
        link(&binary, &dest, dry_run)?;
        writeln!(out, "{verb}     {} -> {}", dest.display(), binary.display()).ok();
    }

    if dry_run || layout.is_prefixed() {
        writeln!(out, "would run  omarchy-shell shell rescanPlugins").ok();
        writeln!(
            out,
            "would run  omarchy-shell shell setPluginEnabled {PLUGIN_ID} true"
        )
        .ok();
        return Ok(());
    }
    shell(layout, &["shell", "rescanPlugins"], out)?;
    // The rescan runs in the background; enabling before it has seen the
    // manifest answers "unknown". Wait for the plugin to be listed.
    if wait_until_listed(layout) == Some(false) {
        return Err(Error::Environment {
            what: format!(
                "omarchy-shell did not list {PLUGIN_ID} after the rescan; its log says why (journalctl --user -u omarchy-shell, or the terminal the shell runs in)"
            ),
            suggestion: format!(
                "check {}/manifest.json and run `omaboot plugin install` again",
                target.display()
            ),
        });
    }
    shell(
        layout,
        &["shell", "setPluginEnabled", PLUGIN_ID, "true"],
        out,
    )?;
    writeln!(
        out,
        "\nopen it with `omaboot`, or from the shell: omarchy-shell shell summon {PLUGIN_ID} '{{}}'"
    )
    .ok();
    Ok(())
}

pub fn uninstall(layout: &Layout, dry_run: bool, out: &mut dyn Write) -> Result<()> {
    let target = install_dir(layout);
    if target.is_symlink() {
        if !dry_run {
            fs::remove_file(&target).map_err(|error| Error::write(target.clone(), error))?;
        }
        writeln!(
            out,
            "{} {}",
            if dry_run { "would remove" } else { "removed" },
            target.display()
        )
        .ok();
    } else if target.exists() {
        return Err(Error::Environment {
            what: format!("{} is not a link omaboot made", target.display()),
            suggestion: "remove it yourself if that is what you want".to_string(),
        });
    } else {
        writeln!(out, "nothing at {}", target.display()).ok();
    }
    if dry_run || layout.is_prefixed() {
        writeln!(
            out,
            "would run  omarchy-shell shell setPluginEnabled {PLUGIN_ID} false"
        )
        .ok();
        writeln!(out, "would run  omarchy-shell shell rescanPlugins").ok();
        return Ok(());
    }
    shell(
        layout,
        &["shell", "setPluginEnabled", PLUGIN_ID, "false"],
        out,
    )?;
    shell(layout, &["shell", "rescanPlugins"], out)?;
    Ok(())
}

/// What the shell knows about the plugin: listed, and enabled.
fn listed(layout: &Layout) -> Option<(bool, bool)> {
    let tool = which(layout, "omarchy-shell")?;
    let output = Command::new(&tool)
        .args(["shell", "listPlugins"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    let entry = value
        .as_array()?
        .iter()
        .find(|entry| entry["id"].as_str() == Some(PLUGIN_ID));
    Some(match entry {
        Some(entry) => (true, entry["enabled"].as_bool().unwrap_or(false)),
        None => (false, false),
    })
}

/// Poll `listPlugins` for a few seconds after a rescan. `None` when the
/// shell cannot be asked at all.
fn wait_until_listed(layout: &Layout) -> Option<bool> {
    for _ in 0..25 {
        match listed(layout) {
            None => return None,
            Some((true, _)) => return Some(true),
            Some((false, _)) => std::thread::sleep(std::time::Duration::from_millis(200)),
        }
    }
    Some(false)
}

/// Summon the plugin in the running shell.
pub fn open(layout: &Layout, out: &mut dyn Write) -> Result<()> {
    if layout.is_prefixed() {
        writeln!(
            out,
            "would run  omarchy-shell shell summon {PLUGIN_ID} '{{}}'"
        )
        .ok();
        return Ok(());
    }
    let Some(tool) = which(layout, "omarchy-shell") else {
        return Err(Error::Environment {
            what: "omarchy-shell was not found, so there is no shell to open the plugin in"
                .to_string(),
            suggestion: "use the subcommands: omaboot status, omaboot new, omaboot apply"
                .to_string(),
        });
    };
    // A linked plugin is a development checkout, and the shell's file
    // watcher does not follow the link, so its code is reloaded here before
    // every open. A copied plugin is left alone.
    if install_dir(layout).is_symlink() {
        let _ = Command::new(&tool)
            .args(["shell", "rescanPlugins"])
            .output();
        let _ = wait_until_listed(layout);
    }
    // Not listed: a stale scan, or not installed. Listed but disabled:
    // enable. Either way the summon that follows is the one that matters.
    match listed(layout) {
        Some((false, _)) => {
            if !install_dir(layout).join("manifest.json").is_file() {
                return Err(Error::Environment {
                    what: "the plugin is not installed in omarchy-shell".to_string(),
                    suggestion: "run `omaboot plugin install` once".to_string(),
                });
            }
            let _ = Command::new(&tool)
                .args(["shell", "rescanPlugins"])
                .output();
            if wait_until_listed(layout) != Some(true) {
                return Err(Error::Environment {
                    what: format!(
                        "omarchy-shell does not list {PLUGIN_ID}, although it is linked at {}",
                        install_dir(layout).display()
                    ),
                    suggestion: "the shell's log says why it refused the manifest: journalctl --user -u omarchy-shell, or the terminal the shell runs in"
                        .to_string(),
                });
            }
            let _ = Command::new(&tool)
                .args(["shell", "setPluginEnabled", PLUGIN_ID, "true"])
                .output();
        }
        Some((true, false)) => {
            let _ = Command::new(&tool)
                .args(["shell", "setPluginEnabled", PLUGIN_ID, "true"])
                .output();
        }
        _ => {}
    }
    let output = Command::new(&tool)
        .args(["shell", "summon", PLUGIN_ID, "{}"])
        .output()
        .map_err(|error| Error::Environment {
            what: format!("omarchy-shell could not be run: {error}"),
            suggestion: "is the shell running?".to_string(),
        })?;
    let reply = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || reply.contains("unknown") {
        return Err(Error::Environment {
            what: format!(
                "the shell did not open the plugin (it said: {})",
                if reply.is_empty() {
                    String::from_utf8_lossy(&output.stderr).trim().to_string()
                } else {
                    reply
                }
            ),
            suggestion: "the shell's log says why: journalctl --user -u omarchy-shell, or the terminal the shell runs in"
                .to_string(),
        });
    }
    Ok(())
}

fn shell(layout: &Layout, args: &[&str], out: &mut dyn Write) -> Result<()> {
    let Some(tool) = which(layout, "omarchy-shell") else {
        writeln!(
            out,
            "omarchy-shell was not found; when the shell runs: omarchy-shell {}",
            args.join(" ")
        )
        .ok();
        return Ok(());
    };
    let output = Command::new(&tool)
        .args(args)
        .output()
        .map_err(|error| Error::Environment {
            what: format!("omarchy-shell could not be run: {error}"),
            suggestion: "is the shell running?".to_string(),
        })?;
    let reply = String::from_utf8_lossy(&output.stdout).trim().to_string();
    writeln!(
        out,
        "ran        omarchy-shell {}{}",
        args.join(" "),
        if reply.is_empty() {
            String::new()
        } else {
            format!("  ({reply})")
        }
    )
    .ok();
    Ok(())
}

/// Make `link` point at `target`, replacing a previous link and refusing to
/// replace anything that is not one.
fn link(target: &Path, link: &Path, dry_run: bool) -> Result<()> {
    if link.exists() && !link.is_symlink() {
        return Err(Error::Environment {
            what: format!("{} exists and is not a link", link.display()),
            suggestion: "move it out of the way first".to_string(),
        });
    }
    if dry_run {
        return Ok(());
    }
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent).map_err(|error| Error::write(parent.to_path_buf(), error))?;
    }
    if link.is_symlink() {
        fs::remove_file(link).map_err(|error| Error::write(link.to_path_buf(), error))?;
    }
    std::os::unix::fs::symlink(target, link)
        .map_err(|error| Error::write(link.to_path_buf(), error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_replaces_a_link_and_refuses_a_real_file() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target");
        fs::create_dir(&target).unwrap();
        let at = tmp.path().join("plugins/x");
        link(&target, &at, false).unwrap();
        assert_eq!(fs::read_link(&at).unwrap(), target);
        let other = tmp.path().join("other");
        fs::create_dir(&other).unwrap();
        link(&other, &at, false).unwrap();
        assert_eq!(fs::read_link(&at).unwrap(), other);

        let real = tmp.path().join("real");
        fs::write(&real, b"x").unwrap();
        assert!(link(&target, &real, false).is_err());
        assert_eq!(fs::read(&real).unwrap(), b"x");
    }

    #[test]
    fn a_dry_run_install_under_a_prefix_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let plugin = tmp.path().join("plugin");
        fs::create_dir_all(&plugin).unwrap();
        fs::write(plugin.join("manifest.json"), "{}").unwrap();
        let layout = Layout::with_dirs(
            Some(tmp.path().join("root")),
            tmp.path().join("config/omaboot"),
            tmp.path().join("state"),
        );
        let mut out = Vec::new();
        install(&layout, &plugin, true, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("would link"), "{text}");
        assert!(
            text.contains("setPluginEnabled mtolhuys.omaboot true"),
            "{text}"
        );
        assert!(!install_dir(&layout).exists());
    }
}
