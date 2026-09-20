# Decisions

Choices made while building M1 that the specification did not pin down. Each
one states what was decided, why, and what would make it worth revisiting.
Anything here that a later milestone overturns should be edited rather than
appended to, so this file stays a description of the code and not a diary.

Written on 18 September 2026, against Omarchy `4.0.0.alpha`.

## Structure

**Two crates in one workspace, and the helper shares no code with the
library.** `omaboot-apply` depends on `clap` and `anyhow` only, and repeats the
handful of constants it needs (the destination directories, the size ceiling,
the Omarchy-owned paths) rather than importing them. A shared crate would be
less repetition and more reading for anyone auditing the privileged binary.
The repetition is small, the constants are stable, and `CLAUDE.md` asks for a
helper that can be audited in one sitting.

**The library holds everything; the binaries are thin.** `omaboot` is a CLI
today and gets a TUI in M3. Both are front ends over `omaboot::cli::run` and
`omaboot::apply::Pipeline`, so the TUI cannot become a second implementation of
the pipeline.

**Module seams:** `theme` (model, strict parse, validation), `generate`
(templates to bytes, pure), `apply` (the pipeline and its operations), `state`
(what is applied, what to go back to), plus `paths`, `exec`, `hash` and `error`
as the shared plumbing. `preview` and `tui` join them in M2 and M3.

## Dependencies

Beyond the agreed core (`clap`, `serde`, `toml`, `anyhow`/`thiserror`):

- `sha2`, for the theme hash in the rollback record and the per-file digests
  the verify step and `omaboot status` compare against. Byte comparison would
  work for verify but not for reporting drift cheaply.
- `tempfile`, for the temporary worlds every test runs in.

`thiserror` is used in the library and `anyhow` in the helper. The library's
errors are matched on and each carries a suggested next action, which wants a
type; the helper only ever prints its error and exits, which does not.

No date or time crate. Timestamps are Unix seconds, and ages are rendered
coarsely ("just now", "3h ago", "2d ago"). The state file is read by omaboot,
not by a person, and the exact second is in it if anyone wants it.

## `--root` and `--dry-run`

**`--root` prefixes system paths only.** `/usr/share/...`, `/etc/...` and
`/etc/plymouth/plymouthd.conf` move under the prefix; the theme directory and
the state directory keep following `HOME` and the XDG variables. A test points
`HOME` at one temporary directory and `--root` at another and is then fully
hermetic. Prefixing the user directories as well would have produced paths like
`<prefix>/home/you/.config`, which reads like a bug in the output.

**Under a prefix the Omarchy tree is `<prefix>/usr/share/omarchy`, and
`OMARCHY_PATH` is not consulted.** Omarchy exports `OMARCHY_PATH` in every
login session and `omarchy dev link` points it at a checkout, so the first
version, which let the variable win, read the host's themes and logo inside
what was meant to be a sandbox: twelve tests failed in an ordinary Omarchy
shell and passed in `env -u OMARCHY_PATH`. A sandboxed run now reads nothing
outside its prefix, and `cargo test --workspace` passes with the variable set
or unset (`omarchy::tests::a_prefix_wins_over_omarchy_path_in_the_environment`).
Without a prefix the variable still wins over `/usr/share/omarchy`, the way
every Omarchy script reads it.

**A prefixed run never executes a real system command.** `omaboot` swaps in a
runner that reports success without spawning anything, so
`plymouth-set-default-theme` and `mkinitcpio` cannot run against a prefix by
accident. Setting the default theme under a prefix rewrites the prefixed
`plymouthd.conf` directly instead.

**A prefixed run installs in-process rather than through the helper.** Calling
`sudo omaboot-apply` from a test would need root. The helper's own publishing
logic is tested in its own crate against its own prefix, so both halves are
covered without either needing privilege.

**`--dry-run` performs no operation but still runs every check.** Validation
and generation really happen in a dry run: they are pure. The destination guard
also runs, so a plan that would touch an Omarchy directory fails instead of
being printed as though it were acceptable. Missing tools are reported as
problems at the end of the plan rather than as a hard error, because listing
what would happen is still useful on a machine that cannot do it.

## theme.toml

The specification sketched the format; these are the details it left open.

- `[meta] name` is required. `author` defaults to empty and `version` to
  `0.1.0`. A version must be `major.minor.patch`, optionally with a `-pre` or
  `+build` suffix.
- Colours must be `#rrggbb`. `#rgb` and colour names are refused: both Plymouth
  and QML need the long form, and expanding a short form is a guess about what
  the author saw.
- `logo.width` is limited to 0.02 to 0.95 of the screen width and `logo.offset` to plus or minus 4000
  pixels per axis. Outside those, the logo is invisible or off screen, which is
  a typo rather than an intention.
- `logo.offset` is only accepted with `position = "custom"`. Silently ignoring
  an offset because the position is `center` is the kind of quiet no-op this
  format is supposed to avoid.
- Messages are limited to 120 characters and may contain no control character.
  A boot screen is the wrong place to find out that a newline ended up in a
  message.
