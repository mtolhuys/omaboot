#!/bin/bash
# Prove a generated theme with the real daemons, headlessly.
#
# Runs the staged theme under a real plymouthd (X11 renderer on Xvfb) and a
# real sddm-greeter in test mode, drives the password prompt with xdotool,
# screenshots each state, and compares the unlock and login frames with what
# `omaboot render` composites. This is the check docs/UI.md asks for: the
# picture omaboot draws and the picture the daemons draw must agree.
#
# Needs: plymouth with its x11 renderer and label plugin (Debian/Ubuntu:
# plymouth plymouth-x11 plymouth-label), sddm, xvfb, xdotool, imagemagick,
# python3 with Pillow. Must run as root: plymouthd insists on it.
#
# Usage: scripts/verify-render.sh <theme-name> [out-dir]
#   with HOME pointing at a config dir that holds the theme, and OMARCHY_PATH
#   at a tree with default/plymouth and default/sddm/omarchy.
set -euo pipefail

THEME=${1:?theme name}
OUT=${2:-/tmp/omaboot-verify}
DISPLAY_NUMBER=${DISPLAY_NUMBER:-:98}
OMABOOT=${OMABOOT:-$(dirname "$0")/../target/release/omaboot}
PREFIX=$OUT/prefix

mkdir -p "$OUT" "$PREFIX/usr/bin" "$PREFIX/etc/plymouth" "$PREFIX/etc/sddm.conf.d"
for tool in sddm-greeter-qt6 limine-mkinitcpio plymouth-set-default-theme; do
  printf '#!/bin/sh\nexit 0\n' > "$PREFIX/usr/bin/$tool"; chmod +x "$PREFIX/usr/bin/$tool"
done
printf '[Daemon]\nTheme=omarchy\n' > "$PREFIX/etc/plymouth/plymouthd.conf"

echo "== generating $THEME into $PREFIX"
"$OMABOOT" apply "$THEME" --root "$PREFIX" > /dev/null
"$OMABOOT" render "$THEME" --screen unlock --out "$OUT/composite-unlock.png" > /dev/null
"$OMABOOT" render "$THEME" --screen login --out "$OUT/composite-login.png" > /dev/null

echo "== starting Xvfb on $DISPLAY_NUMBER"
Xvfb "$DISPLAY_NUMBER" -screen 0 1920x1080x24 > /dev/null 2>&1 &
XVFB=$!
trap 'plymouth quit 2>/dev/null; kill $XVFB 2>/dev/null; rm -rf /run/plymouth/themes/omaboot-verify' EXIT
sleep 1

echo "== plymouthd, boot mode"
# The theme goes under /run, where plymouthd looks first, and is named on the
# fake kernel command line; the system's own configuration is untouched.
rm -rf /run/plymouth/themes/omaboot-verify
mkdir -p /run/plymouth/themes
cp -r "$PREFIX/usr/share/plymouth/themes/omaboot" /run/plymouth/themes/omaboot-verify
mv /run/plymouth/themes/omaboot-verify/omaboot.plymouth /run/plymouth/themes/omaboot-verify/omaboot-verify.plymouth
sed -i 's|/usr/share/plymouth/themes/omaboot|/run/plymouth/themes/omaboot-verify|g' /run/plymouth/themes/omaboot-verify/omaboot-verify.plymouth
DISPLAY=$DISPLAY_NUMBER script -qfec "plymouthd --no-daemon --mode=boot --tty=\$(tty) --kernel-command-line='splash plymouth.ignore-serial-consoles plymouth.splash=omaboot-verify'" /dev/null > /dev/null 2>&1 &
for _ in $(seq 1 50); do plymouth --ping 2>/dev/null && break; sleep 0.1; done
plymouth --ping
plymouth show-splash; sleep 1
(plymouth ask-for-password --prompt "" > /dev/null 2>&1 &)
sleep 1; DISPLAY=$DISPLAY_NUMBER import -window root "$OUT/plymouth-unlock-prompt.png"
DISPLAY=$DISPLAY_NUMBER xdotool type --delay 40 "abcde"; sleep 0.8
DISPLAY=$DISPLAY_NUMBER import -window root "$OUT/plymouth-unlock-typed.png"
DISPLAY=$DISPLAY_NUMBER xdotool key Return; sleep 1.5
DISPLAY=$DISPLAY_NUMBER import -window root "$OUT/plymouth-unlock-progress.png"
plymouth quit; sleep 0.5

echo "== plymouthd, shutdown mode"
DISPLAY=$DISPLAY_NUMBER script -qfec "plymouthd --no-daemon --mode=shutdown --tty=\$(tty) --kernel-command-line='splash plymouth.ignore-serial-consoles plymouth.splash=omaboot-verify'" /dev/null > /dev/null 2>&1 &
for _ in $(seq 1 50); do plymouth --ping 2>/dev/null && break; sleep 0.1; done
plymouth show-splash; sleep 1.5
DISPLAY=$DISPLAY_NUMBER import -window root "$OUT/plymouth-shutdown.png"
plymouth quit; sleep 0.5

echo "== sddm-greeter, test mode"
GREETER=$(command -v sddm-greeter-qt6 || command -v sddm-greeter)
DISPLAY=$DISPLAY_NUMBER timeout 15 "$GREETER" --test-mode --theme "$PREFIX/usr/share/sddm/themes/omaboot" > "$OUT/greeter.log" 2>&1 &
GPID=$!
sleep 4
DISPLAY=$DISPLAY_NUMBER xdotool type --delay 40 "abcde"; sleep 0.8
DISPLAY=$DISPLAY_NUMBER import -window root "$OUT/sddm-login-typed.png"
kill $GPID 2>/dev/null || true

echo "== comparing composites with the real renders"
python3 - "$OUT" <<'PY'
import sys
from PIL import Image, ImageChops
out = sys.argv[1]
worst = 0
for name, composite, real in [
    ("unlock", "composite-unlock.png", "plymouth-unlock-typed.png"),
    ("login", "composite-login.png", "sddm-login-typed.png"),
]:
    a = Image.open(f"{out}/{composite}").convert("RGB")
    b = Image.open(f"{out}/{real}").convert("RGB")
    diff = ImageChops.difference(a, b).convert("L")
    differing = sum(1 for v in diff.getdata() if v > 40)
    share = differing / (a.size[0] * a.size[1])
    worst = max(worst, share)
    print(f"{name}: {differing} pixels differ ({share:.2%})")
# Text antialiasing and a pixel of offset are expected; a wrong layout is not.
limit = 0.02
print("agreement within limit" if worst <= limit else f"DRIFT: more than {limit:.0%} of pixels differ")
sys.exit(0 if worst <= limit else 1)
PY
echo "== done: screenshots in $OUT"
