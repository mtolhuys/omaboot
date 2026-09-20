# Test round, 19 September 2026

A full round over the engine, the harness, the real shell on this machine and
the Omakit lab. Nothing was applied, reverted or reset on this machine; every
system-touching path was exercised with `--dry-run` or inside a prefix.
Evidence for every row is in `docs/evidence/test-round-2026-09-19/`.

During the round the repository received its first commit (`2acffaa`), because
the Omakit lab resolves its subject from `HEAD`. The `Claude outputs/` folder
was left untracked.

## 1. Environment

| Item | Value |
|------|-------|
| Omarchy | `4.0.0.alpha` (`/usr/share/omarchy/version`), quattro branch |
| Running shell | `quickshell -n -p ~/Projects/omarchy/core/shell` started by `omarchy-launch-shell`; `core` checkout at `e5b0dc22`, branch `display-setup-foundation`. The `omarchy-shell` systemd user unit is inactive, so the shell's log is `journalctl --user` without a unit filter. |
| quickshell | 0.3.1 (Arch) |
| rustc / cargo | 1.89.0 / 1.89.0 |
| Display | eDP-1, 2560x1600, scale 1.6 |
| Shell env | `OMARCHY_PATH=/home/mtolhuijs/Projects/omarchy/core`, `XDG_CONFIG_HOME=~/.config`, `XDG_STATE_HOME=~/.local/state` (all set by the login session) |
| OMARCHY_PATH used for the harness | `/usr/share/omarchy` |
| PySide6 | 6.11.2, installed into a scratch venv (the system python has none) |
| Fontconfig monospace | JetBrainsMono Nerd Font |
| Tools found | `zenity`, `sddm-greeter`, `sddm-greeter-qt6`, `limine-mkinitcpio`, `/usr/local/bin/mkinitcpio`, `plymouthd`, `grim`, `wtype` |
| Desktop theme during the round | Catppuccin Dark (a user theme in `~/.config/omarchy/themes`; `~/.local/state/omarchy/current/theme` is a directory, not a link) |
| Omakit lab | `OMAKIT_PLUGIN_LAB` was unset; run with `~/Projects/omarchy/plugin-lab`, guest base image omarchy-4.0.3 |

## 2. Checks

