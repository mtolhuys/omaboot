# Upstream: what exists, and how omaboot stays relevant

All findings below were read from the `quattro` branch of `omacom/omarchy`,
`version` file reporting `4.0.0.alpha`, on 18 September 2026. Re-verify against
the installed tree (`$OMARCHY_PATH`, typically `/usr/share/omarchy`) before
relying on any of it in code.

Re-verified against the installed tree on 18 September 2026, `version` still
reporting `4.0.0.alpha`. Three statements in the first draft of this document did
not survive that check and have been corrected in place: the length and the
claimed `layout()` of `omarchy.script`, the presence of `#ffffff` in `Main.qml`,
and the claim that `omarchy-theme-set` offers no user hook. Each correction is
marked **Corrected 18 September 2026** so a reader can tell a re-read from an
original claim.

## What upstream already ships

`bin/omarchy-plymouth-set` is the real thing to understand. It is not a naive
script:

- It refuses to run as root and refuses symlinked logos. It opens the logo file
  as the unprivileged user and passes the file descriptor into the privileged
  section, so a swapped symlink cannot be followed after `sudo`.
- The privileged part validates that the source tree is root owned and that every
  parent directory up to `/` is root owned and not group or world writable. A
  user-owned development checkout is only accepted when `/etc/omarchy.conf`
  contains exactly one line authorising it, written by `omarchy dev link`.
- It publishes to a fixed list of destination file names through root-owned
  staging plus atomic rename, and compares sizes and contents after copying.
- Assets it manages: `bullet.png`, `entry.png`, `lock.png`, `logo.png`,
  `omarchy.plymouth`, `omarchy.script`, `preview-unlock.png`, `progress_bar.png`,
  `progress_box.png`, plus `logos/oma.png` on refresh. For SDDM: `Main.qml`,
  `bullet.png`, `entry-failed.png`, `entry.png`, `lock-failed.png`, `lock.png`,
  `logo.png`, plus `metadata.desktop` and `theme.conf` on
  `--refresh-sddm-default`, which also removes a stale `logo.svg` from the SDDM
  theme directory.
- Every published asset must be between 1 byte and 64 MiB. The limit is passed
  into the privileged section as an argument and enforced on the logo read as
  well as on each publish. omaboot adopts the same ceiling.
- Privilege is acquired with `sudo`, not polkit. The whole privileged part is a
  single `sudo /bin/bash -c '...'` transaction with `PATH=/usr/bin:/bin` and
  `umask 077`.
- Customisation is exactly: two `sed` substitutions of
  `Window.SetBackgroundTopColor` / `BottomColor` in `omarchy.script`, an
  ImageMagick `+level-colors` recolour of four PNGs (`bullet`, `entry`, `lock`,
  `progress_bar`), replacement of `#1a1b26` and `#ffffff` in `Main.qml`, and the
  logo copy. The failed-login variants are recoloured to a hard coded `#f7768e`.
- **Corrected 18 September 2026.** The installed `Main.qml` contains no
  `#ffffff`. The only colour literal in it is `#1a1b26` on line 8, the root
  rectangle. The text-colour substitution is therefore a no-op today: the
  greeter's `TextInput` is `color: "transparent"` and every visible glyph comes
  from a recoloured PNG. Upstream's SDDM customisation is, in effect, one
  background colour plus recoloured bitmaps.
- Afterwards it always runs `plymouth-set-default-theme omarchy` and then
  `limine-mkinitcpio` if present, else `mkinitcpio -P`.

Adopt its hardening pattern in `omaboot-apply`. It is a good model and reviewers
from this community will recognise it.

Other relevant pieces:

- `omarchy-plymouth-set-by-theme` reads `background` and `foreground` from a
  theme's `colors.toml` and its `unlock.png`. Verified on 18 September 2026:
  every packaged theme under `$OMARCHY_PATH/themes` has an `unlock.png`, and
  every `colors.toml` has `background`, `foreground`, `accent` and `red`, along
  with roughly two dozen other keys. `omarchy-theme-dir` prefers a user copy in
  `~/.config/omarchy/themes/<name>` over the packaged one. omaboot's
  `new --from-omarchy-theme` reads exactly these and nothing else.
