# Closing round, 20 September 2026

The round after the fourth real apply (`apply-round-2026-09-20.md`), at
HEAD `17d6ae9`: the window after the engine's Lock Screen Explorer change,
the greeter smoke test against the installed SDDM, the LUKS VM round that
M1 and three lines of the definition of done ask for, the package, and the
leftovers. Pictures and logs in `docs/evidence/closing-round-2026-09-20/`.
Nothing on the reference machine was applied, rebuilt or set; every
privileged step ran in the disposable guest.

Environment: reference machine on the `core` checkout at `e5b0dc22`
(`/usr/share/omarchy` is the 4.0.3 tree), `sddm 0.21.0-7`, Qt 6.11.2,
PySide6 6.11.2 in a venv first on `PATH`; guest: the omarchy-iso lab's
`omarchy-4.0.3` base image (installed, unencrypted, `sddm 0.21.0-7`,
`plymouth 26.134.222-2`, `limine-mkinitcpio-hook 1.38.0`, glibc 2.44 as
on the host).

## Pass/fail

| # | What | Result |
|---|------|--------|
| 1 | qmllint on `plugin/*.qml` | pass |
| 1 | `shots.sh` with the override shot (12 pictures) | pass, after the fix below (W1) |
| 1 | `exercise.sh` (12 flows, real config untouched) | pass |
| 1 | `theme-follow.sh` (`#121212` to `#eff1f5`) | pass |
| 2 | Greeter, broken theme, installed SDDM | fail as first written, fixed (B1) |
| 3 | Guest: package, plugin, window in the real shell | pass |
| 3 | Guest: apply, reboot, unlock, login, shutdown seen | pass |
| 3 | Guest: reset, reboot, indistinguishable from stock | pass (UKI byte for byte the stock one) |
| 3 | Guest: `kill -9` during the initramfs step, reboot, revert | pass (boots; revert restores stock) |
| 4 | `makepkg -f` on `packaging/PKGBUILD` | pass from a local clone; `namcap` not installed |
| 5 | `shots.sh` success path, nine pictures plus three | pass |
| 5 | `omaboot app` follows a theme switch | pass |
| 5 | The F6 ten-second path | not provoked; the two-second resummon was, and opened the window |

Definition of done (`docs/SPEC.md`): line 1 (own unlock, greeter and
shutdown screen after a reboot) proven in the guest and on the reference
machine; line 2 (reset then reboot indistinguishable from never having
installed) proven in the guest; line 4 (a kill during apply leaves a
bootable system) proven in the guest for the initramfs step; line 3
(`omarchy update` changes nothing) is read, not run
(`docs/UPSTREAM.md`, `omarchy-update` at `e5b0dc22` does not touch the
boot screen).

## 1. The window after the engine change

- What: `qmllint plugin/*.qml`; `shots.sh` with a new block that writes
  `follow` into the fake home's `.local/state/omarchy/lock-explorer-boot`,
  renders the system entry stacked (1100x760) and in columns (1600x1000)
  and a dry run of `matte`, and removes the file on the way out;
  `exercise.sh`; `theme-follow.sh`.
- Saw: the unlock facts carry `overridden by: Lock Screen Explorer
  (io.github.sirjul1337.lock-explorer), boot screen set to follow ...`,
  the footer carries the warning with the way out, and the dry run
  reports the override in red on the validate step and goes on
  (`window-override-columns.png`, `window-override-dryrun.png`).

### W1. In the stacked layout the override fact stood as a 300 px column and pushed the status line off the window

- What: the first render of the override, stacked at 1100x760.
- Saw: the fact, one sentence with the state file's path inside it,
  wrapped over nine lines in a narrow column; the inspector grew past its
  share and the status line under the footer was cut at the window's
  bottom edge (`window-override-stacked-before.png`). The columns layout
  read fine.
- Expected: the fact readable, nothing cut (`docs/UI.md`: a path keeps its
  start and its file name, nothing wraps mid-word, text is never cut).
- Cause: `Facts.qml` gave every fact `Layout.maximumWidth: 300` when the
  grid had more than one column, which is right for a value and wrong for
  a sentence; and the engine put the path inside the sentence, so the
  path treatment (elide in the middle, full path in the tooltip) could not
  apply.
