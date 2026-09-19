# omaboot: product specification

## Problem

An Omarchy user who wants their boot unlock, login, and shutdown screens to look
like anything other than "Omarchy logo, two colours" has three options today:

1. Edit files under `/usr/share/` that upstream overwrites on update.
2. Install someone else's complete theme package and live with their taste.
3. Give up.

## Who it is for

- People who rice. The same person who switches Omarchy themes weekly and wants
  the boot screen to follow.
- People who put a company logo on a work laptop.
- Theme authors who want to ship a boot theme without writing a PKGBUILD, pacman
  hooks, and initramfs handling first.

## Scope, v1

In scope:

- Design and apply a **Plymouth theme** covering the unlock screen (boot, resume)
  and the shutdown screen, with per-mode differences.
- Design and apply an **SDDM theme** for the login screen.
- Live preview of both, without rebooting and without logging out.
- Import colours and logo from the active Omarchy theme.
- Install, switch, revert, and fully uninstall, all reversible.
- Export and import an omaboot theme as a single shareable file.

Out of scope for v1, listed so the boundary is explicit:

- The hyprlock lock screen of a running session. That is `omarchy.lock` and
  [omarchy-lock-explorer](https://github.com/SirJul1337/omarchy-lock-explorer)
  already covers it.
- The Limine boot menu (before Plymouth). Candidate for v2.
- Display managers other than SDDM, and distros other than Omarchy.
- Wallpapers, GTK themes, anything about the running desktop.
- Video or shader backgrounds on the greeter. Tempting, and a good way to make
  logins slow and fragile.

## Commands

`omaboot` with no arguments opens the plugin window in omarchy-shell.
Everything the window does is also a subcommand, because scripting and
debugging need it and because the window is then a
front end over a testable core, not the place where the logic lives.

| Command | Does |
|---------|------|
| `omaboot` | Open the window (the Quattro plugin) |
| `omaboot plugin install` | Link the plugin and the binaries into place and enable it |
| `omaboot app` | The window in a Quickshell instance of its own, app id `omaboot`; `app install` and `app uninstall` add and remove the desktop entry and icons |
| `omaboot show <theme>` | The theme's properties, images and problems (`--json` for the plugin) |
| `omaboot set <theme> key=value...` | Change properties, validated, and save |
| `omaboot add-image <theme> <file> --as logo\|shutdown-logo\|background` | Copy an image in and point the theme at it |
| `omaboot palette <file>` | The colours an image is made of |
| `omaboot delete <theme>` | Remove one of your themes; the system is not touched |
| `omaboot list` | List installed omaboot themes and which is active |
| `omaboot status` | Active theme, drift check, last apply, rollback target |
| `omaboot preview <theme> [--screen unlock\|login\|shutdown]` | Real preview |
| `omaboot apply <theme>` | Full apply pipeline (see ARCHITECTURE.md) |
| `omaboot revert` | Restore the previous state recorded at last apply |
| `omaboot reset` | Return the system to stock Omarchy, remove all omaboot state |
| `omaboot login stock` / `omaboot login release` | Put Omarchy's stock login screen in front of a third-party SDDM theme through omaboot's drop-in, and take that drop-in away again; refused while a theme is applied |
| `omaboot prefs [key=value...]` | The window's preferences in `~/.config/omaboot/prefs.toml`; no key is read by the window yet |
| `omaboot doctor` | Diagnose: greeter failures in the journal, drift, missing deps |
| `omaboot new <name> [--from-omarchy-theme <t>]` | Scaffold a theme |
| `omaboot export <theme>` / `omaboot import <file>` | Share themes |
| `omaboot follow --enable\|--disable` | Track the active Omarchy theme |

Every command that changes system state supports `--dry-run` and prints the exact
operations it would perform.

## Theme format

A theme is a directory, human readable and diffable, so it can live in a git repo
and be shared as text plus a few PNGs.

```
~/.config/omaboot/themes/<name>/
  theme.toml
  logo.png            # or logo.svg, rasterised at apply time
  background.png      # optional, login screen only
```

`theme.toml`, sketched:

```toml
[meta]
name = "Tokyo Night Boot"
author = "mtolhuijs"
version = "1.0.0"

[colors]
background = "#1a1b26"
foreground = "#c0caf5"
accent     = "#7aa2f7"
error      = "#f7768e"

[logo]                        # the file, and its size and place on the unlock screen
source = "logo.png"
width = 0.42                  # share of the screen width; the same on 1080p and 4K, whatever the file's pixels
position = "center"          # center | top | custom
offset = [0, -40]            # applies when position is custom

[logo.login]                  # optional: where the login screen differs
width = 0.25                  # only the size matters there; the login layout places it

[logo.shutdown]               # optional: where the shutdown screen differs
width = 0.3
position = "top"
offset = [0, 0]               # anything left out follows [logo]

[unlock]                      # Plymouth, boot and resume
prompt = "bullets"            # bullets | asterisks | hidden | counter
progress = "bar"              # bar | spinner | none
message = ""

[shutdown]                    # Plymouth, shutdown and reboot
logo = "inherit"
message = "See you"
progress = "spinner"

[login]                       # SDDM
layout = "centered"           # centered | left | right
clock = true
show_session_picker = false
background = "color"          # color | image | blur
```

Unknown keys are a validation error, never a silent ignore. A theme that omits a
section inherits the built-in default for it.

## Distribution

- AUR package `omaboot`, plus static binaries on GitHub releases.
- Menu integration through `~/.config/omarchy/extensions/omarchy-menu.jsonc`,
  which is the supported user overlay of the Omarchy menu. No upstream file is
  edited to get an entry under Style.
- A theme gallery in the repo README, with screenshots, taking community
  submissions as `export` files.

## Milestones

**M1, the spine.** Theme format, validator, asset generator, apply pipeline,
revert, reset. CLI only. Proven in a VM with a real LUKS volume.

**M2, the preview.** Real `plymouthd` preview of all three modes, real greeter
preview, and the in-terminal composited preview. This is the feature that sells
the project, so it comes before the window is pretty.

**M3, the window.** A Quattro shell plugin, per `docs/UI.md`. Ships when a stranger can retheme their boot
screen without reading documentation.

**M4, the ecosystem.** Export, import, gallery, `follow`, doctor, AUR package.

## Definition of done for v1

- A fresh Omarchy install, a theme applied, a reboot, and the user sees their own
  unlock screen, their own greeter, and their own shutdown screen.
- `omaboot reset` followed by a reboot is indistinguishable from never having
  installed omaboot.
- `omarchy update` on a machine with omaboot applied changes nothing about what
  the user sees.
- Killing omaboot at any point during apply leaves a bootable system.
