#!/bin/bash
# The apply pipeline, end to end, against a temporary prefix.
#
# This is the round CI runs on every push: scaffold a theme, edit it, dry run,
# apply, see the system it would boot, provoke drift, revert, apply again,
# reset, and check the failure paths that do not need a machine. Nothing here
# needs root, Omarchy, Plymouth, SDDM or a display: `--root <prefix>` redirects
# every system path into a temporary directory and executes no system command,
# and HOME and the XDG variables point at a temporary home, so the real
# ~/.config/omaboot and ~/.local/state/omaboot are never read or written
# (crates/omaboot/src/paths.rs, Layout::discover).
#
# What a prefixed run cannot prove, and where it is proven instead:
#   - the greeter smoke test and the privileged helper never execute under a
#     prefix (apply/mod.rs, step_install and step_smoke_test), so their failure
#     paths are unit tests with a recording runner (apply/tests.rs) and the
#     helper's own tests (crates/omaboot-apply);
#   - what a machine actually boots is proven in the VM (scripts/vm-round.sh).
#
#   scripts/ci-pipeline.sh [path-to-omaboot]
set -euo pipefail

OMABOOT=${1:-target/release/omaboot}
OMABOOT=$(cd "$(dirname "$OMABOOT")" && pwd)/$(basename "$OMABOOT")
[[ -x $OMABOOT ]] || { echo "not executable: $OMABOOT (cargo build --release)" >&2; exit 2; }

WORK=$(mktemp -d -t omaboot-ci-XXXXXX)
trap 'rm -rf "$WORK"' EXIT

# A home of its own, so nothing of the person running this is read or written.
export HOME=$WORK/home
export XDG_CONFIG_HOME=$WORK/home/.config
export XDG_STATE_HOME=$WORK/home/.local/state
export XDG_DATA_HOME=$WORK/home/.local/share
export XDG_CACHE_HOME=$WORK/home/.cache
unset OMARCHY_PATH
mkdir -p "$HOME"
PREFIX=$WORK/prefix
THEMES=$XDG_CONFIG_HOME/omaboot/themes

failures=0
step() { printf '\n== %s\n' "$1"; }
ok() { printf '   ok   %s\n' "$1"; }
bad() { printf '   FAIL %s\n' "$1"; failures=$((failures + 1)); }
check_file() { if [[ -f $2 ]]; then ok "$1"; else bad "$1 (no $2)"; fi; }
check_no_file() { if [[ ! -e $2 ]]; then ok "$1"; else bad "$1 ($2 exists)"; fi; }
contains() { if grep -qF -- "$2" <<<"$1"; then ok "$3"; else bad "$3"; printf '%s\n' "$1" | tail -5 >&2; fi; }

run() { "$OMABOOT" --root "$PREFIX" "$@"; }
# A run that is expected to fail: its output is captured, its exit code checked.
run_fails() {
  local out
  if out=$("$OMABOOT" --root "$PREFIX" "$@" 2>&1); then
    bad "expected a failure from: omaboot $*"
    printf '%s\n' "$out" | tail -3 >&2
    return 0
  fi
  printf '%s\n' "$out"
}

step "a system to apply onto: $PREFIX"
mkdir -p "$PREFIX/usr/bin" "$PREFIX/etc/plymouth" "$PREFIX/etc/sddm.conf.d"
for tool in sddm-greeter-qt6 limine-mkinitcpio plymouth-set-default-theme omaboot-apply; do
  printf '#!/bin/sh\nexit 0\n' >"$PREFIX/usr/bin/$tool"
  chmod +x "$PREFIX/usr/bin/$tool"
done
printf '[Daemon]\nTheme=omarchy\n' >"$PREFIX/etc/plymouth/plymouthd.conf"
# The packaged Omarchy tree the generator reads its glyphs from. The real one
# is a checkout; these are solid PNGs of the sizes Omarchy's own glyphs have
# (crates/omaboot/src/generate/mod.rs, fixture::glyph_size), which is all the
# generator needs: it recolours them and writes them into the staged theme.
python3 - "$PREFIX" "$WORK/logo.png" <<'PY'
import pathlib, struct, sys, zlib