- `shutdown.logo = "inherit"` reuses the unlock logo; any other value is a file
  name in the theme directory, installed as `logo-shutdown.png`.
- `login.background = "image"` or `"blur"` requires `background.png` in the
  theme directory. There is no key for the file name: one optional background
  per theme is enough, and a second path to validate is not.
- Asset references are relative, have no `..` and no leading `/`, must be
  regular files, may not be symlinks, and must be between 1 byte and 64 MiB.
  The ceiling is the one `omarchy-plymouth-set` passes into its privileged
  section, so a theme that omaboot accepts is a theme the Omarchy publisher
  would also accept.
- **An SVG logo is accepted and rasterised during generation**, with `resvg`,
  at its own size clamped to between 16 and 2048 pixels wide so a logo that
  declares 8000 pixels does not become an 8000 pixel PNG in the initramfs. The
  validator does not parse it: that would mean parsing it twice, so a malformed
  SVG is reported by the generate step, which names the file.

## Scaffolding from an Omarchy theme

`omaboot new <name> --from-omarchy-theme <theme>` exists because a theme that
scaffolds without a logo cannot be applied, not even as a dry run, so the first
command a new user runs used to end in an error about a missing `logo.png`.
The flag is in the command table in `docs/SPEC.md`; this is what it does.

- **It takes four colours and one image.** `background` and `foreground` from
  the theme's `colors.toml`, `accent` if it has one, and `red` as the error
  colour. Omarchy themes have no `error` key; `red` is what every one of them
  uses for this, and it is the closest thing to the `#f7768e` that
  `omarchy-plymouth-set` hard codes for failed-login assets. The logo is the
  theme's `unlock.png`, which is the image
  `omarchy-plymouth-set-by-theme` installs as the boot logo.
- **A user copy wins over the packaged one**, which is what
  `omarchy-theme-dir` does, so a theme someone customised in
  `~/.config/omarchy/themes` is the one omaboot reads.
- **Omarchy's `colors.toml` is parsed permissively.** Unknown keys are
  ignored, which is the opposite of how omaboot treats its own `theme.toml`.
  The file is Omarchy's, it carries about two dozen keys today, and refusing a
  key that appears in a future Omarchy release would turn an upstream update
  into an omaboot bug. The four keys omaboot does read are validated as
  `#rrggbb`, because a colour that is not can not be written into a Plymouth
  script.
- **Nothing is written until everything resolves.** The palette and the image
  are read before the theme directory is created, so a wrong theme name leaves
  no half-scaffolded directory behind.

## Generation

**Everything is generated from a template with values injected as data.** No
`sed` over a shipped script, which is what `docs/ARCHITECTURE.md` asks for. The
renderer refuses to return a string that still contains a placeholder, and a
value that itself contains a placeholder is caught too, so a theme cannot
smuggle a slot into the output.

**Escaping refuses rather than transforms.** Quotes and backslashes are escaped
for Plymouth, QML and desktop files; control characters are an error. The
validator already rejects them, and the generator refusing them again is the
cheap half of defence in depth.

**Glyph assets come from the theme first, then from the packaged Omarchy
tree.** `bullet.png`, `entry.png`, `lock.png`, `progress_bar.png` and
`progress_box.png` are read from `$OMARCHY_PATH/default/plymouth` when the
theme does not ship its own, and the SDDM set from
`$OMARCHY_PATH/default/sddm/omarchy`. `entry-failed.png` and `lock-failed.png`
fall back to their normal variants when neither source has them. The packaged
tree is read, never written.

**Glyph assets are recoloured in process, not with ImageMagick.** Upstream
runs `magick -channel RGB +level-colors "#c","#c"`, which maps every input
level onto one colour and leaves alpha alone. That is a loop over the pixels,
so omaboot does it with the `image` crate: one runtime dependency fewer, and a
result that a unit test can read back. The set is the one upstream recolours,
`bullet`, `entry`, `lock` and `progress_bar`, in the theme's foreground; the
greeter's `entry-failed` and `lock-failed` take the theme's error colour, where
upstream hard codes `#f7768e`.

**`preview-unlock.png` is not generated.** It exists so
`omarchy-plymouth-switcher` can show a thumbnail of an Omarchy theme. omaboot
has its own preview story in M2 and does not install into the Omarchy theme
directory, so producing that file now would be producing it for nobody.

**The greeter uses the same prompt style as the unlock screen.** `[login]` has
no `prompt` key in the specification, and two different prompt styles on two
screens the same person sees seconds apart would read as a bug. `unlock.prompt`
therefore drives both.

**`progress = "spinner"` is a pulse, not a rotation.** The Plymouth script
language animates by swapping images, and this theme ships no frame set to
rotate. The spinner fades the progress box in and out from the refresh
callback. Generating a rotated frame set is now possible and is worth doing
when the live preview can show whether it reads as a spinner at boot.

**`login.background = "blur"` is the image plus a scrim in the background
colour.** A true blur needs `Qt5Compat.GraphicalEffects` or `MultiEffect`,
neither of which is guaranteed on an Omarchy greeter, and a greeter that fails
to load its effect module is exactly the failure this project exists to
prevent. When M2 can render the real greeter and prove the module is there,
this can become a real blur.

