#!/bin/bash
# Render the views a design pass looks at, into one directory.
#   plugin/harness/shots.sh /tmp/shots [extra render.py args]
set -euo pipefail
OUT=${1:?output directory}; shift || true
HERE=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
mkdir -p "$OUT"
render() { python3 "$HERE/render.py" --out "$OUT/$1.png" "${@:2}" ${RENDER_ARGS:-} 2>&1 | grep -v "^qt: qml:" | grep -i "wrote\|error\|warning" || true; }
render stacked-system   --size 1100x760 --select now
render stacked-theme    --size 1100x760 --select matte
render stacked-login    --size 1100x760 --select matte --screen login
render columns-system   --size 1600x1000 --select now
render columns-theme    --size 1600x1000 --select matte
render columns-login    --size 1600x1000 --select matte --screen login
render dryrun           --size 1100x760 --select matte --script 'start("dryrun")' --settle 2
render new-dialog       --size 1100x760 --select now --script 'newDialog.source = "current"; newDialog.openWith("")'
render colour-popup     --size 1600x1000 --select matte --script 'colourPopup.openFor("colors.accent", "#e68e0d")'
