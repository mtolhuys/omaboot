# omaboot: the window

This document is not decoration. In this community the interface *is* the
product: people install a tool because a screenshot of it looked good on
r/omarchy. A correct tool with a mediocre interface will be ignored. Treat every
rule here as a requirement, not a suggestion.

Stack: a Quattro shell plugin (`plugin/`, QML, loaded by `omarchy-shell` as a
`panel` in a Quickshell `FloatingWindow`), built from the shell's own widgets
(`qs.Ui`) and tokens (`qs.Commons`: `Color`, `Style`). The engine is the Rust
binary, driven with argv only and read as JSON (`omaboot --json`). The plugin
never opens a theme file and never runs a shell string.

## What is built, as of 19 September 2026

Built: the window with its three blocks (stacked by default, side by side on
a toggle that is remembered), the system entry that opens first,
your themes, the three screen tabs, the composite preview drawn by the engine,
drag and drop of a logo (PNG or SVG) and a wallpaper (JPG or PNG) onto the
picture, the properties column driven by the engine's field descriptions, the
colour picker with the active Omarchy theme's palette, the dropped wallpaper's
colours and every other Omarchy theme, the new-theme dialog (from what boots
now, from an Omarchy theme, or empty), rename and delete, "Show for real", the
password dialog, the apply, dry run, revert and reset flows with their step
list and full operation report, warnings in the footer, `Choose…` buttons
next to every image field (a file dialog run outside the shell process),
`Use Omarchy's login screen` and `Give the login screen back` on the system
entry, and `omaboot plugin install`.

Not yet, and each one is a note in `docs/DECISIONS.md`: streaming the
initramfs output while it runs, undo,
keyboard cursor navigation in the `PanelKeyCatcher` style, and thumbnails of
all three screens on the system entry.

An earlier terminal interface (ratatui) was built and dropped on 19 September
2026; `docs/DECISIONS.md` says why.

## The governing idea

**You are never editing a config file. You are looking at your boot screen while
you change it.** The preview is the centre of the window, always visible,
always current. No "now render the preview" step exists. Every change updates
the picture.

If a design question is ever unclear, the answer is whichever option keeps the
preview visible and truthful.

## The window

```
 omaboot   boot and shutdown: omarchy (default) · login: omarchy-onscreen-keyboard      [Reload] [Close]
 ┌ SYSTEM ────────────────┐ ┌ [Unlock] [Login] [Shutdown]              what boots now ┐ ┌ WHAT BOOTS NOW ────┐
 │ What boots now         │ │                                                         │ │ theme    omarchy   │
 │ boot and shutdown: …   │ │                                                         │ │ set in   /etc/…    │
 │ login: …               │ │                 ▓▓▓▓▓  LOGO  ▓▓▓▓▓                     │ │ owner    Omarchy's │
 │ [Make a theme from this]│ │                                                         │ │ styled by default  │
 │                        │ │                 🔒 ● ● ● ● ●                            │ │ background ▇ #1a1b26│
 │ YOUR THEMES            │ │                 ▁▁▁▁▁▁▁▁▁▁▁▁▁                          │ │ text       ▇ #c0caf5│
 │ ▸ kopie       ● boots  │ │                                                         │ │ logo     /usr/…    │
 │   matte-black          │ │  (drop a logo or wallpaper here)                        │ │                    │
 │                        │ └─────────────────────────────────────────────────────────┘ │ Nothing here is    │
 │ [+ New theme]          │  composite of the installed files, not the real render [Show for real] │ edited in place… │
 └────────────────────────┘                                                             │ [Revert the last apply] │
 status line · warnings                                                                 └────────────────────┘
```

Three blocks, fixed roles, in the shape every editor with a preview has:
sidebar, canvas, inspector. The sidebar is 280 px, the inspector 400 px, the
canvas takes the rest, and the picture keeps the screen's 16:9 inside it,
centred, with its caption and buttons attached under it. Narrower than
1180 px the same three blocks stack in the same order: the sidebar becomes a
strip of cards (the system card, one card per theme, the new-theme card; the
same list turned on its side), then the picture, then the inspector with each
field group as a column of its own, wrapping, so a group never breaks across
two. There is no layout switch: the width decides, and the widest window is
the best one. The choice is written through `omaboot prefs layout=…` and
read back with `status`, so it survives the window closing.

