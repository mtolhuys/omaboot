# Working agreement for omaboot

omaboot designs and applies the Plymouth unlock screen, the SDDM login screen,
and the Plymouth shutdown screen on Omarchy. The engine is Rust (`crates/`);
the interface is a Quattro shell plugin in QML (`plugin/`) that drives the
engine over `omaboot --json` and never touches a file or the system itself.

Read `README.md`, then `docs/SPEC.md`, `docs/ARCHITECTURE.md`, `docs/UI.md`, and
`docs/UPSTREAM.md` before writing code. They are the source of truth; if code and
docs disagree, update the doc in the same commit rather than leaving the drift.

## Hard rules

1. **Never write to `/usr/share/plymouth/themes/omarchy` or
   `/usr/share/sddm/themes/omarchy`.** Not in code, not in a test, not in a
   script, not "temporarily". This is the project's central promise.
2. **Never run a real apply, `mkinitcpio`, or `plymouth-set-default-theme` on this
   development machine.** Use `--dry-run`, a temporary root prefix, or the QEMU
   VM. Ask before anything that touches real system state.
3. **Privileged code lives only in the `omaboot-apply` binary.** It receives an
   already-staged, already-validated theme directory. It parses no user input and
   never calls back into `omaboot`. Any change to its verbs, its manifests or
   the paths it writes bumps `PROTOCOL` in `crates/omaboot-apply/src/protocol.rs`,
   which both binaries compile in and the engine checks before the first
   privileged step.
4. **No `unsafe`.** Both crates forbid it.
5. **The greeter is smoke tested before it is ever activated.** No code path may
   skip step 4 of the apply pipeline.
6. **Everything in the repository is English**: identifiers, comments, docs,
   commit messages, user-facing strings.

## Order of work

Follow the milestones in `docs/SPEC.md`. M1 (correct, reversible apply) before M2
(preview) before M3 (the plugin). A beautiful window over an apply pipeline that
can brick a machine is the wrong project.

## Verify, do not assume

Omarchy moves fast. Before depending on any upstream behaviour, read the installed
source at `$OMARCHY_PATH` (usually `/usr/share/omarchy`) and confirm. The facts in
`docs/UPSTREAM.md` were true for `4.0.0.alpha` on the `quattro` branch on
18 September 2026, with additions verified on 19 September 2026, and carry
a date for that reason.

The same applies to external tools: detect `sddm-greeter` versus
`sddm-greeter-qt6`, detect `limine-mkinitcpio` versus `mkinitcpio`, and check the
accepted values of `plymouthd --mode` at runtime instead of trusting a document.

## Testing

- Pure logic (theme model, validation, generation, state machine) is unit tested
  and requires no root and no system calls. `cargo test --workspace` must pass
  with and without `OMARCHY_PATH` set: a `--root` prefix reads the Omarchy
  tree at `<prefix>/usr/share/omarchy` and never at the variable.
- The apply pipeline is tested against a temporary root prefix in CI:
  `scripts/ci-pipeline.sh` runs the whole round (scaffold, dry run, apply,
  drift, revert, reset) with `--root` and a temporary home, and fails if a
  file Omarchy owns changed. `.github/workflows/ci.yml` runs it, the unit
  tests with and without `OMARCHY_PATH`, qmllint, and `makepkg` on the
  PKGBUILD. Releases follow `docs/RELEASING.md`.
- Anything that can only be proven by booting is proven in the VM, with a
  screenshot committed to the test evidence directory.
- A change to the apply pipeline without a test that exercises its failure path is
  not finished.
- A change to the window is looked at before it is delivered:
  `plugin/harness/shots.sh <dir>` renders it offscreen with the real shell
  widgets and engine (`docs/UI.md`, "Seeing it without a shell"),
  `plugin/harness/exercise.sh` drives the editing flows and checks the
  theme file, and `plugin/harness/theme-follow.sh` proves the standalone
  app follows a theme switch. qmllint catches syntax; the harness catches
  layout and lost edits. The harness works in `.harness/` at the repository
  root, never in `plugin/` (the shell watches that directory) and never in
  the real `~/.config/omaboot` or `~/.local/share` (exercise.sh checks the
  config directory and the launcher entry are untouched; the wrapper and
  theme-follow.sh set every XDG variable, `XDG_DATA_HOME` included).

## Style

- Small modules with clear seams: `theme`, `generate`, `apply`, `preview`,
  `system`, `api`, `plugin`.
- Errors carry context and a suggested next action; the window shows the
  sentence, stderr carries the chain.
- No panics on any user-reachable path.
- The plugin is argv only: every engine call is a `Process` with a command list,
  never a shell string. What the plugin needs that the engine does not offer is
  added to the engine, with a test, not done in QML.