| Round | Check | Result | Exit | Output |
|-------|-------|--------|------|--------|
| 1 | `cargo fmt --all -- --check` | pass | 0 | no output |
| 1 | `cargo clippy --workspace --all-targets` | pass | 0 | `logs/cargo-clippy.txt` |
| 1 | `cargo test --workspace` as the shell is configured | **fail**, 219 passed, 12 failed (lib); 21 passed (apply) | 101 | `logs/cargo-test-with-OMARCHY_PATH-set.txt`, finding F1 |
| 1 | `env -u OMARCHY_PATH cargo test --workspace` | pass, 231 + 0 + 21 + 0 doc | 0 | `logs/cargo-test-OMARCHY_PATH-unset.txt` |
| 1 | `qmllint plugin/*.qml` (8 files) | pass, no output at all | 0 each | `logs/qmllint.txt` (empty) |
| 2 | `cargo build --release` | pass | 0 | |
| 2 | `plugin/harness/shots.sh` as documented | **fail**, not executable | 126 | finding F4 |
| 2 | `bash plugin/harness/shots.sh /tmp/omaboot-shots` with the login env | ran, but the six theme views are error states and the run took 4 min 14 s | 0 | `logs/shots-run1.txt`, `harness-shots/run1-stacked-theme-broken-seed.png`, finding F2 |
| 2 | same with `XDG_CONFIG_HOME` and `XDG_STATE_HOME` unset | pass, 9 pictures, 15 s | 0 | `logs/shots-run2.txt`, `harness-shots/*.png` |
| 2 | `plugin/harness/exercise.sh /tmp/omaboot-exercise` (same env) | pass, "all flows passed" | 0 | `logs/exercise.txt`, `harness-exercise/*.png` |
| 2 | `plugin/harness/theme-follow.sh /tmp/omaboot-follow` (same env) | pass, `#121212 -> #eff1f5` | 0 | `logs/theme-follow.txt`, `harness-theme-follow/after.png` |
| 3 | `omaboot plugin install` | pass (links kept, rescan + enable ran) | 0 | |
| 3 | `omaboot` right after `plugin install` | **fail**: exit 0, no window | 0 | finding F6 |
| 3 | `omarchy-shell shell summon mtolhuys.omaboot '{}'`, and `omaboot` once the shell had settled | pass: window `org.quickshell` / `omaboot`, 795x970, tiled | 0 | `shell/plugin-window-in-shell.png` |
| 3 | Window: header mark, sidebar, 16:9 picture, inspector | pass, in the stacked layout (the tiled window is 795 px wide) | | same picture |
| 3 | QML warnings from omaboot in the shell log | pass: none; only two `Local plugin changed, reloading` lines | | |
| 3 | `omaboot app install` | pass: desktop entry, 7 icon sizes, app dir | 0 | |
| 3 | `omaboot app`: own window class | pass: class `omaboot`, title `omaboot`, own pid | | `logs/app-run.txt` |
| 3 | `omaboot app`: "omaboot" in the launcher | not checked by hand; the desktop entry is `Name=omaboot`, `Icon=omaboot`, `StartupWMClass=omaboot` | | |
| 3 | `omaboot app`: recolours on a theme switch | partial: `omarchy-theme-set tokyo-night` ran and the window was in Tokyo Night colours six seconds later; the before picture did not include the window, so the change itself was not captured | 0 | `shell/app-window-tokyo-night.png`, note U7 |
| 3 | `omaboot app`: close and the process exits | needs a human: the window was closed from the desktop, not by this round; afterwards no `omaboot app` process was left | | |
| 3 | In-window edits by mouse (colour, widths, position, toggle, drop, Logo button, rename, dry run, delete) | needs a human: no pointer automation on this machine; the same flows were run through the engine (next rows) and through the harness (round 2) | | |
| 3 | `omaboot new roundtest --from-current` | pass, complete manifest with logo | 0 | `logs/engine-edit-flows.txt` |
| 3 | `set colors.background`, `logo.width`, `logo.shutdown.width`, `logo.position`, `login.clock` + `login.show_session_picker` | pass, each diff touched only its own key | 0 | |
| 3 | `add-image ~/Pictures/omarchy-logo-solitude.png --as logo` | pass, file copied, `source` updated, nothing else | 0 | |
| 3 | `rename roundtest "Round Test"` | pass, only `name` changed | 0 | |
| 3 | `apply roundtest --dry-run` on the real layout | pass, 10 steps, 97 lines, no `!` lines, `theme.toml` unchanged | 0 | `logs/dry-run-real-machine.txt` |
| 3 | `delete roundtest` | pass, only that directory removed | 0 | |
| 3 | `omaboot status --json \| python3 -m json.tool` | pass: valid, `applied` null, `login.by_omaboot` false, no warnings | 0 | `logs/status.json` |
| 3 | `omaboot login stock` / `login release` | untested by policy | | |
| 3 | `omaboot apply`, `revert`, `reset` for real | untested by policy | | |
| 4 | `omakit lab conform ~/Projects/omarchy/omaboot#plugin` | **fail** on one check (`lab-log-gate`), every lifecycle check passed | 1 | `logs/omakit-lab.txt`, `omakit-lab/`, finding F7 |

Omakit lab, per check (from `omakit-lab/checks.jsonl`):

| Check | Status | Note |
|-------|--------|------|
| lab-validate | pass | pinned omarchy-plugin-validate accepts the manifest (kinds: panel) |
| log-baseline | pass | shell restarted without the subject |
| lab-install | pass | omarchy-plugin-add --enable from the local repository at the subject commit |
| lab-first-load.panel | unsupported | summon produced no failure line and no new layer; the suite cannot see a panel that shows nothing on open |
| lab-bar-render, lab-click, lab-escape, lab-popout-switch | not applicable | kinds: panel |
| theme-matte-black, theme-catppuccin-latte, theme-tokyo-night, lab-themes | pass | stills in dark and light |
| lab-disable | pass | enabled bit cleared, files kept |
| lab-reenable | pass | |
| lab-restart.loaded | pass | enabled again after a full shell restart |
| lab-restart.no-failure | pass | no load-failure line |
| lab-reload | unsupported | same-path reload does not replace the runtime at Omarchy pin b5589fa |
| lab-remove | pass | files, layout entry and listing gone |
| lab-no-leak.removal | pass | no subject process survives |
| lab-log-gate | **fail** | 1 unexplained line: `WARN: Process failed to start, likely because the binary could not be found. Command: QList("omaboot", "status", "--json")` |