- **First.** The system entry, always first and selected on open, then your
  themes. The theme that is what boots now is marked with a glyph and a word,
  never a colour alone. `+ New theme` at the bottom.
- **Second.** The preview with the three screen tabs (also keys 1, 2, 3), the
  drop zone, the caption saying what fidelity this is, `Choose image…` and
  `Show for real`.
- **Third.** For the system entry: the facts, each with its source, the one
  sentence that says how to change it, and, when a third-party SDDM theme is
  what logs you in, `Use Omarchy's login screen` (or, once omaboot's drop-in
  is what decides it, `Give the login screen back`). For a theme: the
  properties of the selected tab only, no mega-form, with an image icon on
  every image field; rename and delete are icons in the section header, Dry
  run and Apply are buttons under the properties.

## Interaction rules

- Every action says what it touches, in its tooltip and again in the
  confirmation. The confirmation before an apply names the directories, the
  drop-in, the initramfs rebuild and the password prompt.
- Nothing on the system entry is editable. The way to change what boots is
  `Make a theme from this`, then change, then apply.
- A change is saved the moment it is made (the engine validates and writes
  `theme.toml`); there is no Save button and no unsaved state to lose. Saves
  go one at a time: the engine reads, changes and rewrites the file, so two
  in flight would race and the later one could undo the earlier. Edits made
  while a save runs wait and go out together; an image drop or a rename
  waits its turn the same way. A field that loses focus without changing
  ("0.50" over "0.5", "#D35F5F" over "#d35f5f") is not saved at all.
- The logo's size and place are per screen. The Unlock tab sets the default
  (`[logo]`); the Login tab shows Width for the login screen and the Shutdown
  tab Width, Position and Offset for the shutdown screen, each following
  unlock until it is given its own value. Width is the logo's share of the
  screen width, so the slider means the same thing whatever the size of the
  file that was dropped in and on every display.
- Drop a PNG or SVG on the picture for the logo; on the Shutdown tab it is the
  shutdown logo; on the Login tab a JPG or PNG is the wallpaper. Anything else
  is refused by the engine with a sentence, and the source file is never
  modified. The image icon beside a field, or `Logo` / `Wallpaper` under the
  picture, opens a file dialog filtered to the supported images, in `~/Pictures` the
  first time and after that wherever the last image came from; the dialog
  is a separate process (`zenity`), never a dialog inside the shell.
- Colour fields open a picker that offers, in this order: the palette of the
  Omarchy theme the desktop is on, the colours of the dropped wallpaper, every
  other Omarchy theme, then a hex field.
- Escape closes the topmost overlay, then the window. Closing the window tells
  the shell (`shell.hide`), so `toggle` stays consistent.
- Without the engine (nothing at `~/.local/bin/omaboot` and no `omaboot` on
  `PATH`, which is what a fresh machine and the marketplace lab look like)
  the window says so in the header, in the picture and in the footer, with
  the two commands that install it, and starts no process at all. The
  engine is looked for with one `test -x` per candidate before the first
  call; spawning a binary that is not there would be a Quickshell warning in
  the shell log, and a conformance run fails on any unexplained line.

## Preview: three fidelities

1. **In the window, always on.** Composited by the engine (`omaboot render`)
   into `$XDG_RUNTIME_DIR/omaboot`, one file per selection, screen and
   revision, so a stale answer can never be shown. `scripts/verify-render.sh`
   checks the composite against the real render.
2. **For real, on the button.** `omaboot preview`: `plymouthd` in an Xwayland
   window for unlock and shutdown, `sddm-greeter --test-mode` in a window for
   login. Guarded, timed out, always torn down.
3. **Applied.** After Apply, on the next boot.

The caption under the picture states which fidelity you are looking at. Never
let a user believe a composite is the real render.

## The apply flow is the showpiece

Applying takes sudo and an initramfs rebuild, so it is slow and it scares
people. It gets the whole window: a step list that fills in as the engine
reports each step, then, when it is done, every operation by verb and path,
grouped by step, scrollable. A dry run shows the same view and changes
nothing. On failure the view stops at the failing step and shows the engine's
sentence; a refused password reopens the password dialog with the reason.