def png(path, width, height, rgba=(0x7A, 0xA2, 0xF7, 0xFF)):
    rows = b"".join(b"\x00" + bytes(rgba) * width for _ in range(height))
    def chunk(kind, body):
        data = kind + body
        return struct.pack(">I", len(body)) + data + struct.pack(">I", zlib.crc32(data) & 0xFFFFFFFF)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(rows, 9))
        + chunk(b"IEND", b"")
    )

sizes = {
    "bullet.png": (7, 7),
    "entry.png": (400, 50),
    "entry-failed.png": (400, 50),
    "lock.png": (84, 96),
    "lock-failed.png": (84, 96),
    "progress_bar.png": (396, 16),
    "progress_box.png": (400, 20),
    "logo.png": (256, 256),
}
tree = pathlib.Path(sys.argv[1], "usr/share/omarchy/default")
for name, (width, height) in sizes.items():
    png(tree / "plymouth" / name, width, height)
    png(tree / "sddm/omarchy" / name, width, height)
# A logo to drop into a theme, the way a person drops one on the window.
png(pathlib.Path(sys.argv[2]), 256, 256, (0xC0, 0xCA, 0xF5, 0xFF))
PY
# Omarchy's own installed themes, the two directories omaboot promises never to
# write into (README, "The one rule"). Their fingerprint is taken now and
# compared after the whole round.
mkdir -p "$PREFIX/usr/share/plymouth/themes/omarchy" "$PREFIX/usr/share/sddm/themes/omarchy"
cp "$PREFIX/usr/share/omarchy/default/plymouth/"*.png "$PREFIX/usr/share/plymouth/themes/omarchy/"
cp "$PREFIX/usr/share/omarchy/default/sddm/omarchy/"*.png "$PREFIX/usr/share/sddm/themes/omarchy/"
printf '[Plymouth Theme]\nName=Omarchy\nModuleName=script\n' >"$PREFIX/usr/share/plymouth/themes/omarchy/omarchy.plymouth"
printf '# Omarchy, stock\nWindow.SetBackgroundTopColor(0.07, 0.07, 0.07);\n' >"$PREFIX/usr/share/plymouth/themes/omarchy/omarchy.script"
printf '[General]\nbackground=\n' >"$PREFIX/usr/share/sddm/themes/omarchy/theme.conf"
printf '// Omarchy, stock\n' >"$PREFIX/usr/share/sddm/themes/omarchy/Main.qml"
fingerprint_omarchy() {
  (cd "$PREFIX/usr/share" && find plymouth/themes/omarchy sddm/themes/omarchy \
    \( -type f -o -type l \) -exec sha256sum {} + | sort)
}
OMARCHY_BEFORE=$(fingerprint_omarchy)
ok "fake tools, plymouthd.conf, an Omarchy tree, its two installed themes and a logo staged"

step "status on a machine omaboot has never touched"
status=$(run status)
contains "$status" "not installed" "no omaboot theme is installed"
check_no_file "no state written yet" "$XDG_STATE_HOME/omaboot/applied.toml"

step "a theme of your own"
run new mine >/dev/null
check_file "the manifest is there" "$THEMES/mine/theme.toml"
run set mine colors.background=#1a1b26 colors.foreground=#c0caf5 login.clock=false >/dev/null
contains "$(cat "$THEMES/mine/theme.toml")" "#1a1b26" "set wrote the background"
contains "$(cat "$THEMES/mine/theme.toml")" "clock = false" "set wrote the login clock"
# A scaffolded theme names a logo it does not have yet, and says so until one
# is there: the manifest is not trusted, the directory is read.
contains "$(run_fails validate mine)" "logo.png" "a theme without its logo is refused, naming the file"
run add-image mine "$WORK/logo.png" --as logo >/dev/null
contains "$(run validate mine)" "valid" "validate passes once the logo is there"

step "failure paths that need no machine"
contains "$(run_fails apply not-a-theme)" "not-a-theme" "applying a theme that does not exist names it"
contains "$(run_fails set mine colors.background=purple)" "purple" "an unparseable colour names the value"
contains "$(run_fails set mine colors.nonsense=#000000)" "nonsense" "an unknown key is refused, not ignored"