Evidence bundle: `/home/mtolhuijs/Projects/omarchy/omarchy-iso/test-runs/omarchy-4.0.3/runs/20260919-230551/omakit-conform-home-mtolhuijs-projects-omarchy-omaboot-plugin/evidence.json`
(copied to `omakit-lab/`). Guest stills: `conform-00-before-install.png`,
`conform-theme-matte-black-bar.png`, `conform-theme-catppuccin-latte-bar.png`,
`failure-acceptance-final.png` in the run directory. None of them shows an
omaboot window.

## 3. Findings, by severity

### F1. The test suite and the `--root` sandbox are not hermetic against `OMARCHY_PATH`

- What: `cargo test --workspace` in a normal Omarchy login shell.
- Saw: 12 failures in `omarchy::tests`, `scaffold::tests`, `cli::tests` and
  `system::tests`. They read the real tree: "there is no Omarchy theme called
  odd. Suggested next step: the themes on this system are: catppuccin, ...",
  and the scaffold test compared a real `unlock.png` against the fixture's bytes.
  Two `system::tests` saw `styled_by = "unknown"` instead of `"default"`.
- Expected: 231 passing whatever the caller's environment.
- Cause: `omarchy_path()` in `crates/omaboot/src/omarchy.rs:25` returns
  `$OMARCHY_PATH` as is when set, and only applies `layout.system(...)` when
  it is unset. Omarchy sets it for every session, and `omarchy dev link` points
  it at a checkout. The same code path means a `--root <prefix>` run reads the
  host's Omarchy tree, not the prefix's.
- Reproduce: `OMARCHY_PATH=/home/mtolhuijs/Projects/omarchy/core cargo test -p omaboot --lib`
  against `env -u OMARCHY_PATH cargo test -p omaboot --lib`.
- Log: `logs/cargo-test-with-OMARCHY_PATH-set.txt`.

### F2. The harness wrapper leaks into the real `~/.config/omaboot` when `XDG_CONFIG_HOME` is set

- What: `bash plugin/harness/shots.sh /tmp/omaboot-shots` in a normal login
  shell.
- Saw: the engine, run through the harness's wrapper script, reported
  `themes_dir = /home/mtolhuijs/.config/omaboot/themes` (the real one), found
  the real `matte` theme there, so `seed_theme` skipped seeding; the window's
  own engine calls, which do set `XDG_CONFIG_HOME` to the fake home, then
  found no theme and every theme view showed "matte is not a valid theme:
  there is no theme.toml in this directory". The real
  `~/.config/omaboot/themes/matte/theme.toml` is named "Maarten I", the
  harness's seed title, so an earlier harness run has already created or
  renamed a theme in the real home.
- Expected: the harness never reads or writes outside `plugin/harness/.work`.
- Cause: `Layout::discover` in `crates/omaboot/src/paths.rs:69` prefers
  `XDG_CONFIG_HOME` and `XDG_STATE_HOME` over `HOME`; the wrapper written by
  `build_world` in `plugin/harness/render.py:421` exports only `HOME`,
  `XDG_RUNTIME_DIR` and `OMARCHY_PATH`. `theme-follow.sh` sets
  `XDG_CONFIG_HOME` for its own `app install` call, which shows the gap was
  known once.
- Reproduce: `plugin/harness/.work/home/.local/bin/omaboot status --json | jq .themes_dir`
  with `XDG_CONFIG_HOME` set.
- Workaround used for this round: `env -u XDG_CONFIG_HOME -u XDG_STATE_HOME`.
- Log: `logs/shots-run1.txt`, picture `harness-shots/run1-stacked-theme-broken-seed.png`.

### F3. The file dialog does not open in `~/Pictures`

