# Apply round, 20 September 2026

The first real `Apply to the system` from the window, on the reference
machine, by the owner, theme `matte`. It stopped at step 4 and installed
nothing: steps 1 to 3 write only under `~/.local/state/omaboot/stage`, and
the smoke test is the gate before any privileged step. The gate held, but
for the wrong reason. Screenshots in `docs/evidence/apply-round-2026-09-20/`.

## Findings

### A1. The greeter smoke test cannot pass: a healthy greeter runs until the timeout, and the timeout is a failure

- What: `Apply to the system` on `matte`; step 4, "Smoke test greeter".
- Saw: "Validate theme", "Generate assets" and "Stage theme" ticked at once;
  "Smoke test greeter" spun for about thirty seconds
  (`smoke-test-running.png`), then the step failed with
  `QT_QPA_PLATFORM=offscreen /usr/bin/sddm-greeter-qt6 --test-mode --theme
  /home/mtolhuijs/.local/state/omaboot/stage/sddm exited with a timeout: (no
  output). Suggested next step: the greeter smoke test, which decides
  whether this theme is ever shown at login did not succeed; read the
  message above and fix the cause, then run the step again`
  (`smoke-test-timeout.png`). Nothing was installed, no drop-in written, no
  initramfs rebuilt.
- Expected: the step passes for a theme the greeter renders, and fails for
  one it refuses.
- Cause: `sddm-greeter --test-mode` does not exit on its own. It shows the
  theme in a window and runs until that window is closed, which under
  `QT_QPA_PLATFORM=offscreen` is never. `crates/omaboot/src/preview.rs`
  (`run_greeter`) knows this and waits "until the greeter window is closed";
  `crates/omaboot/src/apply/mod.rs` (`step_smoke_test`) runs the same
  command through `RealRunner` with `SMOKE_TEST_TIMEOUT` (30 s) and
  `docs/DECISIONS.md` says "a timeout counts as a failure". So the only
  greeter that passes the step is one that exits within 30 seconds, which is
  a greeter that failed. The step had never run for real: the tests use a
  scripted runner, and a `--root` prefix uses `SimulatedRunner`, which
  answers success without running anything. Two smaller things in the same
  path: `CommandOutput::timed_out()` drops whatever the greeter wrote, so the
  sentence says "(no output)" whatever the greeter said; and the suggestion
  tells the user to fix a cause the sentence does not name.
- Reproduce: `QT_QPA_PLATFORM=offscreen sddm-greeter-qt6 --test-mode --theme
  ~/.local/state/omaboot/stage/sddm; echo $?` in a terminal: it does not
  return. `timeout 30` around it returns 124.

- Fixed: see the commit "apply: the greeter smoke test passes a greeter that
  stays up and fails one that exits or complains (A1)". The step now gives
  the greeter `greeter::SETTLE` (10 s), requires it to be alive then, stops
  it, and fails on an early exit or a complaint about the theme on stderr,
  carrying the lines. The stderr scan is written from SDDM's behaviour as
  read in its source, not yet against the installed version: see
  `docs/UPSTREAM.md`, "Read on 20 September 2026". The next real apply is
  the healthy-path verification; the broken-theme path needs the command in
  that note once.

### A2. Step 5, "Authorise", fails with "a password is required" after the password was accepted

- What: the second real `Apply to the system` on `matte`, with the smoke
  test fixed (A1); the password dialog answered.
- Saw: steps 1 to 4 ticked (the new smoke test passed in about ten
  seconds), then "Authorise" failed: `sudo -n -v exited with exit code 1:
  sudo: a password is required. Suggested next step: one authorisation for
  the whole operation, rather than one per file did not succeed; read the
  message above and fix the cause, then run the step again`
  (`authorise-no-ticket.png`). Nothing was installed.
- Expected: the ticket taken from the password at the start of the run
  covers `sudo -n -v` and every privileged command after it.
