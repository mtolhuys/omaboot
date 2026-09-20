#!/bin/bash
# The generated app must follow an Omarchy theme switch by itself, since the
# shell's applyTheme IPC never reaches it. Generate the app into the harness
# home, open it, switch the current-theme link under it, and expect the
# window's colours to change within the probe's interval.
#   OMARCHY_PATH=/usr/share/omarchy plugin/harness/theme-follow.sh
set -euo pipefail
HERE=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO=$(cd -- "$HERE/../.." && pwd)
WORK=${OMABOOT_HARNESS_WORK:-$REPO/.harness}
OUT=${1:-/tmp/omaboot-theme-follow}
mkdir -p "$OUT"
HOME_DIR="$WORK/home"
SHELL_DIR=${OMABOOT_SHELL:-${OMARCHY_PATH:?}/shell}

# A home the way render.py builds it, with the shell where app.rs looks.
python3 "$HERE/render.py" --work "$WORK" --out "$OUT/warmup.png" --select now ${RENDER_ARGS:-} >/dev/null 2>&1 || true
mkdir -p "$HOME_DIR/.local/share/omarchy"
ln -sfn "$SHELL_DIR" "$HOME_DIR/.local/share/omarchy/shell"
# Every directory the engine writes to points into the fake home: without
# XDG_DATA_HOME the desktop entry and the icons land in the real
# ~/.local/share, and a later `app uninstall` takes the real ones away.
HOME="$HOME_DIR" XDG_CONFIG_HOME="$HOME_DIR/.config" XDG_STATE_HOME="$HOME_DIR/.local/state" \
  XDG_DATA_HOME="$HOME_DIR/.local/share" OMABOOT_PLUGIN_DIR="$REPO/plugin" \
  "$REPO/target/release/omaboot" app install >/dev/null

LINK="$HOME_DIR/.local/state/omarchy/current/theme"
FROM=$(readlink -f "$LINK")
TO="$WORK/themes/catppuccin-latte"
if [ ! -d "$TO" ]; then
  mkdir -p "$WORK/themes"; cp -r "${OMARCHY_PATH}/themes/catppuccin-latte" "$TO"
fi
trap 'ln -sfn "$FROM" "$LINK"' EXIT

LOG=$(python3 "$HERE/render.py" --work "$WORK" --entry "$HOME_DIR/.config/omaboot/app/shell.qml" \
  --out "$OUT/after.png" --select now --script-pause 3 ${RENDER_ARGS:-} \
  --script 'console.log("before", Color.background)' \
  --script "Quickshell.execDetached([\"ln\", \"-sfn\", \"$TO\", \"$LINK\"])" \
  --script 'console.log("after", Color.background)' 2>&1)
echo "$LOG" | grep -E "before|after|error|Error" || true
BEFORE=$(echo "$LOG" | sed -n 's/.*before \(#[0-9a-f]*\).*/\1/p' | head -1)
AFTER=$(echo "$LOG" | sed -n 's/.*after \(#[0-9a-f]*\).*/\1/p' | head -1)
if [ -z "$BEFORE" ] || [ -z "$AFTER" ] || [ "$BEFORE" = "$AFTER" ]; then
  echo "FAIL: the app did not follow the theme switch (before=$BEFORE after=$AFTER)"; exit 1
fi
echo "the app followed the theme switch: $BEFORE -> $AFTER; picture in $OUT/after.png"