step "dry run writes nothing"
dry=$(run apply mine --dry-run)
contains "$dry" "$PREFIX/usr/share/plymouth/themes/omaboot" "the plan names the omaboot theme directory"
contains "$dry" "$PREFIX/etc/sddm.conf.d/zz-omaboot.conf" "the plan names the one SDDM drop-in"
check_no_file "the plymouth theme was not installed" "$PREFIX/usr/share/plymouth/themes/omaboot/omaboot.plymouth"
check_no_file "the drop-in was not written" "$PREFIX/etc/sddm.conf.d/zz-omaboot.conf"
check_no_file "no state was written" "$XDG_STATE_HOME/omaboot/applied.toml"
contains "$(cat "$PREFIX/etc/plymouth/plymouthd.conf")" "Theme=omarchy" "plymouthd.conf still names omarchy"

step "apply"
applied=$(run apply mine)
contains "$applied" "Reboot to see it" "the apply finished"
check_file "the plymouth theme is installed" "$PREFIX/usr/share/plymouth/themes/omaboot/omaboot.plymouth"
check_file "the plymouth script is installed" "$PREFIX/usr/share/plymouth/themes/omaboot/omaboot.script"
check_file "the sddm theme is installed" "$PREFIX/usr/share/sddm/themes/omaboot/Main.qml"
check_file "the drop-in is written" "$PREFIX/etc/sddm.conf.d/zz-omaboot.conf"
check_file "the applied state is recorded" "$XDG_STATE_HOME/omaboot/applied.toml"
contains "$(cat "$PREFIX/etc/plymouth/plymouthd.conf")" "Theme=omaboot" "plymouthd.conf names omaboot"

step "Omarchy's own themes are byte for byte untouched"
if [[ $(fingerprint_omarchy) == "$OMARCHY_BEFORE" ]]; then
  ok "both Omarchy theme directories are as they were"
else
  bad "an apply changed a file Omarchy owns"
  diff <(printf '%s\n' "$OMARCHY_BEFORE") <(fingerprint_omarchy) >&2 || true
fi

step "status sees what it applied"
status=$(run status)
contains "$status" "applied: mine" "status names the applied theme"
contains "$status" "drift:   none" "status reports no drift"
json=$(run --json status)
python3 - "$json" <<'PY'
import json, sys
answer = json.loads(sys.argv[1].splitlines()[-1])
applied = answer.get("applied") or {}
assert applied.get("theme") == "mine", answer
assert not (answer.get("drift") or {}).get("files_changed"), answer
print("   ok   --json says the same")
PY

step "drift is seen, not hidden"
printf 'tampered\n' >>"$PREFIX/usr/share/plymouth/themes/omaboot/omaboot.script"
contains "$(run status)" "drift" "an edited installed file shows as drift"

step "revert puts the recorded state back"
run revert >/dev/null
contains "$(cat "$PREFIX/etc/plymouth/plymouthd.conf")" "Theme=omarchy" "plymouthd.conf names omarchy again"
check_no_file "the drop-in is gone" "$PREFIX/etc/sddm.conf.d/zz-omaboot.conf"

step "apply again, then reset to stock"
run apply mine >/dev/null
check_file "applied a second time" "$PREFIX/etc/sddm.conf.d/zz-omaboot.conf"
run reset >/dev/null
check_no_file "the drop-in is gone" "$PREFIX/etc/sddm.conf.d/zz-omaboot.conf"
check_no_file "the omaboot plymouth theme is gone" "$PREFIX/usr/share/plymouth/themes/omaboot"
check_no_file "the omaboot sddm theme is gone" "$PREFIX/usr/share/sddm/themes/omaboot"
check_no_file "the state is gone" "$XDG_STATE_HOME/omaboot/applied.toml"
contains "$(cat "$PREFIX/etc/plymouth/plymouthd.conf")" "Theme=omarchy" "plymouthd.conf names omarchy"
check_file "your theme is still yours" "$THEMES/mine/theme.toml"
if [[ $(fingerprint_omarchy) == "$OMARCHY_BEFORE" ]]; then
  ok "after apply, revert, apply and reset, Omarchy's own themes never changed"
else
  bad "the round changed a file Omarchy owns"
  diff <(printf '%s\n' "$OMARCHY_BEFORE") <(fingerprint_omarchy) >&2 || true
fi

step "render draws the three screens from the same bytes"
for screen in unlock login shutdown; do
  run render mine --screen "$screen" --out "$WORK/$screen.png" >/dev/null
  check_file "$screen.png" "$WORK/$screen.png"
done

printf '\n'
if ((failures)); then
  echo "$failures check(s) failed"
  exit 1
fi
echo "the round passed"