**Two deliberate improvements on the stock script.** The generated Plymouth
script re-runs its layout when the window size changes, which the installed
`omarchy.script` does not do, so a display that appears after `plymouthd`
started is handled. And message text is drawn in the theme's foreground colour
rather than the hard coded white of `Image.Text(text, 1, 1, 1)`.

## The image pipeline and the preview

**One set of layout numbers, used by both renders.** The gap between logo and
entry, the bullet size and pitch, the lock's height ratio, the text inset: all
of them are constants in `render::layout`, injected into the Plymouth script as
template values and used directly by the compositor. Neither side carries a
literal of its own, so the picture omaboot draws and the picture Plymouth draws
cannot drift apart because someone changed one of them. `docs/UI.md` asks for a
test that compares the composite against a real `plymouthd` screenshot; that
test still belongs to the live preview, but the thing it would catch is now
mostly prevented by construction.

**The compositor draws from the generated theme, not from the theme
directory.** It decodes the same bytes the install would publish, so a preview
cannot show a colour or an asset that an apply would not produce.

**The greeter's layout is mirrored, not driven.** QML lays itself out at
runtime, so `render::layout::login` reproduces what the generated `Main.qml`
does: a centred column, the logo capped at 80% of the screen width exactly as
the QML caps it, then a row of lock and entry. This is the one place where the
two can drift, and it is why the live greeter preview still matters.

**Text is drawn through resvg.** There is no text in `image`, and a preview
that silently drops the shutdown message would be a preview of something else.
A one-line SVG is rasterised with the system fonts; on a machine with no fonts
the call returns nothing and the preview is drawn without its caption rather
than failing.

**`omaboot render <theme> --out <file>.png` exists** so the composite can be
looked at, tested and put in a README before the TUI can display it. It touches
no system state, takes no prefix, and is the same call the TUI will make.

## The terminal interface (dropped, see below)

**The inline preview is the kitty graphics protocol where the terminal
speaks it, and half blocks everywhere else.** Ghostty, kitty and WezTerm are
recognised from `TERM` and `TERM_PROGRAM`; `OMABOOT_GRAPHICS=kitty|blocks`
overrides the guess, and inside tmux the guess is blocks, because passthrough
is off by default there. The protocol is implemented in `tui::graphics`
without a dependency: a PNG in base64 chunks, placed at the pane and scaled
into its cells at a z-index below text, so ratatui's characters stay legible
over it. The image is transmitted once per edit and placed again only when the
pane moves; it is taken down while an overlay is open, because it floats above
cell backgrounds. The half-block fallback draws two pixels per cell in 24 bit
colour into ratatui's buffer, which is what the drawing tests read back.
`chafa` is not used: the half-block renderer is what it would have produced,
without the process.

**The preview is composited at 1920x1080 and scaled into the pane**, and the
caption says so. Compositing at the pane's own pixel size would mean laying
out a 90x40 pixel screen, where a logo is wider than the display and the
layout means nothing.

**The generated theme is cached per revision.** Every keystroke that changes a
value bumps a revision counter; generation, which decodes and recolours every
asset, runs once per revision, and the composite once per revision and screen.
That is what keeps the picture live without regenerating a theme sixty times a
second.

**The preview follows unsaved edits.** It generates from the in-memory
manifest with the theme directory's assets, so what is on screen is what an
apply would install, including the edit that has not been saved yet.

**The apply runs on a thread and reports through a channel.** The pipeline
gained an `Observer`, which the CLI ignores and the TUI turns into the step
list filling in with elapsed times. The UI keeps its keys while a step takes a
minute. Streaming the initramfs output into the pane needs a runner that
reports lines as they arrive; the pane has the space reserved for it.

**A dry run leaves unsaved edits out, and says so.** A dry run that quietly
wrote `theme.toml` first would be changing something, which is the one thing
it promises not to do. Apply saves first, and says that too.

**Every confirmation lists what it would change**, which `docs/UI.md` requires
of any confirmation: the two directories, the drop-in, the initramfs rebuild,
and the promise about the directories Omarchy owns.

**Making a theme happens in the app, and it is the first thing offered.**
A fresh machine has no omaboot themes, so opening on an empty list with an
instruction to quit and run a command was a wall rather than an application.
Starting omaboot with no themes opens the create flow: a name, then a list of
the Omarchy themes on this machine to borrow colours and a logo from, with an
empty theme as the last option. Three keystrokes and there is something on
screen. `n` opens the same flow later, `x` removes a theme after a question
that names the directory, and `omaboot new` still does the same thing from a
shell, through the shared `scaffold` module so the two cannot drift.

**A logo is picked, not typed.** The logo fields offer the images that are
actually in the theme directory, and the shutdown logo offers `inherit`
alongside them, so nobody has to guess a file name. The text editor still
opens on `space` for a name that is not there yet.

**A prefixed run says so in the header.** `sandbox /tmp/...` in the accent
colour, because the difference between a run that can change the machine and
one that cannot should be visible before pressing `a`, not discovered
after.