- Cause: `auth::acquire` ran `sudo -S -k -v -p ""`. The password was
  accepted (otherwise the run would have stopped before step 1), but with
  `-v`, `-k` makes sudo ignore and not update the cached credentials
  (sudo(8), `--reset-timestamp`: "will prompt for a password ... and will
  not update the user's cached credentials"). So no ticket was ever
  recorded, and the first `sudo -n` found none. The path had never run for
  real either: under `--root` no sudo runs, and the tests script the runner.
- Reproduce: `printf '%s\n' "$password" | sudo -S -k -v -p ''; sudo -n -v`
  in a shell without a tty (`setsid` or from a Quickshell Process): the
  second command fails with "a password is required". Without `-k` it
  succeeds.
- Fixed: `TICKET_ARGS` is `-S -v -p ""`, with a test that it carries no
  `-k`; and `acquire` asks `sudo -n -v` right after, so a sudoers policy
  that keeps no ticket (`timestamp_timeout=0`) is reported at the password
  dialog with the setting to look at, instead of at step 5.

### A3. Step 10, "Verify", fails on the drop-in the helper had just written; the automatic revert put the system back

- What: the third real `Apply to the system` on `matte`, with A1 and A2
  fixed. Steps 1 to 9 ticked: smoke test, authorise, install, rollback
  point, switch, initramfs.
- Saw: `step verify failed: /etc/sddm.conf.d/zz-omaboot.conf could not be
  read: No such file or directory (os error 2) ... the recorded rollback
  point was restored, so the system is back where it was`
  (`verify-no-dropin.png`). Afterwards, on the machine: `plymouthd.conf`
  says `Theme=omarchy`, `/etc/sddm.conf.d` holds only Omarchy's and the
  on-screen keyboard's files, `~/.local/state/omaboot` holds only `stage`,
  and the two omaboot theme directories are installed and inactive. The
  sudo journal shows exactly omaboot's commands as root and nothing else:
  at 18:47:33 `omaboot-apply install --staged ...`,
  `plymouth-set-default-theme omaboot`, `omaboot-apply switch --on`,
  `limine-mkinitcpio`; at 18:47:39 the revert: `plymouth-set-default-theme
  omarchy`, `omaboot-apply switch --off`, `limine-mkinitcpio`.
- Expected: `switch --on` writes `/etc/sddm.conf.d/zz-omaboot.conf` and
  verify reads it back.
- Cause: the helper on the machine was from an older build. `strings` on
  `target/release/omaboot-apply` (18 September, 21:48) shows
  `/etc/sddm.conf.d/90-omaboot.conf` and no `--stock`; the source has said
  `zz-omaboot.conf` since the first commit (the rename after the on-screen
  keyboard's `99-z-...conf` was found). `cargo build --release` never
  rebuilt it: the workspace had `default-members = ["crates/omaboot"]`, so
  the root build only built the engine, and `plugin install` kept the copy
  in `~/.local/bin` because it was identical to the equally stale file in
  `target/release`. So `switch --on` wrote `90-omaboot.conf` (which would
  not even have won against `99-z-omarchy-onscreen-keyboard.conf`), verify
  looked for `zz-omaboot.conf`, and the revert's `switch --off` removed the
  `90-` file again, which is why nothing is left. Nothing outside omaboot
  touched the directory; the on-screen keyboard plugin has no watcher
  (read: its `bin/login-keyboard-lib.sh` only installs and uninstalls its
  own drop-in), Omarchy's `bin/` has none, and no path unit watches
  `/etc/sddm.conf.d`.
- Reproduce: `grep -a -o '/etc/sddm.conf.d/[A-Za-z0-9._-]*'
  ~/.local/bin/omaboot-apply` on a helper built before the rename.
- Fixed: `default-members` removed, so `cargo build --release` builds both
  binaries; `crates/omaboot-apply/src/protocol.rs` (PROTOCOL = 2) compiled
  into both, `omaboot-apply protocol` answers it, and every plan that runs
  the helper asks first (`Operation::CheckHelper`) and refuses a mismatch
  with the build command; `plugin install` warns on a stale helper.

## The fourth run

With A1 to A3 fixed, the fourth `Apply to the system` on `matte` ticked all
ten steps. After a reboot the owner saw the omaboot unlock screen, the
omaboot login screen and the omaboot shutdown screen; the lock screen of the
session is Lock Screen Explorer's, as it should be (`docs/SPEC.md`, out of
scope). That is the first line of the v1 definition of done, on one
machine. Not yet done from the window: revert and reset by hand, and the
other three lines of the definition.

### What the A1 step had to become (written before the fix, followed by it)

The greeter is alive after N seconds with no error on stderr, then killed,
is the pass; an exit before N seconds, whatever the code, is the fail; the
captured stderr travels with the verdict either way, and the suggestion
names what the greeter said. What "an error on stderr" is, and whether the
greeter exits at all when `Main.qml` fails to load or stays up with an empty
window, must be read from SDDM's `GreeterApp.cpp` for the installed version
and tried once against a deliberately broken staged theme, before the
verdict is written: if a broken theme keeps the greeter alive, the stderr
scan is the whole test and needs a known bad theme in the test suite. The
preview's `run_greeter` and the smoke test should then share one function,
so the two cannot disagree again.
