# omaboot: architecture and safety model

The failure mode of this project is a user who cannot boot or cannot log in.
Everything below exists to make that outcome unreachable, and recoverable if it
somehow happens anyway.

## Components

```
omaboot            unprivileged: CLI (with --json for the plugin), theme model, asset generation, previews, system reading
plugin/            the window: a Quattro shell plugin in QML, argv-only calls into omaboot --json
omaboot-apply      privileged helper, invoked through polkit, does nothing else
```

The helper is small enough to audit in one sitting. It takes a staged theme
directory and a mode, validates it, installs it atomically, and exits. It never
parses user input, never resolves user-controlled paths after gaining privilege,
and never calls back into `omaboot`.

## Ownership: the rule that everything follows

| Path | Owner |
|------|-------|
| `/usr/share/plymouth/themes/omarchy` | Omarchy. **Never touched.** |
| `/usr/share/sddm/themes/omarchy` | Omarchy. **Never touched.** |
| `/usr/share/plymouth/themes/omaboot` | omaboot |
| `/usr/share/sddm/themes/omaboot` | omaboot |
| `/etc/sddm.conf.d/zz-omaboot.conf` | omaboot, single line `Current=omaboot` |
| `~/.config/omaboot/` | the user's themes |
| `~/.local/state/omaboot/` | applied state, rollback point, logs |

Activation is therefore two reversible acts: `plymouth-set-default-theme omaboot`
and one drop-in file. Deactivation is `plymouth-set-default-theme omarchy` and
`rm` of that file. Upstream's own `omarchy-plymouth-reset` continues to work and
restores a theme that is simply not selected.

The drop-in is called `zz-omaboot.conf` and not `90-omaboot.conf`, which the
first draft of this document said. SDDM reads `/etc/sddm.conf` and then every
`*.conf` in `sddm.conf.d` in file-name order, and the last `[Theme]
Current=` wins. Omarchy ships `99-omarchy-login.conf`, and the on-screen
keyboard login theme installs `99-z-omarchy-onscreen-keyboard.conf` to beat
it (both are on the reference machine, verified 18 September 2026). A `90-`
file would have been silently overridden by either. Writing the drop-in is
therefore not enough: the verify step resolves the theme the way SDDM does
and fails, naming the file, when something else still wins.

The same drop-in has a second use. When a third-party SDDM theme is what logs
you in and no omaboot theme is applied, `omaboot login stock` writes it with
`Current=omarchy` (helper: `switch --stock`) and verifies the result the same
way; `omaboot login release` removes it. That is how Omarchy's stock login
screen is put back in front of a third party's without touching the third
party's file.

## What boots now

Everything the interface and `omaboot status` say about the current screens
is read from the system when asked, never from a cache: `plymouthd.conf`
names the Plymouth theme, the SDDM configuration files decide the login theme
(and which file decided it), and the installed theme directories supply the
colours and the logo. The `system` module does this; it writes nothing. The
applied-state record is shown next to what the system says, never instead of
it, so a record the system no longer agrees with shows up as exactly that.

That entry is not edited in place. The way to change what boots is to make a
theme of your own from it (`e` in the interface, `omaboot new --from-current`
in the shell), edit that, and apply it. A theme derived from Omarchy's own
Plymouth theme takes its background from `Window.SetBackgroundTopColor` in the
installed script, its text colour from the installed `bullet.png`, its logo
from the installed `logo.png`, and its accent and error colours from the login
theme's `theme.conf` when that has them. When omaboot's own theme is
installed, the theme it was applied from is copied instead, since that is the
exact description of what is installed.

## Apply pipeline

Each step must be individually re-runnable, and the system must be bootable
between any two steps.

1. **Validate.** Parse `theme.toml` strictly. Check assets exist, are regular
   files, are not symlinks, and are within size limits. Reject anything unknown.
2. **Generate.** Rasterise SVG logos, recolour glyph assets, write the Plymouth
   script from the template with values substituted as data, not as `sed` over a
   shipped script. Same for the SDDM QML.
3. **Stage.** Build the complete theme in a fresh directory under `/tmp` owned by
   the invoking user. Nothing partial is ever visible to the system.