**Below 100 columns the inspector becomes an overlay on `i`, and below 80 the
theme list folds into the header**, as the responsive rules ask. The preview
keeps at least ten rows.

**A panic restores the terminal and writes `panic.log` into the state
directory**, then prints where it is, so a crash never leaves someone with a
broken terminal and no message.

## The live preview

**It is the real daemons.** `omaboot preview <theme> --screen login` runs the
greeter in `--test-mode` on the staged theme, which opens as a window in the
session and needs no privilege. `--screen unlock` and `--screen shutdown` run
`plymouthd` itself with its X11 renderer and drive it with the `plymouth`
client: `show-splash`, then `ask-for-password` for the unlock screen, so the
prompt, the bullets and the progress bar that follows are all the real thing.
The `p` key in the TUI does the same, handing the terminal back for the
duration because sudo may ask a question and the daemons draw in their own
windows.

**The preview theme lives under `/run/plymouth/themes`, and plymouthd is
pointed at it by name.** plymouthd searches `/run/plymouth/themes` before
`/usr/share/plymouth/themes`, and a `plymouth.splash=<name>` option on its
(fake, `--kernel-command-line`) kernel command line overrides the theme in
`plymouthd.conf`. So the privileged helper publishes the staged theme as
`omaboot-preview` under `/run`, a tmpfs a reboot empties, and nothing under
`/usr/share` or `/etc` is read differently, let alone written. The helper gained
`preview install` and `preview remove` for this, with the same fixed manifest
and destination discipline as `install`.

**plymouthd gets a terminal of its own through `script(1)`.** It refuses to
run without a tty and takes over the one it is given, so it is started inside
`script -qfec ... /dev/null`, which allocates a pty for it, and keyboard input
reaches it through the X window, not the tty. `DISPLAY` is passed inside that
command line rather than through sudo's environment, which sudoers may not
preserve.

**On Wayland, a rootful Xwayland window is the display.** With `DISPLAY` set
that display is used. Without one, and with `WAYLAND_DISPLAY` present,
`Xwayland :<free> -ac -geometry 1920x1080 -decorate -noreset` is started for
the preview and stopped after it, its socket and lock removed. `-ac` turns off
access control on that throwaway server so root's plymouthd can connect to a
display the user started; the server exists for seconds and shows one splash,
which is the whole of its exposure. Both paths are verified in a headless
Weston in the container that builds this. `xhost` is not needed.

**Everything started is stopped by a guard, however the preview ends.** The
Xwayland server, plymouthd, the password client, the greeter, and the theme
under `/run` are each owned by a value whose drop kills or removes them, so an
error halfway leaves nothing behind.

**`--seconds` and `--screenshot`.** A preview runs until the greeter window
closes or the password is answered and the bar has run, or for `--seconds`,
which the CLI shortens with Enter. `--screenshot <file>` saves the X display
with ImageMagick's `import` while the splash is up, and a second file after
the password, which is how `scripts/verify-render.sh` and `docs/evidence` are
made.

## What the real renders proved

`scripts/verify-render.sh` runs the whole thing headlessly: Xvfb, a real
plymouthd with the label plugin, a real `sddm-greeter`, `xdotool` typing the
password, screenshots of every state, and a pixel comparison with what
`omaboot render` composites. The screenshots in `docs/evidence` come from it.
It found three things the unit tests could not:

- Plymouth's `Image.Text` draws nothing without the label plugin
  (`label-pango.so`). Omarchy ships it; a machine without it shows no message
  and no typed text, silently.
- A method call on a sprite that does not exist yet aborts the whole callback
  it happens in. The typed-text sprite for the asterisks and counter prompts
  was created lazily and the first `display_normal` callback tripped over it,
  so those two prompt styles drew nothing. Every sprite is now created at
  load, like upstream does.
- The unlock message was showing before the prompt and after it. It now
  appears with the password dialog and leaves with it; the shutdown message
  stays for the whole shutdown.

The composite and the real render agree to within a third of a percent of the
pixels for both screens, which is text antialiasing and a pixel of offset. The
comparison fails the script above two percent.

## The pipeline

**Steps are lists of operations, and there is one executor.** A dry run prints
the operations; a real run performs them. There is no second code path, so what
`--dry-run` shows is what would happen.

**`--step <slug>` runs a subset**, which is how "every step must be
individually re-runnable" is exercised. Validation and generation always run:
every later step is described in terms of what they produce, and both are pure.

**The rollback point is written before the switch**, as step 7 of
`docs/ARCHITECTURE.md` requires, and it records the previous Plymouth theme and
whether the SDDM drop-in existed. The previous theme is read from
`/etc/plymouth/plymouthd.conf` rather than by running
`plymouth-set-default-theme`, so the record can be produced under a prefix and
in a dry run, where that tool must not run.

**A failure at or after the switch reverts automatically**, including a verify
mismatch, which `docs/ARCHITECTURE.md` asks for in step 10. If the revert fails
as well, the error carries the three rescue commands from the recovery section
verbatim.

**Revert leaves the installed theme directories in place.** Once nothing points
at them they are inert, and keeping them makes a re-apply cheap. `omaboot
reset` is the command that removes them, which matches what the two commands
promise in `docs/SPEC.md`.