- What: read `chooseImage` in `plugin/Omaboot.qml:275`.
- Saw: the zenity command carries `--file-selection`, a title and two filters,
  and no `--filename=`. So the dialog opens wherever zenity defaults to, and
  never "where the last image came from".
- Expected: README and `docs/UI.md` both say the dialog opens in `~/Pictures`
  the first time and afterwards in the directory of the last image.
- Reproduce: click Logo in the window, or read the lines above.
- Not exercised by mouse in this round.

### F4. The harness scripts and `scripts/verify-render.sh` are not executable

- What: `plugin/harness/shots.sh /tmp/omaboot-shots` as written in CLAUDE.md
  and `docs/UI.md`.
- Saw: `bash: plugin/harness/shots.sh: Permission denied`, exit 126. Modes are
  `-rw-------` on `shots.sh`, `exercise.sh`, `render.py`, `ShellRoot.qml`,
  `FloatingWindow.qml` and `scripts/verify-render.sh`; only `theme-follow.sh`
  is `-rwx--x--x`. The modes are now in the first commit as they were.
- Expected: `chmod +x` on the three shell scripts and `render.py`.

### F5. `active_omarchy_theme` is null when `current/theme` is a directory

- What: `omaboot status --json` on this machine.
- Saw: `"active_omarchy_theme": null` while the desktop is on Catppuccin Dark.
  `~/.local/state/omarchy/current/theme` is a real directory here, with a
  `theme.name` file beside it; `api.rs:185` uses `fs::read_link` and gives up
  when it is not a link.
- Consequence: the colour popup's first row, "Your Omarchy theme", is missing
  in the real shell, and `new --from-omarchy-theme` has no default to offer.
- Expected: read the directory name, or `theme.name`, when the path is not a
  link. `docs/UPSTREAM.md` should say which layout the running Omarchy uses.
- Reproduce: `ls -la ~/.local/state/omarchy/current/` then
  `omaboot status --json | jq .active_omarchy_theme`.

### F6. `omaboot` straight after `omaboot plugin install` opens nothing

- What: `omaboot plugin install && omaboot`, as the round prescribes.
- Saw: both exit 0, no window; `hyprctl clients` listed no quickshell
  window; the shell log shows two `Local plugin changed, reloading:
  mtolhuys.omaboot` lines and nothing else. Three minutes later
  `omarchy-shell shell summon mtolhuys.omaboot '{}'` opened it, and a bare
  `omaboot` also opened it once the shell had settled.
- Expected: the window, or a sentence saying why not.
- Cause, likely: `plugin::open` rescans (the plugin is a link), then summons
  while the shell is still reloading the plugin that `plugin install` had just
  rescanned and enabled. `wait_until_listed` only checks the listing.
- Reproduce: `omaboot plugin uninstall; omaboot plugin install && omaboot`.

### F7. In the lab the plugin spawns the engine without checking it exists, and fails the log gate

- What: `omakit lab conform ~/Projects/omarchy/omaboot#plugin`.
- Saw: `lab-log-gate: 1 unexplained log lines`:
  `WARN: Process failed to start, likely because the binary could not be found. Command: QList("omaboot", "status", "--json")`.
  Every lifecycle check passed. The scenario as a whole is marked FAILED
  because of this single line.
- Expected: the engine-not-found state was expected in the guest, but a
  marketplace conformance run must be green; the plugin should check
  `omaboot` is on `PATH` before spawning (and show its engine-not-found state
  without a Quickshell warning), or the manifest should declare the engine as
  a requirement the lab understands.
- Reproduce: the command above; bundle in `omakit-lab/`.

### F8. The plugin is reloaded repeatedly by the shell's file watcher

- What: shell logs on the host and in the guest.
- Saw: on the host, two `Local plugin changed, reloading: mtolhuys.omaboot`
  per open; in the guest, about twenty such lines during the conformance run
  (`omakit-lab/shell-log-filtered.txt`). On the host, `plugin/harness/.work`
  lives inside the linked plugin directory and every harness run writes there.
- Expected: one reload per real change. Moving the harness work directory out
  of `plugin/` (it is git-ignored anyway) removes the host cause; the guest
  count needs a look.

### F9. Doc drift: `omaboot-apply` is copied, not linked