4. **Smoke test the greeter, before it can ever be shown at login.** Run the
   staged SDDM theme under `sddm-greeter --test-mode` headless (offscreen Qt
   platform). The greeter in test mode never exits on its own, so the test
   is inverted from a normal command: the greeter has to still be running
   when the settling time is up (`greeter::SETTLE`, 10 seconds), it is then
   stopped, and its stderr must carry no complaint about the theme (a QML
   error naming a file under the staged directory, SDDM's fallback to its
   embedded theme, or any `(EE)` line). An exit before the time is up,
   whatever the code, or a complaint, aborts here with what the greeter
   said. This is the single most valuable check in the pipeline: SDDM
   greeter crash loops are a known Omarchy failure mode
   ([#10302](https://github.com/omacom/omarchy/issues/10302)). The same
   command, minus the offscreen platform, is the login preview
   (`crates/omaboot/src/greeter.rs` is the one place both come from).
5. **Authorise.** One polkit prompt for the whole operation. Not one per file.
6. **Install.** First, unprivileged, `omaboot-apply protocol` must answer the
   number this engine was built with (`crates/omaboot-apply/src/protocol.rs`,
   compiled into both binaries): a helper from an older build writes other
   paths than the engine then verifies, and is refused here with the build
   command to run. Then `omaboot-apply` copies into the omaboot-owned directories using
   root-owned staging plus atomic rename per file. Destination directories are
   validated as root-owned and not group or world writable, and symlinks are
   refused, mirroring the hardening upstream already applies in
   `omarchy-plymouth-set`.
7. **Record the rollback point** *before* switching anything: previous default
   Plymouth theme name, previous SDDM drop-in contents, timestamp, theme hash.
8. **Switch.** Set the default Plymouth theme, write the SDDM drop-in.
9. **Rebuild initramfs.** `limine-mkinitcpio` when present, otherwise
   `mkinitcpio -P`, matching what upstream does. Stream output to the UI.
10. **Verify.** Confirm the default theme is what we set, the drop-in is present,
    and the installed files hash to what was staged. Any mismatch reverts
    automatically and says so.

Interrupt handling: a SIGINT before step 8 aborts cleanly. From step 8 onward the
handler completes the current file operation and then reverts. There is no window
in which a half-written theme is the active theme, because activation is a rename
and a one-line file, not a directory copy.

## Recovery

- `omaboot revert` restores the recorded rollback point and rebuilds.
- `omaboot reset` goes further and removes omaboot from the system entirely.
- `omaboot doctor` reads the journal for `sddm` and `sddm-greeter` unit failures
  since the last apply, checks drift between recorded and actual state, and
  verifies dependencies. It suggests exactly one command per finding.
- **The printed rescue path.** After every apply, and in the README, document the
  three commands to recover from a TTY (`Ctrl+Alt+F2`) with no GUI:
  `sudo plymouth-set-default-theme omarchy`, `sudo rm
  /etc/sddm.conf.d/zz-omaboot.conf`, `sudo mkinitcpio -P`. A user who can read
  that line is never stuck, which is worth more than any amount of cleverness.
- Consider for v2: a first-boot confirmation. After applying, arm a systemd unit
  that reverts unless `omaboot confirm` runs after a successful graphical login.
  Specify it before building it; a watchdog that misfires is worse than none.

## Previews without rebooting

**Plymouth.** `plymouthd --no-daemon --debug` with `plymouth show-splash`, then
`plymouth quit` (documented on the Gentoo wiki for theme development). Drive the
password prompt and messages through `plymouth` client commands so the preview
shows the states that actually matter. Run on a spare VT, restore the previous VT,
enforce a hard timeout, and tear down in a `Drop` implementation so a panic cannot
leave the user staring at a splash screen. `plymouthd --mode` selects boot versus
shutdown; verify the exact accepted values at runtime rather than trusting this
document.

**SDDM.** `sddm-greeter --test-mode --theme <dir>`, noting that on Qt 6 the binary
is `sddm-greeter-qt6`. Detect which exists; do not hard code.

**Inline composite.** omaboot renders the frame itself from the theme model and
pushes it into the terminal. This is what makes editing feel live, and it must
agree with the real render. Add a test that composites and screenshots the real
`plymouthd` output and compares layout anchor points, so the two cannot drift
silently.

## Following the Omarchy theme

`omarchy-theme-set` ends with `omarchy-hook theme-set "$THEME_NAME"`, and
`omarchy-hook` runs `~/.config/omarchy/hooks/theme-set` plus every file in
`~/.config/omarchy/hooks/theme-set.d/`. That is a supported user hook, verified
against the installed tree on 18 September 2026, so omaboot installs a drop-in
there rather than watching state files. An earlier draft of this document claimed
no hook existed and proposed a `systemd --user` path unit on
`~/.local/state/omarchy/current/theme.name`; that unit is the fallback if the
hook is ever removed, not the primary design.

Because applying needs root, the default behaviour on a theme change is to
**notify, not to act**: a desktop notification offering to restyle the boot
screens, applying only on confirmation. A silent sudo prompt appearing seconds
after a theme switch would be both startling and, correctly, suspicious.

## Testing

- The theme model, validator, generator, and state machine are pure and unit
  tested with no root and no system calls.
- The apply pipeline is tested against a temporary root prefix, so every step runs
  in CI without touching a real system.
- Integration tests run in a QEMU VM with a real LUKS volume and a real SDDM,
  booting and screenshotting. This is the only way to be sure, and it is worth the
  setup cost.
- **Never run a real apply on the development machine during ordinary work.**
  Development uses `--dry-run` and the VM.