**The greeter smoke test runs with `QT_QPA_PLATFORM=offscreen` and requires
the greeter to stay up.** The first draft gave it a 30 second timeout and
counted the timeout as a failure; the first real apply (20 September 2026,
`docs/evidence/apply-round-2026-09-20.md`, A1) showed that the greeter in
test mode never exits on its own, so that test could only pass a greeter
that had failed. Now the greeter gets a settling time (`greeter::SETTLE`,
10 seconds), has to be alive when it is up, is stopped there, and what it
wrote is read: a complaint about the theme fails the step, silence passes
it, and an exit before the time is up fails it whatever the exit code. The
verdict travels with the lines that show it. No code path skips the step,
as `CLAUDE.md` requires; the preview and the smoke test build their command
in the same function so the two cannot disagree again. That function sets
`QT_FORCE_STDERR_LOGGING=1` since the closing round of the same day
(`docs/evidence/closing-round-2026-09-20.md`, B1): on the installed SDDM
0.21.0 the greeter's log goes to journald unless stderr is a console, and
a scan of an empty pipe passed a theme with a syntax error. Reading the
journal instead was the alternative and was not taken: the variable is
Qt's documented switch, it makes the greeter's words travel with the
verdict in the same process, and it needs no journal access.

## The privileged helper

**Interface:** `omaboot-apply install --staged <dir>`, `omaboot-apply switch
--on|--off`, `omaboot-apply remove`. Three verbs, no theme parsing, no path
from the user beyond the staging directory, which must be absolute, must not be
a symlink, and must equal its own canonical form.

**The drop-in is written by the helper, not by omaboot.**
`/etc/sddm.conf.d/zz-omaboot.conf` needs privilege, and every privileged write
belongs in the audited binary. Its contents are a constant in that binary, so
nothing about them is under the caller's control.

**`plymouth-set-default-theme` and `mkinitcpio` are invoked through `sudo`,
like upstream does**, rather than being folded into the helper. They are
packaged tools with their own behaviour, and wrapping them would mean the
helper grows an interface to pass their arguments, which is the thing the
helper is supposed not to have.

**Authorisation is `sudo`, not polkit, in M1.** `docs/ARCHITECTURE.md` says one
polkit prompt for the whole operation; `omarchy-plymouth-set` itself uses
`sudo`. M1 does what upstream does, with a single `sudo -v` first so there is
one prompt rather than one per command. A polkit policy file is a packaging
decision and belongs with the AUR package in M4; the step is already named
"Authorise" so it can change without the pipeline changing.

**An unexpected file in the staging directory refuses the whole transaction.**
The helper publishes a fixed list of names. Anything else is either a bug in
omaboot or somebody else's idea, and skipping it silently would make the second
case invisible.

**Files that the current theme no longer has are pruned** from the omaboot
directories after publishing, so a shutdown logo from a previous apply does not
linger. Only regular files directly inside omaboot's own two directories are
ever removed.

**Modes are 0644 for files and 0755 for directories**, and the helper does not
`chown`: running as root, the files it creates are root-owned already.

**The effective uid is read from `/proc/self/status`.** There is no safe std
API for it, and `no unsafe` is a hard rule, so a three-line parse of a file
Linux always has beats adding `libc`.

**Ownership checks apply in strict mode only.** On a real system the helper
walks each destination directory up to `/` and requires every one of them to be
root-owned and not group or world writable, mirroring
`omarchy-plymouth-set`. Under `--root` that check is skipped, because a prefix
inside a temporary directory is user-owned by construction and nothing outside
the prefix can be reached.

## What boots now

**The interface opens on the system, not on a saved theme.** The question
the tool answers first is "what are my boot, login and shutdown screens right
now", and until that is on screen a list of saved themes is a list of files.
So the list has an entry above your themes, read from the system every time
it is shown: `plymouthd.conf` for the Plymouth theme, `/etc/sddm.conf` and
every file in `/etc/sddm.conf.d` for the login theme, and the installed theme
directories for colours and logo. It is `system::Snapshot`, it writes nothing,
and it has no cache to go stale: after a run it is simply read again.

**The record of what omaboot applied is shown next to the system, never
instead of it.** A record the system no longer agrees with (something else
changed `plymouthd.conf`, another drop-in sorts after ours, the saved theme
was deleted) is a warning with the reason, not a fact. "This theme is
applied" is true only when the record and the system agree, and only then is
the theme marked in the list.

**The system entry is not edited in place.** The installed files are not a
theme of yours, and editing them would mean writing where omaboot promises
not to write. `e` on that entry makes a theme of yours from it, so the change
you want is: copy, edit, apply. That keeps every write on the one path that
is audited, dry-runnable and reversible.