- Fixed: commit "window: the override is a fact that spans the row and a
  path of its own". The engine gives the path its own fact, `its state
  file`; a fact whose value is a sentence spans the whole row in the
  stacked layout (`window-override-stacked.png`). The rule is in
  `docs/UI.md`.

## 2. The greeter against the installed SDDM

### B1. The smoke test passed a theme with a syntax error: SDDM 0.21.0 says nothing on stderr

- What: `pacman -Q sddm` (0.21.0-7); the staged `matte` theme copied twice
  (`~/.local/state/omaboot/stage/sddm`, left by `apply matte --dry-run`),
  line 12 of one copy's `Main.qml` broken with `{{ broken on purpose`;
  `QT_QPA_PLATFORM=offscreen timeout 15 sddm-greeter-qt6 --test-mode
  --theme <copy>; echo $?` on both, stderr captured; then the engine's own
  step, alone, on the real stage: `omaboot apply matte --step smoke-test`,
  which runs nothing privileged.
- Saw: exit 124 on both (the greeter never exits, as read), and an empty
  stderr on both (`greeter/broken.stderr`, `greeter/healthy.stderr`, 0
  bytes). The greeter's words were in the user journal
  (`greeter/broken.journal`): the QML error positioned in the file,
  `Fallback to embedded theme`, then the greeter staying up on
  `qrc:/theme/Main.qml`; `_TRANSPORT=journal`, fields `QT_CATEGORY`,
  `CODE_FILE`, `PRIORITY=4`, no `(WW)` anywhere. The engine at `e652e7b`
  ticked the smoke test on the broken stage and printed "Reboot to see
  it" (`greeter/smoke-old-broken.txt`).
- Expected: the broken copy refused with the greeter's lines, the healthy
  one passed.
- Cause: at this version the greeter logs through Qt's default message
  handler, not SDDM's `(II)`/`(WW)`/`(EE)` handler, and Qt sends that to
  journald when stderr is not a console. Under a pty (`script`) or with
  `QT_FORCE_STDERR_LOGGING=1` every line is on stderr
  (`greeter/broken.forced.stderr`). The scan in `greeter.rs` was written
  from the `(WW)`-tagged shape and read an empty pipe.
- Reproduce: the two commands above, then `journalctl --user
  _COMM=sddm-greeter-qt6`.
- Fixed: commit "apply: the greeter is made to say on stderr what it says
  to the journal, and a broken theme is refused (B1)". `greeter::test_mode`
  sets `QT_FORCE_STDERR_LOGGING=1` (so the preview reads it too),
  `complaints()` reads `file://<file under the stage>:<line>:<column>:`
  and the fallback line, keeping the old tags. On the broken stage the
  engine now refuses at step 4 with the two QML lines and the fallback
  (`greeter/smoke-new-broken.txt`); on the restored stage it passes
  (`greeter/smoke-new-healthy.txt`). `docs/UPSTREAM.md`'s entry is
  verified with the version.

### B2. A partial run (`--step smoke-test`) ends with "Reboot to see it"

- What: `omaboot apply matte --step smoke-test` on the healthy stage.
- Saw: the closing lines `Reboot to see it. omaboot revert puts everything
  back.` and the rescue commands, after a run that installed nothing
  (`greeter/smoke-new-healthy.txt`).
- Expected: a closing line that says what ran, or nothing.
- Cause: the closing message is printed on any performed apply, whatever
  `--step` selected.
- Reproduce: the command above.
- Fixed after the round (see "After the round" below): `cli::closing_lines`
  prints the reboot line only when the switch step ran, and otherwise
  "Only <steps> ran; nothing was switched, what boots is unchanged".

## 3. The LUKS VM round

Tooling: `scripts/vm-round.sh`, written for this round on the shape of
`bin/omarchy-iso-test` in the omarchy-iso repository (a qcow2 overlay over
the lab's `test-runs/omarchy-4.0.3/base.qcow2`, QMP `screendump` and
`send-key`, SSH with the lab's key) and of Lock Screen Explorer's
`extras/boot-vm-test.sh` (the guest's own kernel and initramfs, taken out
of its UKI, booted against a throwaway LUKS2 disk with `cryptdevice=`, a
16 MB root with a do-nothing init so the boot does not fall into an
emergency shell after the unlock). Every command and its output is in
`vm/commands.log`; the pictures are the guest's 1280x800 screen.

What the tooling could not do, said exactly: the lab has no encrypted base
image (only `omarchy-4.0.3`, installed unencrypted), so the guest's own
boot shows no passphrase prompt. The unlock screen is therefore proven
with the guest's real kernel and the initramfs the apply rebuilt, booted
against a real LUKS volume, but not with the guest's root on that volume.
Everything else (apply, reset, kill, revert, the login and shutdown
screens, the boot splash) is the guest itself. The `omarchy update` line
of the definition of done was not run: the guest's package mirror is the
ISO's and there is no newer release to update to.

### V1. The lab's guest never draws a Plymouth theme: a serial port makes Limine add a serial console and Plymouth forces details

- What: the first applied reboot, pictures one per second, then sampled at
  five per second.
- Saw: at shutdown and at boot the screen showed systemd's `[ OK ]` lines
  (Plymouth's details splash), never a theme, omaboot's or Omarchy's. In
  the LUKS boot with `plymouth.debug` and a serial port present, on tty1:
  `serial consoles detected, managing them with details forced`
  (`vm/plymouth-serial-console-forces-details.png`).
- Expected: the theme `plymouthd.conf` names.
- Cause: the lab boots the guest with `-serial file:...` for its logs;
  with a UART present, Limine appends `console=uart,io,0x3f8 console=tty0`
  to the kernel command line (it is in `/proc/cmdline` and in none of the
  guest's configuration files), and Plymouth, seeing a serial console in
  `/sys/class/tty/console/active`, forces the details splash. Nothing to
  do with omaboot; the reference machine has no serial console.
- Reproduce: boot the lab's base with `-serial file:x`, `cat /proc/cmdline`.
- Worked around: `scripts/vm-round.sh` boots with `-serial none`; then the
  command line is the one Limine's config says and the themes draw. Every
  picture below is from that.

### The run

1. Install: `pacman -U` of the package from item 4, `omaboot-apply
   protocol` answers 2, the `matte` theme copied into
   `~/.config/omaboot/themes`, `omaboot plugin install` from an SSH session
   with the login session's environment, `omaboot` opens the window in the
   guest's real shell (`vm/01-window-in-the-guest-shell.png`). Stock
   login: `vm/00-stock-login.png`.
2. `omaboot --password-stdin apply matte`: ten steps ticked
   (`vm/apply-first.txt`), `status` reports `applied: matte`, drift none,
   18 files match; the UKI hash changed (`vm/uki-stock.sha256`,
   `vm/uki-first.sha256`). Reboot: the shutdown screen, held twelve
   seconds by a transient unit whose stop sleeps
   (`vm/02-applied-shutdown.png`), the boot splash
   (`vm/03-applied-boot.png`), the login screen
   (`vm/07-applied-login.png`, `vm/08-applied-login-typed.png`). The UKI
   the apply rebuilt, booted against the LUKS disk: the unlock prompt
   (`vm/04-applied-unlock-prompt.png`), four typed characters as bullets
   (`vm/05-applied-unlock-typed.png`), the progress bar after the
   passphrase (`vm/06-applied-unlocked.png`). Autologin was already off
   in the base image (no `autologin.conf`; the greeter is what the lab
   types the password into).
3. `omaboot --password-stdin reset` (`vm/reset.txt`): `plymouthd.conf`
   says `Theme=omarchy`, `/etc/sddm.conf.d` holds the three stock files,
   no `/usr/share/plymouth/themes/omaboot`, no
   `/usr/share/sddm/themes/omaboot`, no `~/.local/state/omaboot`; the
   Omarchy theme directories hash as before; the initramfs carries the
   `omarchy` theme and `Theme=omarchy`; and the UKI is byte for byte the
   stock one, `39cb4675...` (`vm/fingerprint-after-reset.txt`,
   `vm/uki-after-reset.sha256` against `vm/uki-stock.sha256`). Reboot:
   `vm/10-reset-shutdown.png`, `vm/11-reset-boot.png`,
   `vm/12-reset-unlock-prompt.png` (the stock initramfs against the LUKS
   disk), `vm/13-reset-login.png`.
4. Apply again, with a watcher in the guest reading the `--json` stream and
   `kill -9` on the engine the moment `initramfs` reports `started`
   (`vm/apply-killed-steps.txt`, `vm/after-kill.txt`): the kill landed
   while `limine-mkinitcpio` and `mkinitcpio --uki` were running under
   `sudo`; a second later nothing of them was left and the UKI on disk was
   still the stock one (`vm/uki-after-kill.txt`), so the rebuild was
   interrupted before its atomic move. State on disk: `plymouthd.conf`
   says `omaboot`, the drop-in is written, the theme directories are
   installed, `rollback.toml` is recorded, `applied.toml` is not. Reboot:
   the guest boots (`vm/21-killed-boot.png`, the stock splash from the
   stock initramfs; `vm/20-killed-shutdown.png` and `vm/22-killed-login.png`
   are omaboot's, from the root's own configuration), `uptime` 0 min,
   SSH and the session back. `omaboot --password-stdin revert`
   (`vm/revert.txt`): Plymouth back to `omarchy`, drop-in removed,
   initramfs rebuilt to the stock hash, the state files removed; the
   theme directories stay, as `docs/DECISIONS.md` says a revert leaves
   them (`vm/fingerprint-after-revert.txt`). Reboot:
   `vm/30-reverted-shutdown.png`, `vm/31-reverted-boot.png`,
   `vm/32-reverted-login.png`.

### V2. `plugin install` says "ran omarchy-shell shell rescanPlugins" when the call failed

- What: `omaboot plugin install` over SSH, without `OMARCHY_PATH` in the
  environment.
- Saw: `ran        omarchy-shell shell rescanPlugins` and the same for
  `setPluginEnabled`, exit 0; `omaboot` then said the shell did not open
  the plugin, `it said: OMARCHY_PATH is not set`. Run by hand,
  `omarchy-shell shell rescanPlugins` exits 1 with that sentence on stderr.
  With `OMARCHY_PATH=/usr/share/omarchy` the second call answers `(ok)`.
- Expected: a failed IPC call reported as failed, with what the shell said.
- Cause: `plugin::shell` prints "ran" with stdout and ignores the exit
  status and stderr.
- Fixed after the round: `plugin::shell` prints `failed     omarchy-shell
  ... (exit code N): <last stderr line>` and `plugin install` ends with an
  error saying the links are in place and the shell did not take the call.

### V3. After an interrupted apply, `status` says "nothing is applied" and nothing about the interruption

- What: `omaboot status` right after the kill (`vm/after-kill.txt`).
- Saw: "what boots now" names omaboot's theme and drop-in as installed by
  omaboot, then `nothing is applied by omaboot; the system is on its own
  themes` and `rollback target: plymouth omarchy, drop-in removed (just
  now)`, with no warning.
- Expected: a sentence that an apply was interrupted after the switch and
  that revert puts the rollback point back, since that is exactly the
  state.
- Cause: `applied.toml` is written by the verify step, and the warnings
  are keyed on the applied record; a rollback record without an applied
  record is not read as anything.
- Fixed after the round: the snapshot reads the rollback point, and a
  rollback point without an applied record is a warning in `status` and
  the window: "an apply of <theme> was interrupted after the switch
  (<age>) ... omaboot revert puts plymouth <previous> back, or apply again
  to finish".

### V4. `status` reports the omaboot Plymouth theme's background as unknown

- What: `omaboot status` while `matte` (background `#000000`) is applied.
- Saw: `background  unknown` under boot and shutdown, `text #bebebe`,
  while the login screen's background reads `#000000`.
- Expected: `#000000`.
- Cause: the system module reads the background from
  `Window.SetBackgroundTopColor` in the installed script; the generated
  `omaboot.script` states it differently (`global.background_red = 0.102;`
  and the names passed to the call).
- Fixed after the round: `parse_background` resolves a name to the last
  float the script assigned it.

## 4. Packaging

- What: `makepkg -f` in a scratch copy of `packaging/PKGBUILD` with
  `source=("git+file:///home/mtolhuijs/Projects/omarchy/omaboot")`, since
  the GitHub repository does not exist yet; build only, then `tar -tvf` on
  the result. `namcap` is not installed here.
- Saw: prepare, build, check (the whole suite, release profile, with
  `OMARCHY_PATH` set in the session) and package ran through;
  `omaboot-git 0.1.0.r24.gf7b0025-1` with the two binaries, the plugin
  without its harness, six docs and the 128 px icon
  (`packaging/contents.txt`, `packaging/PKGINFO`, `packaging/makepkg.log`).
  The package installed and ran in the guest (item 3).
- Two things for the AUR, not failures of the build: the repository has no
  `LICENSE` file, so nothing lands under `/usr/share/licenses/omaboot-git`
  as the MIT licence requires there; and `strip = true` in the release
  profile leaves makepkg's own strip and its debug package nothing to do
  (`gdb-add-index: No index was created` in the log). Both done after the
  round: `LICENSE` (MIT, 2026, Maarten Tolhuijs) is in the repository and
  the PKGBUILD installs it under `/usr/share/licenses/$pkgname`, and
  `options=('!debug')` says why there is no debug package. Not rebuilt with
  makepkg since; the change is two lines of `package()` and one option.

## 5. Leftovers

- `shots.sh` success path: twelve pictures written (the nine of the design
  pass and the three override pictures), exit 0.
- `omaboot app` still opens and follows a theme switch: `theme-follow.sh`
  reports `#121212 -> #eff1f5`.
- The F6 ten-second path: not provoked, in the guest, twice.
  `omarchy-restart-shell; omaboot` opened the window in 1.2 s (the
  restart script waits for the shell's ping before returning,
  `vm/40-f6-after-restart-shell.txt`). Killing the shell with `quickshell
  kill`, launching it through `hyprctl dispatch` and running `omaboot` at
  once: the first summon hit the shell mid-load (`WARN qml: summon:
  unknown plugin mtolhuys.omaboot` in its journal), the resummon after two
  seconds opened the window, 2.2 s in all, exit 0
  (`vm/41-f6-shell-mid-load.txt`, `vm/42-f6-window-after-shell-mid-load.png`).
  The ten-second give-up needs a shell that stays unanswering for ten
  seconds, which this guest's does not. Not attempted on the reference
  machine, where it would restart the owner's desktop.

## After the round

The four findings recorded above as not fixed (B2, V2, V3, V4) and the two
packaging notes were fixed on 20 September 2026 in one commit, each with a
test: `cli::tests::a_partial_apply_does_not_promise_a_new_boot_screen`,
`plugin::tests::a_shell_call_that_fails_is_reported_as_failed_with_what_the_shell_said`
(a scripted `omarchy-shell` in a prefix), `system::tests::a_rollback_point_without_an_applied_record_is_an_interrupted_apply`,
and the generated-script case in `system::tests` for `parse_background`.
V1 is the lab's, not omaboot's, and stays as recorded. Not re-run since:
the harness and qmllint (no QML changed), makepkg (see above).

Also after the round, from the window on the reference machine: "New
theme" from the Omarchy theme `catppuccin-dark` (a store theme) answered
`the Omarchy theme catppuccin-dark has no unlock.png. Suggested next step:
pick a theme that has one, or scaffold without --from-omarchy-theme and add
your own logo.png` in the dialog. Fixed the same day: such a theme gets
Omarchy's default logo and a note saying so; `status` marks each Omarchy
theme with `has_logo`. The dialog does not yet show that mark (QML
unchanged; a window pass is owed).

Also after the round: the launcher no longer listed omaboot. On the
machine, `~/.local/share/applications/omaboot.desktop` and the seven
`hicolor/<size>/apps/omaboot.png` icons were gone (`mimeinfo.cache`
rewritten 17:41, during the round). `theme-follow.sh` ran `omaboot app
install` with `HOME` and `XDG_CONFIG_HOME` pointed at the fake home but
without `XDG_DATA_HOME`, which a login session sets to the real
`~/.local/share`, so the harness's entry and icons were the real ones, and
whatever ran `app uninstall` afterwards took the real ones away. Put back
by hand (the same files `app install` writes); the wrapper `render.py`
writes and `theme-follow.sh` now export `XDG_DATA_HOME` into the fake home,
and `exercise.sh`'s fingerprint covers the real desktop entry and icons.
Not re-run on the machine since (the harness needs it); the change is
three environment lines and a stat loop.