- What: `omaboot plugin install`.
- Saw: `kept /home/mtolhuijs/.local/bin/omaboot-apply (identical copy)`;
  the file is a regular file dated 18 September. README says "the two
  binaries become `~/.local/bin/omaboot` and `~/.local/bin/omaboot-apply`"
  as links. The code comment in `plugin.rs:74` says a copy is deliberate for
  the binary sudo runs. Fix the README, not the code.

### F10. README test count is stale

- README says `cargo test` runs 249 tests; the workspace has 231 + 21 = 252.

## 4. UX notes, from the pictures

- U1. Stacked layout: the caption under the picture is elided to
  "composite; drop a PNG …" and "drop a JPG or…" (`harness-shots/stacked-theme.png`,
  `stacked-login.png`). The one line the rules allow under the picture is
  the one that gets cut.
- U2. Stacked layout, system entry: the logo path wraps mid-word,
  "logo.p / ng" (`stacked-system.png`, also in the real shell).
- U3. Run view: long paths wrap at arbitrary characters ("omarch / y/omaboot")
  and the last visible operation line sits half under the fade above the
  Back button (`harness-shots/dryrun.png`).
- U4. Columns layout, system entry with no themes: the hint "No themes of
  yours yet ..." floats vertically centred in the sidebar, far below the
  New theme button (`columns-system.png`).
- U5. Every operation in the dry run is verbed "check", including the ones
  that would generate or copy; the arrow text after it says what would
  happen, so the verb column carries little.
- U6. Shutdown tab, Progress "spinner": the composite draws a bar-shaped box.
  The Plymouth script defines the spinner as a pulsing progress box, so it
  is faithful, but a first-time user reads it as a bar
  (`harness-exercise/shutdown-scale.png`).
- U7. Both the plugin window and the standalone app open tiled at 795 px on
  this laptop, which forces the stacked layout every time; the columns
  layout the preference asks for (`prefs.layout = columns`) never shows
  unless the window is floated or widened by hand.
- U8. The broken-seed state (F2) shows "drawing…" in the picture forever with
  the error only in the footer; the picture should show the sentence.
- U9. In the shell the login line reads `login: omarchy-onscreen-keyboard`
  and the system inspector offers "Use Omarchy's login screen"; correct, and
  the button's consequence (a drop-in through sudo) is only in its tooltip.

## 5. What a human must do next

Everything below touches the system and was skipped by policy.

1. Close the standalone app from its own window and confirm the process is
   gone (Escape should close it):

   ```bash
   omaboot app
   ```
   then, after closing it:
   ```bash
   pgrep -af "omaboot app" || echo "exited"
   ```

2. The in-window edits by mouse, with a file check after each one, on a
   scratch theme:

   ```bash
   omaboot new roundtest --from-current && omaboot
   ```
   then in the window: change a colour, logo width on Unlock then on
   Shutdown, position, a toggle, drop a PNG, Logo button (expect the dialog
   in `~/Pictures`; it will not be, see F3), rename, Dry run, delete; after
   each step:
   ```bash
   cat ~/.config/omaboot/themes/roundtest/theme.toml
   ```

3. The login drop-in, on a machine where the third-party SDDM theme is what
   logs in (this one):

   ```bash
   omaboot login stock --dry-run
   ```
   then for real, and back:
   ```bash
   omaboot login stock
   ```
   ```bash
   omaboot login release
   ```
   Check `/etc/sddm.conf.d` before and after each.

4. The apply on real hardware, in the QEMU VM first, never on this machine
   without a rollback plan:

   ```bash
   omaboot apply roundtest --dry-run
   ```
   ```bash
   omaboot apply roundtest
   ```
   reboot, look at unlock, login and shutdown, then:
   ```bash
   omaboot revert
   ```
   and confirm `/usr/share/plymouth/themes/omarchy` and
   `/usr/share/sddm/themes/omarchy` are byte for byte what they were.

5. Rerun the lab after F7 is fixed, with the tree clean:

   ```bash
   OMAKIT_PLUGIN_LAB=~/Projects/omarchy/plugin-lab ~/Projects/omarchy/omakit/bin/omakit lab conform ~/Projects/omarchy/omaboot#plugin
   ```