**A derived theme is read from the installed files, except when omaboot
installed them.** For Omarchy's theme: background from
`Window.SetBackgroundTopColor` in the installed script, text colour from the
most opaque pixel of the installed `bullet.png`, logo from the installed
`logo.png`, accent and error from the login theme's `theme.conf` when it has
them (the stock Omarchy `theme.conf` has none; the on-screen keyboard theme's
does), Omarchy's own layout choices for the rest (bullets, a bar while
booting, a bare logo while shutting down, a centred greeter without a clock).
When `plymouthd.conf` names `omaboot`, the saved theme the record points at is
copied instead, because it is the exact description of what is installed.
Which Omarchy theme styled the stock screen is found the way
`omarchy-plymouth-current` finds it: by comparing the installed logo with the
packaged default and every theme's `unlock.png`.

**A login theme omaboot does not know gets facts, not a picture.** The stock
Omarchy theme and omaboot's own are composited from their glyphs. Anything
else, such as `omarchy-onscreen-keyboard`, has a layout omaboot has never
read, and a composite would be a guess presented as a picture. The pane says
which theme it is, which file chose it, and that `p` shows the real greeter.

**Every action says what it touches.** The hint line changes with the
selection: on the system entry it offers `e` and `p` and never apply or
delete, because neither does anything there. The confirmation before an
apply lists the directories, the drop-in, the initramfs rebuild and what
boots now, and the dry run view lists every operation by verb and path,
scrollable, under the step list. "Wat doet wat" is answered on the screen
rather than in a manual.

## The SDDM drop-in name

**`zz-omaboot.conf`, not `90-omaboot.conf`.** SDDM reads `sddm.conf.d` in
file-name order and the last `[Theme] Current=` wins. The reference machine
has `99-omarchy-login.conf` from Omarchy and
`99-z-omarchy-onscreen-keyboard.conf` from a login theme, so `90-` would have
been silently overridden by both and the login screen would never have
changed. The first draft of `ARCHITECTURE.md` got this wrong; the name is
now a constant with the reason in its doc comment, and a test asserts the
sort order against exactly those names.

**Writing the drop-in is not enough, so the verify step resolves the theme.**
`Operation::VerifySddmTheme` reads the configuration the way SDDM does and
fails, naming the winning file, when omaboot's drop-in is not the one in
effect. A dry run shows the check; a real run that fails it reverts. The same
resolution feeds `omaboot status` and the interface, so the three never
disagree about which login theme is in effect.

## From a terminal interface to a Quattro plugin

**The terminal interface was dropped on 19 September 2026.** It worked, it
was tested, and it was the wrong shape for the job. The tasks people have
here are visual and file-shaped: drop a logo, drop a wallpaper, pick a colour
from a palette, look at a picture. A terminal can show a picture only where
a graphics protocol happens to be available, cannot take a drag and drop, and
makes every file a typed path. The keyboard-first design that makes a TUI
good for text work made this one feel like a form. Omarchy's own desktop is
a Quickshell scene with a plugin host, first-party widgets and a theme
singleton, so the natural interface for an Omarchy tool is a plugin in that
scene.

**The engine stays; the interface is a view over `omaboot --json`.** Nothing
of the pipeline, the generation, the rendering or the system reading moved
into QML. The plugin makes argv-only calls (`Process` with a command list,
never a shell string) and reads one JSON object per line. What the plugin
needs that the engine did not have became subcommands with tests: `show`,
`set`, `rename`, `delete`, `add-image`, `palette`, `render --current`, and
JSON events for `apply`, `revert`, `reset` and `preview`. The rule for future
work: if the plugin needs something, the engine gets a command, the plugin
does not get a file read.

**`serde_json` was added.** It is the companion of `serde`, which the project
already depends on, and hand-writing JSON escaping to avoid it would be the
worse dependency. The `jpeg` and `webp` features of `image` were switched on
so a dropped photo can become a wallpaper; they are features of a crate
already in use, not new crates.

**Privilege from a window: one password, `sudo -S -v`, then `sudo -n`.**
`omarchy-shell` has a polkit agent, so `pkexec` would give a themed prompt,
but it prompts once per command and the pipeline runs several privileged
commands (install, switch, set the default, rebuild), separated by
unprivileged steps that must stay between them. Folding those into one
helper transaction would widen the helper's interface, which is the thing it
is designed not to have. So the plugin asks for the password in its own
dialog, hands it to the engine on stdin (`--password-stdin`), and the engine
turns it into a sudo ticket with `sudo -S -v` and drops the string. Every
later privileged command runs as `sudo -n`, so a missing ticket is an error
in one line rather than a process waiting forever for a prompt nobody can
see. Without a terminal, sudo keys the ticket on the parent process, and
every privileged command is a child of that one engine process, so one
prompt covers the whole run. The password never touches a file, an argument
or the environment. The first draft used `sudo -S -k -v`, and the first
real apply (20 September 2026, A2) stopped at step 5 with "a password is
required": with `-v`, `-k` makes sudo check the password and record
nothing, per sudo(8). The engine now also asks `sudo -n -v` right after
taking the ticket, so a sudoers policy that keeps no ticket is reported at
the dialog rather than five steps later.

