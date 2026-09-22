<img src="brand/icon-128.png" width="64" height="64" alt="omaboot: an open padlock on a 16-cell grid">

# omaboot

Design the three screens Omarchy shows around your session, in a window that
is part of your desktop:

1. the Plymouth **unlock** screen at boot (LUKS passphrase)
2. the SDDM **login** screen
3. the Plymouth **shutdown** screen

The lock screen of a running session is not one of them: that one is the
shell's (`omarchy.lock`, or Lock Screen Explorer when it is installed) and
omaboot leaves it alone.

Status: M1 is done. Applied on real hardware on 20 September 2026, from
the window, with all three screens seen after a reboot
(`docs/evidence/apply-round-2026-09-20.md`); the same day, in a disposable
Omarchy 4.0.3 guest, apply, reset and revert were proven, an apply killed
during the initramfs rebuild left a machine that boots, the unlock prompt
was seen against a real LUKS volume, and the package built with `makepkg`
(`docs/evidence/closing-round-2026-09-20.md`). The engine is built and
tested; the interface is a Quattro shell plugin.

From a checkout, which is the only way today:

```
cargo build --release                   # both binaries: the engine and the privileged helper
target/release/omaboot plugin install   # links the plugin into omarchy-shell and the binaries into ~/.local/bin
omaboot                                 # opens the window
cargo test --workspace                  # no root, no system calls, with or without OMARCHY_PATH set
scripts/ci-pipeline.sh                  # the apply pipeline end to end, in a temporary prefix
```

The package is built and tested on every push (`packaging/`,
`.github/workflows/ci.yml`) but is not on the AUR yet. When it is, it is
`omarchy pkg aur add omaboot` and then `omaboot plugin install`, and
`docs/RELEASING.md` is what stands between a tag and that line being true.

The engine and the helper are one build: before anything privileged runs,
`omaboot` asks `omaboot-apply protocol` and refuses a helper that answers
another number than it was built with (`crates/omaboot-apply/src/protocol.rs`,
compiled into both). `plugin install` says so too when the helper next to the
binary is from an older build.

`omaboot plugin install` makes three links and two IPC calls: the repository's
`plugin/` directory becomes `~/.config/omarchy/plugins/mtolhuys.omaboot`, the
two binaries are linked as `~/.local/bin/omaboot` and
`~/.local/bin/omaboot-apply`, then the shell rescans and enables the plugin.
Editing the QML is picked up by the shell's hot reload, and a rebuilt binary
is live at once. A copy of a binary already sitting in `~/.local/bin` is
kept when it is byte for byte the built one (`kept ... (identical copy)`)
and replaced by the link when it is not, since a copy goes stale at the
next build. `omaboot plugin uninstall` undoes it.

## The window

It opens on **what boots now**: the Plymouth theme `plymouthd.conf` names, the
SDDM theme that wins after every file in `/etc/sddm.conf.d` has been read and
which file decided it, who put each theme there, and the colours and logo read
from the installed files. Nothing in it is cached and nothing in it is edited
in place. Under it are **your themes**, the directories in
`~/.config/omaboot/themes`; the one that is what boots now, if any, is marked.

Every button says what it touches:

- **Make a theme from this** copies what boots now into a theme of yours: its
  colours and logo, or a copy of your applied theme when omaboot installed it.
- The logo's width is a share of the screen width, so a dropped file of any
  size and any display give the same picture; 0.42 matches Omarchy's own.
- The logo's size and place are set per screen: what the Unlock tab says is
  the default, and the Login and Shutdown tabs keep their own value once you
  change it there (`[logo.login]` and `[logo.shutdown]` in `theme.toml`).
- The picture in the middle follows every change. **Drop a PNG or SVG on it**
  for the logo, a JPG or PNG on the Login tab for the wallpaper; the wallpaper's
  colours then appear in the colour picker next to the palette of the Omarchy
  theme your desktop is on.
- **Show for real** runs the real `plymouthd` or the real greeter in a window
  with the theme (the installed one from the system entry). `plymouthd` needs
  root, so that asks for your password once.
- **Dry run** lists every operation an apply would perform and performs none.
- **Apply to the system** installs the theme into the omaboot directories,
  writes the one SDDM drop-in, sets the Plymouth default and rebuilds the
  initramfs, with the steps and every operation on screen. **Revert** puts the
  recorded state back; **Remove omaboot from the system** goes back to
  Omarchy's own themes.
