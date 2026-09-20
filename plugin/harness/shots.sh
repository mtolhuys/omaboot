#!/bin/bash
# Render the views a design pass looks at, into one directory.
#   plugin/harness/shots.sh /tmp/shots [extra render.py args]
set -euo pipefail
OUT=${1:?output directory}; shift || true
HERE=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO=$(cd -- "$HERE/../.." && pwd)
WORK=${OMABOOT_HARNESS_WORK:-$REPO/.harness}
mkdir -p "$OUT"
# The filtered lines are what a reader wants; the exit status is render.py's
# own, so an interpreter that cannot start (no PySide6, say) fails the run
# instead of leaving yesterday's pictures in place under a green exit.
render() {
  local log code=0
  log=$(python3 "$HERE/render.py" --work "$WORK" --out "$OUT/$1.png" "${@:2}" ${RENDER_ARGS:-} 2>&1) || code=$?
  echo "$log" | grep -v "^qt: qml:" | grep -i "wrote\|error\|warning" || true
  if [ "$code" -ne 0 ]; then
    if echo "$log" | grep -q "No module named 'PySide6'"; then
      echo "FAIL: render.py needs PySide6 and $(command -v python3) has none; install it (pip install PySide6, in a venv first on PATH) and run again"
    else
      echo "FAIL: render.py exited with code $code on $1; the log is above"
    fi
    exit "$code"
  fi
}
render stacked-system   --size 1100x760 --select now
render stacked-theme    --size 1100x760 --select matte
render stacked-login    --size 1100x760 --select matte --screen login
render columns-system   --size 1600x1000 --select now
render columns-theme    --size 1600x1000 --select matte
render columns-login    --size 1600x1000 --select matte --screen login
render dryrun           --size 1100x760 --select matte --script 'start("dryrun")' --settle 2
render new-dialog       --size 1100x760 --select now --script 'newDialog.source = "current"; newDialog.openWith("")'
render colour-popup     --size 1600x1000 --select matte --script 'colourPopup.openFor("colors.accent", "#e68e0d")'

# Lock Screen Explorer's boot screen set to something other than stock: its
# state file in the fake home says `follow`, and the engine reads that as an
# override. The system entry must then carry it as a fact under the unlock
# screen and as a warning in the footer, and a dry run must report it as a
# problem on the validate step. The file is removed again on the way out so
# no later render, of this run or the next, sees it.
OVERRIDE="$WORK/home/.local/state/omarchy/lock-explorer-boot"
mkdir -p "$(dirname "$OVERRIDE")"
trap 'rm -f "$OVERRIDE"' EXIT
echo follow > "$OVERRIDE"
render override-stacked --size 1100x760 --select now --expect-status "boot and shutdown: omarchy"
render override-columns --size 1600x1000 --select now --expect-status "boot and shutdown: omarchy"
render override-dryrun  --size 1100x760 --select matte --script 'start("dryrun")' --settle 2
rm -f "$OVERRIDE"