- `omarchy-plymouth-current` identifies the active theme by byte-comparing
  `logo.png`, which is a neat hack and also why a second theme installed
  elsewhere confuses nothing. It reports nothing at all once omaboot is active,
  because it only ever inspects the omarchy theme directory.
- **Corrected 18 September 2026.** `omarchy-theme-set` does offer a user hook.
  Its last steps are `omarchy-hook theme-set "$THEME_NAME"`, and `omarchy-hook`
  runs `~/.config/omarchy/hooks/<name>` followed by every file in
  `~/.config/omarchy/hooks/<name>.d/` (skipping `*.sample`), passing the hook
  arguments through and reporting a non-zero hook as `Hook failed: <path>`
  without aborting the caller. This is the supported integration point for
  following the active Omarchy theme, and it replaces the systemd path unit the
  first draft of `ARCHITECTURE.md` proposed.
- **Verified 19 September 2026**, on the reference machine (`4.0.0.alpha`,
  quattro) and in `bin/omarchy-theme-set` at upstream `e38c1d1` of the same
  day: `~/.local/state/omarchy/current/theme` is a real directory, not a
  link. `omarchy-theme-set` stages the packaged theme with the user's copy
  on top in a `next` directory, `mv`s it over `current/theme`, and writes the
  name to `current/theme.name`; `omarchy-theme-current` reads only that
  file. Earlier releases linked `current/theme` at the theme's directory.
  omaboot's `active_omarchy_theme` reads `theme.name` first, then a link
  target, then matches the copy's `colors.toml` against the installed
  themes, so both layouts resolve.
- `omarchy-plymouth-switcher` drives `omarchy-menu-images`, so a picker that
  looks native is available to any caller.
- `omarchy-plymouth-preview` composites a fake 1920x1080 PNG with ImageMagick and
  opens `imv`. This is the bar omaboot has to clear, and it is not high.
- Menu entries live in `default/omarchy/omarchy-menu.jsonc`, overlaid by the user
  file `~/.config/omarchy/extensions/omarchy-menu.jsonc`. Both paths are read by
  the first-party `omarchy.menu` shell plugin (`shell/plugins/menu/Menu.qml`,
  `defaultMenuPath` and `userMenuPath`), and `omarchy menu refresh` re-parses
  them. A broken user extension
  drops user entries but keeps the shipped menu working. This is the supported way
  to add omaboot to the menu.
- The shutdown path is `bin/omarchy-system-shutdown`: it schedules
  `systemctl poweroff` through the user manager, shows `omarchy-osd -i shutdown
  -m "Shutting down"`, then closes windows. The Plymouth shutdown screen is what
  the user sees after that.
- **Corrected 18 September 2026.** `default/plymouth/omarchy.script` is 235
  lines. It has the fake-progress curve (ease-out quadratic to 70% over 15s, with
  a `max_progress` ratchet so the bar never moves backwards), the password
  callback (bullets capped at 21, 7x7, 12px apart), and the message callback
  (`Image.Text(text, 1, 1, 1)`, so message text is hard coded white regardless of
  the theme). It has **no** `layout()` and no re-centring on display changes: all
  sprite positions are computed once at load from `Window.GetWidth()` and
  `Window.GetHeight()`. A display that appears after `plymouthd` started is not
  handled. Read the script before writing a replacement anyway, but do not
  inherit the assumption that this problem is solved upstream.
- `Plymouth.GetMode()` is used in `display_normal_callback` to show the progress
  bar only for `boot` and `resume`, which is the mechanism omaboot needs for
  per-mode differences. The `plymouth` client binary accepts `change-mode`.