**A theme without an `unlock.png` gets Omarchy's logo, not a refusal.**
The first "New theme" from the window on the reference machine picked the
owner's store-installed `catppuccin-dark`, which has no `unlock.png`, and the
dialog answered with an error and a command-line suggestion. Omarchy's own
switcher shows such a theme with Omarchy's default logo, so omaboot does the
same: the colours come from the theme, the logo from
`default/plymouth/logo.png`, and `new` says so in one sentence (`note` in the
JSON, a line on the command line). An `unlock.png` that is there but
unusable (a symlink, an empty file) is still refused: that is a mistake the
author should see. `status` marks each Omarchy theme with `has_logo` so the
dialog can say it before the click.

**An apply that would never be seen is refused, not performed.** Lock
Screen Explorer's boot screen, when set, puts its theme over the initramfs
at boot, so an omaboot apply under it would succeed at every step and change
nothing on screen. omaboot reads the plugin's own state file and refuses
before staging, naming the setting and the way out; a dry run reports it as
a problem and goes on. The alternative, applying and warning, leaves a user
with a green run and the wrong boot screen. SDDM's autologin is reported as
a fact (user and file) and nothing more: whether the greeter is skipped is
SDDM's decision at boot, and on the reference machine it was shown with
`autologin.conf` in place, so omaboot does not guess.

**The helper carries a protocol number and the engine checks it before
anything privileged.** The first real apply that reached step 10 (20
September 2026, A3) failed its verify because the helper in `~/.local/bin`
had been built two days earlier, when the drop-in was still called
`90-omaboot.conf`; `cargo build --release` had never rebuilt it, because the
workspace had `default-members = ["crates/omaboot"]`. Two things follow.
`default-members` is gone, so `cargo build --release` builds both binaries
(the interface is `cargo run -p omaboot`). And `crates/omaboot-apply/src/
protocol.rs` holds one number that both binaries compile in (the engine by
`#[path]`); the helper answers it to `omaboot-apply protocol`, unprivileged,
and every plan that runs the helper (apply, revert, reset, login stock and
release) asks first and refuses a helper that answers anything else, naming
the build command. `plugin install` runs the same check and warns. The
number is bumped with every change to the helper's verbs, manifests or
paths.

**Installation is three links, not a copy.** `omaboot plugin install` links
`plugin/` into `~/.config/omarchy/plugins/mtolhuys.omaboot` and both
binaries, `omaboot` and `omaboot-apply`, into `~/.local/bin`, then asks the
shell to rescan and enable. A link means the shell's hot reload picks up
QML edits and a rebuild is live at once, which is how the plugin is
developed; a packaged install can put the directory under
`/usr/share/omaboot/plugin` and the binary finds it there. The helper is
linked like the engine: `sudo` runs it by its path either way, and a copy
in `~/.local/bin` is no better protected than a link into the checkout,
so a copy would buy nothing and go stale at the next build. What
`install` does with a copy it finds there is the one nuance: a copy that
is byte for byte the built binary is kept, because replacing it changes
nothing that runs and the message `kept ... (identical copy)` says what
was seen; a copy that differs is removed and replaced by the link. The
test round of 19 September 2026 read that message as a deliberate copy of
the helper; it was a copy left by an earlier install, and the README now
says which it is.

**`omaboot` waits for the window, because a summon can be dropped.**
`omarchy-shell shell summon` answers "ok" the moment the shell has noted
the request, and a request noted while the shell is reloading its plugins
is cleared together with the panels and opens nothing. The shell reloads
after `plugin install` (the rescan and the enable), and again when its file
watcher sees the plugin directory change, so `omaboot` straight after
`omaboot plugin install` used to exit 0 with no window. After each summon
`plugin::open` now asks the window itself, `omarchy-shell shell call
mtolhuys.omaboot ping ""`, which the shell answers "unknown" until the
plugin is loaded and the plugin answers "open" or "closed" after; a summon
that has produced no window in two seconds is sent again, and after ten
seconds `omaboot` gives up and says the shell was still reloading. A shell
without `call` (older than the quattro branch of 19 September 2026) cannot
be asked and is trusted on its "ok". The waiting is a pure function over
two IPC calls and a clock, tested with a scripted shell.

**The plugin is not built on Omakit's runtime.** Omakit's own README marks
its runtime as unsupported scope for now, so the plugin uses the shell's
first-party widgets directly, the way the first-party panels do. Its shape
(argv-only processes, no file access, one manifest) is what a Passport
review would want to see, and nothing in it would resist being ported when
the runtime is supported again.

**Deleting a theme goes through the engine too.** The first draft had the
plugin run `rm -r` on the theme directory; it was the one place the plugin
touched a file, and one is too many. `omaboot delete <theme>` removes the
directory and says whether it was the theme that boots.

**No dialog runs inside the shell process.** The first file picker was a
`QtQuick.Dialogs` `FileDialog` inside the plugin. It opened the portal
dialog modally in `omarchy-shell` itself, the shell stopped responding, and
Quickshell crashed, taking the bar and every panel with it. The picker is now
`zenity --file-selection` in its own process, argv only, its answer read from
stdout. The rule that follows: anything that blocks (a dialog, a prompt, a
long command) runs outside the shell process; the plugin only ever waits on a
`Process`.

## The login screen when a third party owns it