Rules for this view:

- Every step names what it touches. No "Working...".
- On success, one line: `Done. Reboot to see it. Revert puts everything back.`
- Revert and Remove are on the system entry, with what they will do in their
  tooltips and confirmations.

## It wears the user's theme

The plugin draws with the shell's `Color` and `Style` singletons and `qs.Ui`
widgets, so it is themed by construction and follows a theme switch live, the
way every first-party panel does. No colours are hard coded in the plugin
except the swatches that show a theme's own colours.

The mark in the header is omaboot's own (`brand/`, `plugin/Mark.qml`): an
open padlock on a 16-cell grid, drawn as rectangles in the accent colour, so
it is on the theme like everything else. All other icons are Nerd Font
glyphs, the way the first-party panels draw theirs, and
live in `plugin/icons.js` by meaning (`Icons.apply`, `Icons.login`), so one
action shows the same sign on its button, in its confirmation and in the run
view. Every screen has one glyph (lock, login, power) that marks its tab, its
line on the system card and its facts header. Actions that are safe and
frequent (layout, reload, close, rename, delete, pick an image) are icon-only
`PanelActionButton`s with a tooltip; actions that change the system keep
their words next to the glyph, because a rocket alone does not say what it
launches. Text under the picture is one line; the rest is in tooltips.
That line says the fidelity first ("composite") and the drop hint after
it; when the buttons leave no room for both, which the stacked layout does
since the picture there is sized by the height, the hint goes first, then
the rest of the sentence, and the whole of it is in the line's tooltip. It
is never cut mid-sentence. A path in the facts keeps its start and its file
name and loses its middle, with the full path in its tooltip; nothing
there wraps mid-word.

## Anti-patterns, explicitly banned

- Emoji as UI chrome.
- Loading spinners with no subject: the header says what the engine is doing.
- Any confirmation prompt that does not say what will change.
- Any file the plugin reads or writes itself. If the plugin needs something,
  the engine gets a subcommand and a test.
- A shell string anywhere in the plugin. Argv only.

## Seeing it without a shell

`plugin/harness/render.py` draws the window to a PNG with no shell, no
display and no root: it supplies the few Quickshell types the plugin uses
(Process on QProcess, FileView, the `Quickshell` singleton) in Python, loads
the real `qs.Commons` and `qs.Ui` from an Omarchy tree, points the engine at
a sandbox prefix with a fake home, and grabs the window offscreen. The
widgets, tokens, font and engine are the real ones; only Hyprland's rounding
and gaps are missing. `plugin/harness/shots.sh <dir>` renders the set a
design pass looks at (wide and narrow, system and theme, the three screens,
the dry run, the dialogs), and `plugin/harness/exercise.sh` drives the
editing flows through the window (a burst of edits, a per-screen logo width,
the file dialog's directory before and after a pick, an
image drop, a wallpaper, a rename, a new theme, a delete, a dry run) and
checks what landed in `theme.toml`; it ends by taking the engine away
(`render.py --no-engine`) and checking that the window shows the
not-installed state and spawns nothing. `plugin/harness/theme-follow.sh` opens
the generated app (`omaboot app`) the same way, repoints the current-theme
link under it and expects the window's colours to change. It needs PySide6, a Nerd Font that fontconfig gives
for `monospace`, `target/release/omaboot` and `OMARCHY_PATH`. Everything
the harness makes (the fake home, the sandbox prefix, the runtime
directory, the `qs` import links) lives in `.harness/` at the repository
root, git-ignored and outside `plugin/`: the shell watches the linked
plugin directory, and a work directory inside it made the shell reload the
plugin on every harness run. The wrapper the harness writes exports
`HOME`, `XDG_CONFIG_HOME` and `XDG_STATE_HOME` into that fake home, and
`exercise.sh` checks on the way out that the caller's own
`~/.config/omaboot` is exactly as it found it. This is how
the layouts above were checked before they reached a real shell; a real
shell is still the last word, because the harness cannot see a Quickshell
crash.

## Screenshot test

Before v1 ships, open the window, take one screenshot, and ask: would this get
upvotes with no caption? If not, the design is not finished. Ship the project
with a GIF of the preview updating while a colour is picked. That GIF is the
marketing.