- Verified 18 September 2026, both on the installed `plymouthd` here and on
  Plymouth 24.004 in a test container: `plymouthd --mode` accepts `boot` and
  `shutdown`, nothing else. plymouthd looks for a theme under
  `/run/plymouth/themes/<name>/` before `/usr/share/plymouth/themes/<name>/`,
  and `plymouth.splash=<name>` on its kernel command line (real or the one
  given to `--kernel-command-line`) overrides `Theme=` in `plymouthd.conf`.
  `Image.Text` draws nothing unless a label plugin is present; this machine has
  both `label-pango.so` and `label-freetype.so` in `/usr/lib/plymouth`, along
  with the `drm`, `frame-buffer` and `x11` renderers. `Xwayland`,
  `sddm-greeter` and `sddm-greeter-qt6` are all in `/usr/bin`. This is what
  the live preview in omaboot relies on.
- Verified 18 September 2026 on the reference machine (Omarchy `4.0.0.alpha`):
  `/etc/sddm.conf.d` holds `10-theme.conf` (`Current=omarchy`),
  `10-wayland.conf`, `99-omarchy-login.conf` (`Current=omarchy` plus
  `[Users]`), `autologin.conf`, and, from the on-screen keyboard login theme,
  `99-z-omarchy-onscreen-keyboard.conf` (`Current=omarchy-onscreen-keyboard`).
  SDDM reads `/etc/sddm.conf` and then these in file-name order, last
  `[Theme] Current=` wins, so the login screen on that machine is
  `omarchy-onscreen-keyboard`, not `omarchy`, and `plymouthd.conf` says
  `Theme=omarchy` with a logo identical to the packaged default. This is why
  omaboot's drop-in is `zz-omaboot.conf`, why the verify step resolves the
  theme the way SDDM does, and why the interface reports the file that
  decided the login theme rather than assuming Omarchy's.
- Verified 18 September 2026: the installed `/usr/share/sddm/themes/omarchy/
  theme.conf` is an empty `[General]` section; the theme's colours live in
  `Main.qml` (`color: "#1a1b26"` on the root rectangle) and in its recoloured
  PNGs. The on-screen keyboard theme's `theme.conf` does carry `background`,
  `foreground`, `accent` and `error` under `[General]`. omaboot reads
  `theme.conf` first and falls back to the first `color:` in `Main.qml` and
  the colour of the installed `bullet.png` when describing a login theme.
- Verified 18 September 2026: the installed Plymouth theme directory also
  carries `logos/oma.png` and `preview-unlock.png`, which `omarchy-plymouth-set`
  publishes on refresh. omaboot reads neither; it derives a theme from
  `logo.png`, `bullet.png` and the script.
- Verified 19 September 2026 in `shell/shell.qml` at upstream `e38c1d1`
  (quattro): `shell rescanPlugins` runs `reloadPlugins`, which first unloads
  every panel, clearing the open set and the pending summon payloads, and
  then rescans; a rescan asked for while one is running is queued and runs
  again after it, unloading again. `shell summon` answers "ok" once the
  request is noted, so a summon during either unload is lost silently. The
  file watcher (`Local plugin changed, reloading`) triggers the same path.
  `shell call <id> <method> <arg>` calls a method on a loaded plugin
  instance and answers "unknown" while there is none. Verified 20 September
  2026 on the reference machine: the running shell (the `core` checkout at
  `e5b0dc22`) has `call` (`shell/shell.qml:1115`) and answered `open` from
  the plugin's `ping()` with the window up. A shell without it makes
  `omaboot` fall back to trusting the summon.
