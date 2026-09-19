#!/bin/bash
# Drive the window through its editing flows and check what landed on disk.
# The same engine, the same QML, no shell: `omaboot set` through the window's
# own save queue, an image drop, a rename, a new theme, a delete.
#   OMARCHY_PATH=/usr/share/omarchy plugin/harness/exercise.sh
set -euo pipefail
HERE=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
WORK=${OMABOOT_HARNESS_WORK:-$HERE/.work}
OUT=${1:-/tmp/omaboot-exercise}
mkdir -p "$OUT"
THEMES="$WORK/home/.config/omaboot/themes"
LOGO=${OMARCHY_PATH:?}/themes/tokyo-night/unlock.png

render() { python3 "$HERE/render.py" --work "$WORK" --out "$OUT/$1.png" "${@:2}" ${RENDER_ARGS:-} 2>&1 | grep -v "^qt: qml:" | grep -i "wrote\|error\|expected" || true; }
expect() { grep -qF -- "$2" "$1" || { echo "FAIL: $1 lacks: $2"; sed -n 1,60p "$1"; exit 1; }; }
reject() { grep -qF -- "$2" "$1" && { echo "FAIL: $1 has: $2"; sed -n 1,60p "$1"; exit 1; } || true; }

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

echo "all flows passed; pictures in $OUT"