On the machine this was built on, `99-z-omarchy-onscreen-keyboard.conf` makes
a third-party SDDM theme the login screen, and the stock `omarchy` theme is
only what the window says is Omarchy's default. Someone who wants Omarchy's
login screen back has two honest options: delete that third party's drop-in,
which is a file omaboot did not write and will not touch, or put a drop-in
after it. omaboot does the second. `omaboot login stock` (the `Use Omarchy's
login screen` button) writes omaboot's own `zz-omaboot.conf` with
`Current=omarchy` through the privileged helper (`switch --stock`), verifies
that SDDM now resolves to `omarchy`, and records nothing else: no applied
state, no rollback, because nothing of omaboot's was installed. `omaboot login
release` removes the drop-in again. Both are refused while a theme is applied,
since the drop-in is then the applied theme's and `revert` is the way back.
The window shows `login.by_omaboot` from `status --json` to pick which of the
two buttons to offer.

## One layout, decided by the width

Two layouts were tried: the three blocks side by side, then, at the owner's
request, stacked under each other with a switch top right and the choice
remembered through `omaboot prefs`. Rendered next to each other in the
harness (`docs/UI.md`, "Seeing it without a shell") neither was good: side
by side had the properties column running out of height and the picture
small; stacked had a picture floating in empty width and field groups
breaking across columns. The fix was not a third option but one layout done
properly, the shape every editor with a preview has: sidebar, canvas,
inspector, the picture at its 16:9 with its caption attached, Dry run and
Apply at the foot of the inspector. The stacked arrangement stays as what
happens below 1180 px, where three columns do not fit, and nothing else
chooses it. The switch is gone, since a switch is a sign that neither side
of it was finished. `omaboot prefs` remains as the way the window will keep
a preference when it has one worth keeping.

## The logo's size is a share of the screen

The first format sized the logo with `scale`, a multiplier of the file's
own pixels, the way Omarchy's script draws its 800 pixel logo at 800 pixels.
That broke twice at once: a 4000 pixel file dropped in was enormous at the
smallest scale the slider allowed, and the same theme was half the size on
a 4K display as on 1080p. `width` replaces it: the share of the screen's
width the logo takes, 0.42 by default because that is what Omarchy's own
logo is on a 1920 pixel screen. The Plymouth script scales the image in
`layout()` from `Window.GetWidth()`, the greeter from `root.width`, the
composite from its canvas, so all three agree. An older `scale` key still
parses, is ignored, and disappears on the next save.

## A window of the shell, or an app of its own

A Quattro plugin's window belongs to the shell process, and Quickshell gives
every window of a process one app id, so a dock shows omaboot under
Quickshell's id and icon, and "[2] omaboot" underneath. Quickshell 0.3 has an
`AppId` pragma, but it is per process, and setting it from a plugin would
rename the whole shell. So the same QML can also run in a Quickshell instance
of its own: `omaboot app` generates `~/.config/omaboot/app` (a `shell.qml`
with `//@ pragma AppId omaboot` that opens the window and quits when it
closes, links to the shell's `Commons` and `Ui`, links to the plugin's files)
and runs `qs -p` on it; `omaboot app install` adds the desktop entry and the
icons for that id. The plugin is kept: it is the integration the marketplace
reviews, and `omaboot` without arguments still summons it. The app is for
people who want it in the launcher and the dock with its own face. The
generated directory is rewritten before every launch, so it cannot drift
from the plugin, and it holds no file of its own worth editing. One thing
the shell gets for free that the app has to do itself: `omarchy-theme-set`
tells the shell about a switch over IPC (`applyTheme`), which never reaches
a second instance, so every two seconds the app reads `colors.toml` through
the `current/theme` link again and, when the text differs from what it
applied last, reloads it and `shell.toml` the way `applyTheme` would. That
catches the link being repointed as well as the file being edited, and it
is proven by `plugin/harness/theme-follow.sh`. The launcher icon is flat in omaboot's default accent, not white:
hicolor has no light and dark variants, and white vanished on light themes.

## What M1 deliberately does not do

`preview`, `doctor`, `export`, `import` and `follow` from `docs/SPEC.md` are
not implemented, and neither is the TUI. They belong to M2 to M4.

The delegation idea in `docs/UPSTREAM.md`, where a theme that only changes
colours and the logo is handed to `omarchy plymouth set`, is not implemented
either. It needs a way to recognise "this theme is only colours and a logo",
which is only meaningful once recolouring exists, so it lands with M2.

`omaboot new` without `--from-omarchy-theme` scaffolds `theme.toml` and
nothing else, and says so. Silently copying some logo into every new theme
would make a choice about how a theme looks on the user's behalf; asking for a
theme to borrow from is the same convenience without the guess.

## Testing

Every test runs against a temporary prefix with fake tool binaries, and the
command runner is a recording double, so nothing in the suite needs root and
nothing in it can reach the real system. Each pipeline step has a test for its
failure path: an invalid theme, missing packaged assets, an unusable stage, a
greeter that fails and a greeter that hangs, a symlinked destination, a
transient initramfs failure that reverts cleanly, a permanent one that cannot,
and a verify mismatch. One test walks every operation in a plan and asserts
that none of them names a directory Omarchy owns.