- Verified 20 September 2026 in `shell/services/PluginRegistry.qml` at the
  plugin lab's Omarchy pin `b5589fa` (quattro): the local plugin watcher is
  `inotifywait -m -r -e close_write,create,delete,move` on
  `~/.config/omarchy/plugins`, and every event whose path is not hidden and
  not under `.git` emits `localPluginChanged` for that plugin id, one reload
  per event, with no coalescing. `omarchy-plugin-add` clones into a hidden
  staging directory and moves it into place in one `mv` (one event, one
  reload); `omarchy-plugin-remove` runs `rm -rf` on the plugin directory (one
  event per file and per directory). A lab conform run therefore logs
  1 + files + directories lines of `Local plugin changed, reloading: <id>`,
  the plugin directory itself counted: 19 for omaboot, 16 files in
  `plugin/` and `plugin/harness`, whatever the plugin does. On the host the
  same watcher fires for anything written into the linked `plugin/`
  directory, which is why the harness works in `.harness/` at the
  repository root.
- Quattro plugins are QML entry points (`bar`, `panel`, `overlay`, `menu`,
  `service`) loaded by the shell from `~/.config/omarchy/plugins/<id>/`. Boot and
  login happen before the shell exists, so **omaboot is not a shell plugin** and
  should not pretend to be one.

## Proposed upstream

Not sent. The reload watcher in `PluginRegistry.qml` could coalesce the
events of one plugin id into a single reload a few hundred milliseconds after
the last one, and skip the reload altogether when the plugin's directory no
longer exists at that moment, since a removal is already handled by the
listing. Both together turn the 1 + files + directories lines above into two
(install, remove) and make "one reload per change" true for local plugins
edited by hand, which is what the watcher is for. The change is upstream's to
make; nothing in omaboot can affect the count.

## The risk

DHH already built the colour-and-logo path, including the SDDM half, and
[#2455](https://github.com/omacom/omarchy/discussions/2455) shows demand for more.
He may extend it. If he does, a competing full implementation loses by default.

## The strategy

**1. Never compete on the part upstream owns.** omaboot does not reimplement
"set two colours and a logo". If a theme only changes colours and the logo, offer
to delegate to `omarchy plymouth set` and say so in the UI. That is one line of
trust-building copy: *this theme needs nothing custom, so omaboot will use
Omarchy's own command.*

**2. Own a different axis: design, not configuration.** Upstream's job is that a
stock install looks coherent. omaboot's job is that a person can design something
upstream would never ship: their own layout, their own shutdown message, their own
greeter. A distribution does not want to own a theme editor.

**3. Coexist physically.** Because omaboot writes only to its own paths, an
upstream change can never conflict with an omaboot install, and an omaboot user
can return to stock at any moment. If upstream extends its command, omaboot users
lose nothing.

**4. Contribute the parts that belong upstream.** The real preview via
`plymouthd`, and the greeter smoke test before activation, are improvements
Omarchy itself would benefit from. Offering them in #2455 costs little and buys
standing in the community, which is worth more than sole ownership of a feature.

**5. What stays defensible even in the worst case.** If upstream ships full
theming tomorrow, these remain omaboot's: the TUI itself, live preview of all
three screens, the pre-activation greeter test, export and import of themes
between users, and the gallery. Those are product, not configuration.

## Prior art, named

| Project | What it is | Covers SDDM |
|---------|-----------|-------------|
| [omarchy-plymouth-nier](https://github.com/Willi005/omarchy-plymouth-nier) | A full theme package: Python generators, installer, Limine theming, and two pacman hooks to reclaim the config after `omarchy update` | No |
| [Thinkpad-boot-screen](https://github.com/Yilmaz41/Thinkpad-boot-screen) | A single Plymouth theme following Omarchy's Style | No |
| [plymouth-theme-omarchy-mac](https://github.com/iamdanielh/plymouth-theme-omarchy-mac) | A single Plymouth theme | No |
| [omarchy-lock-explorer](https://github.com/SirJul1337/omarchy-lock-explorer) | Picker for hyprlock designs in a running session, a different screen entirely | n/a |

nier is the closest thing to a competitor and it is worth reading its hooks
before writing ours, even though the ownership rule in ARCHITECTURE.md means
omaboot should not need reclaim hooks at all. If that turns out to be wrong,
nier's `85-claim` / `99-rebuild` hook pair is the proven fallback.
