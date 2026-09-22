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

**The AUR is the channel.** omaboot is two compiled binaries plus a QML plugin
that is inert without them, so the package is the product and everything else
is a way of finding it. Two PKGBUILDs, which conflict with each other:

- `packaging/PKGBUILD-release` is `omaboot`, built from the tagged GitHub
  source tarball. `docs/RELEASING.md` is the checklist.
- `packaging/PKGBUILD` is `omaboot-git`, the same package from the tip of the
  default branch.

Both install the binaries to `/usr/bin` and the plugin to
`/usr/share/omaboot/plugin`, where the engine already looks, and leave the
development harness out. CI builds the `-git` one with `makepkg` on every push
and keeps the package as an artefact, so what the AUR would build is known to
build.

**Not static binaries on GitHub releases.** The audience runs Arch; a tarball
of binaries would be a second thing to keep current for nobody. The release
page carries the tag and the notes, and the package is the download.

**Not the Quattro plugin marketplace, for now.** The marketplace lists
companion plugins for separately installed apps (`org.omacalendar.widget`,
`akitaonrails.ai-usagebar`), so omaboot would be allowed there, but it would
need a repository of its own with `manifest.json` at its root
(`scripts/submission-feedback.mjs` in the marketplace: "New submissions require
one plugin with `manifest.json` in the repository root"), and it would give a
person a second way to install the same QML: the marketplace's clone under its
own id beside the package's link at `mtolhuys.omaboot`, two entries, two
windows. The package installs the plugin already. Revisit when someone asks for
it, with a generated plugin-only repository and `plugin install` taught to
recognise a marketplace clone.

- Menu integration through `~/.config/omarchy/extensions/omarchy-menu.jsonc`,
  which is the supported user overlay of the Omarchy menu. No upstream file is
  edited to get an entry under Style.
- A theme gallery in the repo README, with screenshots, taking community
  submissions as `export` files.

## Milestones

**M1, the spine.** Theme format, validator, asset generator, apply pipeline,
revert, reset. CLI only. Proven in a VM with a real LUKS volume.
Status: done, 20 September 2026. Apply, reset, an apply killed during the
initramfs rebuild, and revert ran in a disposable Omarchy 4.0.3 guest, and
the unlock prompt was seen from the guest's own initramfs against a LUKS2
volume (`docs/evidence/closing-round-2026-09-20.md`); the fourth real
apply on the reference machine showed all three screens
(`docs/evidence/apply-round-2026-09-20.md`).

**M2, the preview.** Real `plymouthd` preview of all three modes, real greeter
preview, and the in-terminal composited preview. This is the feature that sells
the project, so it comes before the window is pretty.
Status: built; the composite agrees with the real render to within a third
of a percent (`docs/evidence`, `scripts/verify-render.sh`), and the real
previews run from the window.

**M3, the window.** A Quattro shell plugin, per `docs/UI.md`. Ships when a stranger can retheme their boot
screen without reading documentation.
Status: built and used for the real applies; the stranger test has not
been run.

**M4, the ecosystem.** Export, import, gallery, `follow`, doctor, AUR package.
Status: the packaging is ready and proven by machine. Both PKGBUILDs are in
`packaging/`, CI builds the `-git` one with `makepkg` on every push and keeps
the package as an artefact, and its package ran in the guest
(20 September 2026); `docs/RELEASING.md` is the path from a tag to the AUR,
which nothing has walked yet. Export, import, gallery, `follow` and doctor are
not started.

## Definition of done for v1

- A fresh Omarchy install, a theme applied, a reboot, and the user sees their own
  unlock screen, their own greeter, and their own shutdown screen.
  Proven: the reference machine (20 September 2026) and the guest.
- `omaboot reset` followed by a reboot is indistinguishable from never having
  installed omaboot.
  Proven in the guest: the configuration, the theme directories and the
  state are as on the base image, and the rebuilt UKI is byte for byte
  the stock one.
- `omarchy update` on a machine with omaboot applied changes nothing about what
  the user sees.
  Read, not run: `omarchy-update` at `e5b0dc22` does not touch the boot
  screen (`docs/UPSTREAM.md`); `omarchy-refresh-plymouth` does, and shows
  as drift.
- Killing omaboot at any point during apply leaves a bootable system.
  Proven in the guest for a `kill -9` during the initramfs rebuild, twice
  (20 and 22 September 2026); the other steps are covered by the pipeline's
  order (nothing is switched before the rollback point is recorded) and not
  yet by a kill. What such a kill leaves is written down in
  `docs/evidence/release-round-2026-09-22.md`: a machine that boots, with the
  boot screen still Omarchy's and the shutdown and login screens already
  omaboot's, until a revert or a second apply.