- The system entry also says what would get in the way. When Lock Screen
  Explorer has its own boot screen set (it layers its theme over the
  initramfs at boot, so what `plymouthd.conf` names is not what is seen),
  the unlock and shutdown facts say so and **Apply** is refused with the
  way out, until it is set back to stock. When SDDM is configured to log a
  user in automatically, the login facts name the user and the file.
- **Use Omarchy's login screen** appears on the system entry when a
  third-party SDDM theme (Omarchy's on-screen keyboard login, for instance) is
  what logs you in. It writes omaboot's own drop-in with `Current=omarchy`, which
  sorts after the third party's, so Omarchy's stock login screen is back without
  touching the file that put the other one there. **Give the login screen
  back** removes that drop-in again. Neither runs while a theme of yours is
  applied; the login screen is then the theme's.
- The window is the usual editor shape: a sidebar with what boots now and
  your themes, the picture in the middle at the screen's own 16:9, and the
  inspector on the right with the properties of the selected screen and, at
  its foot, Dry run and Apply. Narrower than 1180 px the same three blocks
  stack: the sidebar becomes a strip of cards and the inspector goes under
  the picture, its groups side by side.

The window takes its colours, font and widgets from the shell, so it looks like
the rest of Omarchy by construction. The password goes to `sudo` on the
engine's stdin and is not kept; every later privileged command runs with
`sudo -n`, so nothing can ever hang waiting for a prompt nobody sees.

## As an app of its own

Inside the shell the window is one of the shell's, and a dock shows it under
Quickshell's app id and icon; Quickshell gives every window of a process the
same id. `omaboot app` runs the same QML in a Quickshell instance of its own
with `//@ pragma AppId omaboot`, so the window is called `omaboot`, and
`omaboot app install` writes the desktop entry and the icon that go with
that name (`~/.local/share/applications/omaboot.desktop`,
`~/.local/share/icons/hicolor/*/apps/omaboot.png`). After that it is
"omaboot" in the app launcher, with its own face in the dock. The generated
application in `~/.config/omaboot/app` is the plugin's files linked in beside
links to the shell's `Commons` and `Ui`, so it is themed exactly as the
plugin is; it is rewritten on every launch and never edited. A theme switch
reaches the shell over IPC; the app is not the shell, so it re-reads the
theme's `colors.toml` every two seconds and reloads the same two files the
moment it differs. Both ways of opening the window can be installed at once.

## The command line

Everything the window does is a subcommand, with `--json` for machines:

```
omaboot status                                   # what boots now, your themes, drift, the rollback target
omaboot new mine --from-current                  # a theme of yours, copied from what boots now
omaboot set mine colors.background=#1a1b26 login.clock=false
omaboot add-image mine ~/Pictures/logo.svg --as logo
omaboot add-image mine ~/Pictures/wall.jpg --as background
omaboot palette ~/Pictures/wall.jpg              # the colours it is made of
omaboot render mine --screen login --out /tmp/login.png
omaboot preview --current --screen unlock        # the real plymouthd, in a window, showing the installed theme
omaboot apply mine --dry-run
omaboot apply mine
omaboot revert
omaboot app                                      # the window in a Quickshell instance of its own, app id omaboot
omaboot app install                              # desktop entry and icons, so the launcher and the dock know it
omaboot login stock                              # Omarchy's login screen instead of a third-party one, via omaboot's drop-in
omaboot login release                            # remove that drop-in again
omaboot prefs key=value                          # window preferences in ~/.config/omaboot/prefs.toml; none are read yet
```

With `--root <prefix>` everything lands inside that prefix and no system
command runs; the header says `sandbox <prefix>` while it does. The Omarchy
tree is read from `<prefix>/usr/share/omarchy` too, whatever `OMARCHY_PATH`
says, so a sandboxed run reads nothing outside its prefix. That is the
way to try `apply` without touching the machine. `preview` does not run
against a prefix, it only prints what it would do.

`new --from-current` copies what boots now: the installed Plymouth theme's
background (from its script), text colour (from its glyphs) and logo, plus
the login theme's accent and error colours when its `theme.conf` has them;
when omaboot's own theme is installed it copies the theme it came from
instead. `new --from-omarchy-theme` takes the colours from that theme's
`colors.toml` and its `unlock.png` as a starting logo; a theme without one
(themes from the store often have none, Omarchy's bundled ones all do) gets
Omarchy's own logo, which is what Omarchy's boot screen shows for such a
theme too, and `new` says so. Either way the theme is complete and the dry
run works straight away. Without a flag you get a
manifest and add your own `logo.png`.

`render` draws a screen to a PNG from the same bytes an apply would install,
which is what the window shows. It touches nothing, and it agrees with the
real render to within a third of a percent of the pixels
(`docs/evidence`, made by `scripts/verify-render.sh`).

Every state-changing command takes `--dry-run` and `--root <prefix>`, and a
prefixed run never executes a real system command, so the whole pipeline can be
exercised without touching the machine it runs on.

## The mark

`brand/` holds the icon: an open padlock on a 16-cell grid, one colour on
transparent, in the same block construction as Omarchy's own wordmark and
icon. `icon.svg` uses `currentColor`; the PNGs, 16 to 1024 px, are in
omaboot's default accent (`#7aa2f7`), which reads on a light launcher and on
a dark one alike, the way Omarchy's own flat green icon does; `icon.txt` is
the block-character version, as Omarchy keeps for its own.
The window draws it itself (`plugin/Mark.qml`, from the same rows) in the
theme's accent, so it is crisp at any size and recolours with the desktop.

## Why this exists

Omarchy ships `omarchy plymouth set`. It changes exactly three things: background
colour, text colour, and the logo PNG. It does this by running `sed` over
`omarchy.script` and replacing two hex literals in `Main.qml`. Everything else
(layout, font, spinner behaviour, messages, a shutdown screen that differs from
the boot screen, anything about the login screen beyond two colours) is hard
coded in the Omarchy source tree.

[Discussion #2455](https://github.com/omacom/omarchy/discussions/2455) asks for
exactly this. It was opened in October 2025, has 12 votes, and the most recent
comment (May 2026) reports that the manual workaround does not work.

What exists today are individual themes with their own installers:
[omarchy-plymouth-nier](https://github.com/Willi005/omarchy-plymouth-nier),
[Thinkpad-boot-screen](https://github.com/Yilmaz41/Thinkpad-boot-screen),
[plymouth-theme-omarchy-mac](https://github.com/iamdanielh/plymouth-theme-omarchy-mac).
None of them is a way to design your own, and none of them covers SDDM.

## The one rule

**omaboot never writes into files Omarchy owns.**

It installs its own Plymouth theme at `/usr/share/plymouth/themes/omaboot` and
its own SDDM theme at `/usr/share/sddm/themes/omaboot`, then points the system at
them. `/usr/share/plymouth/themes/omarchy` and `/usr/share/sddm/themes/omarchy`
are left byte for byte untouched.

This single decision solves three problems at once:

- `omarchy-refresh-plymouth` (run by `omarchy-reinstall-configs`, and by any
  migration that chooses to) rewrites Omarchy's own theme directory, which
  omaboot never touched, so nothing of yours is clobbered. It does also run
  `plymouth-set-default-theme omarchy` and a rebuild, so after it your boot
  screen is stock again while omaboot's files sit there intact: `omaboot
  status` reports that as drift, the window shows it under "what boots now",
  and applying again is one click. `omarchy update` itself does not do this.
- Uninstalling is removing one drop-in file and running one command. The stock
  experience is still sitting there, intact.
- If upstream extends `omarchy plymouth`, omaboot does not conflict with it. See
  `docs/UPSTREAM.md`.

## What makes it worth building

- **Real preview, no reboot.** `plymouthd` plus `plymouth show-splash` renders the
  actual theme, including the password prompt and shutdown mode. `sddm-greeter-qt6
  --test-mode` renders the actual greeter. Upstream's `omarchy plymouth preview`
  composites a fake PNG with ImageMagick and opens `imv`.
- **Preview in the window.** The composited frame updates with every change,
  drawn from the same bytes an apply would install.
- **Both halves in one tool.** Nobody covers the SDDM side.
- **Reversible by construction.** Every apply is stageable, verifiable, and
  revertible, and the greeter is smoke tested before it is ever activated.

## Documents

| File | What it answers |
|------|-----------------|
| `docs/SPEC.md` | What the app does: scope, commands, theme format, milestones |
| `docs/UI.md` | What the window looks like and how it feels. Read this one twice. |
| `docs/ARCHITECTURE.md` | How it applies changes without breaking anyone's boot |
| `docs/UPSTREAM.md` | Verified facts about Omarchy internals, and the upstream risk strategy |
| `docs/DECISIONS.md` | Every choice the specifications did not pin down, and why |
| `docs/RELEASING.md` | What a release has to prove, in the order the proofs depend on each other |
