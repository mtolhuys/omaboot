#!/bin/bash
# Drive the window through its editing flows and check what landed on disk.
# The same engine, the same QML, no shell: `omaboot set` through the window's
# own save queue, an image drop, a rename, a new theme, a delete.
#   OMARCHY_PATH=/usr/share/omarchy plugin/harness/exercise.sh
set -euo pipefail
HERE=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO=$(cd -- "$HERE/../.." && pwd)
WORK=${OMABOOT_HARNESS_WORK:-$REPO/.harness}
OUT=${1:-/tmp/omaboot-exercise}
mkdir -p "$OUT"
THEMES="$WORK/home/.config/omaboot/themes"
LOGO=${OMARCHY_PATH:?}/themes/tokyo-night/unlock.png

render() {
  local log code
  log=$(python3 "$HERE/render.py" --work "$WORK" --out "$OUT/$1.png" "${@:2}" ${RENDER_ARGS:-} 2>&1); code=$?
  echo "$log" | grep -v "^qt: qml:" | grep -i "wrote\|error\|expected\|warning" || true
  return $code
}
expect() { grep -qF -- "$2" "$1" || { echo "FAIL: $1 lacks: $2"; sed -n 1,60p "$1"; exit 1; }; }
reject() { grep -qF -- "$2" "$1" && { echo "FAIL: $1 has: $2"; sed -n 1,60p "$1"; exit 1; } || true; }

# The harness must never reach the caller's own omaboot directory. A
# fingerprint of it (every path with its size and mtime) is taken before the
# flows and compared after them; a directory that does not exist must still
# not exist. This is the regression check for the wrapper that exported HOME
# but not XDG_CONFIG_HOME, and so read and wrote the real ~/.config/omaboot.
REAL_CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}/omaboot"
# The desktop entry and the icons of `omaboot app install` live under the
# data directory; a harness that reaches them installs or removes the real
# launcher entry (it happened once, 20 September 2026).
REAL_DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
fingerprint() {
  if [ -e "$REAL_CONFIG" ]; then find "$REAL_CONFIG" -printf '%p %s %T@\n' | sort; else echo "absent"; fi
  for f in "$REAL_DATA/applications/omaboot.desktop" "$REAL_DATA"/icons/hicolor/*/apps/omaboot.png; do
    if [ -e "$f" ]; then stat -c '%n %s %Y' "$f"; else echo "$f absent"; fi
  done
}
BEFORE=$(fingerprint)
# Checked on the way out, so a leak is named even when it makes a flow fail
# before the end (a seeded theme that the window then cannot find, say).
untouched() {
  local code=$?
  local after; after=$(fingerprint)
  if [ "$BEFORE" != "$after" ]; then
    echo "FAIL: the harness reached $REAL_CONFIG or the launcher entry under $REAL_DATA"; diff <(echo "$BEFORE") <(echo "$after") || true; exit 1
  fi
  case "$THEMES" in "$REAL_CONFIG"/*) echo "FAIL: the harness themes directory is inside $REAL_CONFIG"; exit 1;; esac
  [ "$code" -eq 0 ] && echo "the real config directory $REAL_CONFIG is untouched, and so is the launcher entry under $REAL_DATA"
  exit "$code"
}
trap untouched EXIT

rm -rf "$THEMES/matte" "$THEMES/second"

echo "== edits in a burst are all saved, in order"
render edits --size 1600x1000 --select matte \
  --script 'setField("logo.width", "0.65")' \
  --script 'setField("colors.accent", "#ff0000"); setField("logo.position", "top"); setField("logo.width", "0.70")' \
  --expect-status ""
expect "$THEMES/matte/theme.toml" 'width = 0.7'
expect "$THEMES/matte/theme.toml" 'accent = "#ff0000"'
expect "$THEMES/matte/theme.toml" 'position = "top"'

echo "== the shutdown screen gets its own logo width, the unlock screen keeps its own"
render shutdown-scale --size 1600x1000 --select matte --screen shutdown \
  --script 'setField("logo.shutdown.width", "0.30")'
expect "$THEMES/matte/theme.toml" '[logo.shutdown]'
expect "$THEMES/matte/theme.toml" 'width = 0.3'
expect "$THEMES/matte/theme.toml" 'width = 0.7'
reject "$THEMES/matte/theme.toml" '[logo.login]'

echo "== the file dialog opens in ~/Pictures first, then where the last image came from"
render picker --size 1600x1000 --select matte \
  --script 'var want = "--filename=" + Quickshell.env("HOME") + "/Pictures/"; say(chooserCommand("logo").indexOf(want) >= 0 ? "picker opens in " + want : "FAIL: the picker command lacks " + want + ": " + chooserCommand("logo").join(" "), false)' \
  --expect-status "picker opens in --filename="
render picker-after --size 1600x1000 --select matte \
  --script "chooser.chosen = \"$LOGO\"; chooser.exited(0, 0)" \
  --script "say(chooserCommand(\"background\").indexOf(\"--filename=$(dirname "$LOGO")/\") >= 0 ? \"picker now opens in $(dirname "$LOGO")/\" : \"FAIL: the picker did not follow the last image: \" + chooserCommand(\"background\").join(\" \"), false)" \
  --expect-status "picker now opens in"
expect "$THEMES/matte/theme.toml" 'source = "unlock.png"'

echo "== a dropped logo is copied in and used"
render drop --size 1600x1000 --select matte --script "dropFile(\"$LOGO\", \"logo\")"
test -f "$THEMES/matte/unlock.png" || { echo "FAIL: unlock.png not copied"; exit 1; }
expect "$THEMES/matte/theme.toml" 'source = "unlock.png"'

echo "== a dropped wallpaper switches the login background to it"
render wallpaper --size 1600x1000 --select matte --screen login --script "dropFile(\"$LOGO\", \"background\")"
test -f "$THEMES/matte/background.png" || { echo "FAIL: background.png not copied"; exit 1; }
expect "$THEMES/matte/theme.toml" 'background = "image"'

echo "== rename keeps the id and changes the name"
render rename --size 1600x1000 --select matte --script 'renameDialog.openWith("Renamed"); renameDialog.commit()'
expect "$THEMES/matte/theme.toml" 'name = "Renamed"'

echo "== a new theme from what boots now, then deleted"
render new --size 1600x1000 --select now --script 'makeTheme("second", "current")'
test -f "$THEMES/second/theme.toml" || { echo "FAIL: second not created"; ls "$THEMES"; exit 1; }
render delete --size 1600x1000 --select second --script 'removeTheme("second")'
test ! -e "$THEMES/second" || { echo "FAIL: second not deleted"; exit 1; }

echo "== a dry run reports every step"
render dryrun --size 1600x1000 --select matte --script 'start("dryrun")' --settle 2 --expect-status ""

echo "== without the engine the window says so and spawns nothing"
render engine-missing --size 1100x760 --no-engine --timeout 3 --expect-status "omaboot is not installed"

echo "all flows passed; pictures in $OUT"
