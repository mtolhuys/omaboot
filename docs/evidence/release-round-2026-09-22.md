# Release round, 22 September 2026

The round `docs/RELEASING.md` puts between a tag and the AUR, run for the
first time: the **v0.1.0 package**, `omaboot 0.1.0-1`, built by
`makepkg -p packaging/PKGBUILD-release` from the GitHub source tarball of the
tag, installed with `pacman -U` in a disposable Omarchy 4.0.3 guest and driven
end to end. Theme `matte` (a copy of the reference machine's, never the
original). Pictures, logs and hashes in
`docs/evidence/release-round-2026-09-22/`; `commands.log` has every command
with its output.

Two defects came out of the evening, one in the round's own script and one in
the engine. Neither is in the apply pipeline, and nothing the round proved had
to be redone because of them. The round was then run a second time against the
`omaboot 0.1.1-1` package, which carries the engine fix; that run is at the
foot of this file.

## What passed

| # | What | Result |
|---|------|--------|
| 1 | `pacman -U` the release package; `omaboot --version`, `omaboot-apply protocol` | pass — `omaboot 0.1.0-1`, version `0.1.0`, protocol `2`, plugin at `/usr/share/omaboot/plugin`, no harness |
| 2 | `omaboot plugin install` into the running shell, window summoned | pass |
| 3 | `omaboot --password-stdin apply matte` | pass — every step, `apply-first.txt` |
| 4 | Reboot: shutdown splash, boot splash, greeter | pass — `03-applied-reboot-08.png`, `-88.png`, `03-applied-after-greeter-typed.png` |
| 5 | The unlock prompt against a real LUKS volume | pass — `04-unlock-applied-prompt.png`, `-typed`, `-unlocked` |
| 6 | Omarchy's own theme directories, with omaboot applied | pass — unchanged, `fingerprint-applied.txt` |
| 7 | `omaboot reset`, reboot | pass — stock again, `fingerprint-after-reset.txt`, `05-reset-reboot-01.png`, `-13`, `-16` |
| 8 | The UKI after reset against the stock one | pass — byte for byte, `39cb4675…` both |
| 9 | Apply again, `kill -9` during the initramfs step, reboot | pass — the guest boots, `07-killed-reboot-86.png` |
| 10 | `omaboot revert` after the interrupted apply, reboot | pass — stock again, UKI `39cb4675…`, `08-reverted-reboot-13.png` |

The stock UKI hash, `39cb4675bf292bbb…`, is the same one the closing round
recorded on 20 September, and reset and revert both land back on it exactly.
The two Omarchy theme directories hash the same with omaboot applied, after
reset and after revert (`ea824684…` Plymouth, `30459331…` SDDM): the one rule,
across a whole round, on a real machine image.

## Findings

### R1. `sample()` in the round's own script dies on bash 5.3

- What: the first attempt at this round, `scripts/vm-round.sh`, immediately
  after the first apply.
- Saw: `scripts/vm-round.sh: line 205: name: unbound variable`, thirty minutes
  in, with the guest already applied and rebooting. Nothing after the apply was
  sampled.
- Cause: `local name="$1" seconds="$2" dir="$WORK/sample-$name" …`. Bash 5.3
  declares every name in one `local` before it assigns any of them, so `$name`
  in the same statement is unset, and `set -u` ends the script. The round of 20
  September ran under bash 5.2, where the same line works. shellcheck has had a
  check for exactly this since 0.8: SC2318.
- Fix: two `local`s, and `shellcheck -S warning` over `scripts/*.sh` and
  `plugin/harness/*.sh` as a CI step, which is the test for the class rather
  than for the one line. Verified by running `sample` against a stubbed
  `screendump` and `magick`, and by the round below, which sampled four
  reboots.

### R2. `status` after an interrupted apply says the system is on its own themes

- What: phase 9, `omaboot status` in the guest right after `kill -9` during the
  initramfs step (`after-kill.txt`).
- Saw: the facts said `theme omaboot`, `owner installed by omaboot`,
  `decided by /etc/sddm.conf.d/zz-omaboot.conf` — and under them,
  `nothing is applied by omaboot; the system is on its own themes`, followed by
  the interrupted-apply warning. The middle sentence is the only untrue thing
  on the screen: the switch had happened, and what boots was omaboot's.
- Cause: `cli::status` prints that sentence whenever there is no applied
  record. The record is written last, so an apply killed after the switch has
  none, while the switch it already made stands.
- Fix: when there is no record but a rollback point, `status` says
  `no apply finished: the last one was interrupted, and what boots now is what
  it left`, and the warning below still names the theme and the way out. Test:
  `cli::tests::status_after_an_interrupted_apply_does_not_claim_the_system_is_on_its_own_themes`,
  which fails on the old text and also checks that a system with no rollback
  point still gets the plain sentence. This is what v0.1.1 carries.
- Not a pipeline change: the apply, revert and reset paths are untouched.

## What a kill during the initramfs step actually leaves

Worth writing down, because it is what a person would see and it is not a
defect. The switch happens before the initramfs rebuild, so after the kill:

- the **boot** screen is still Omarchy's own (`07-killed-reboot-86.png`): the
  initramfs was never rebuilt, so it carries the stock theme and
  `Theme=omarchy`;
- the **shutdown** screen is omaboot's (`07-killed-reboot-08.png`): that one is
  drawn from `/usr/share/plymouth/themes` on the running system, which the
  install step had already written;
- the **login** screen is omaboot's (`07-killed-reboot-89.png`): the drop-in
  was written by the switch.

So the machine boots, nothing is broken, and the three screens disagree with
each other until `omaboot revert` or a second `apply`. `status` says so — and
after R2 it says so without contradicting itself. The JSON step stream stops at
`"step":"initramfs","state":"started"` (`apply-killed-steps.txt`), which is
where the kill landed.

## Beside the round

- **The harness**, all three scripts, against the same tree (`.harness/venv` on
  `PATH`, XDG unset, `OMARCHY_PATH=/usr/share/omarchy`): `shots.sh` wrote its
  twelve pictures, `exercise.sh` passed every flow and reported the real
  `~/.config/omaboot` untouched, `theme-follow.sh` saw the app follow a theme
  switch (`#121212 -> #eff1f5`). The mark region of `stacked-system.png` is
  byte for byte the one in `test-round-2026-09-19/harness-shots/`, so
  `pragma ComponentBehavior: Bound` in `Mark.qml` changed nothing that is
  drawn. A targeted render after a wallpaper drop shows the colour picker's
  "From the wallpaper you dropped" row filled, which is `Engine.imagePalette`
  answering under its new name.
- **CI** is green on the tag and on every commit of the evening, including the
  two steps added after it: `shellcheck`, and a qmllint that now fails on the
  categories the shell's absent modules cannot explain (`.qmllint.ini`).

## The same round again, on v0.1.1

The whole round was then run a second time, in a fresh guest, against the
`omaboot 0.1.1-1` package built from the v0.1.1 tarball — the one an AUR
upload would carry. Evidence in
`docs/evidence/release-round-0.1.1-2026-09-22/`.

Every row of the table above passed again: the same stock UKI hash
(`39cb4675…`) back after both reset and revert, the same two Omarchy theme
directory hashes with omaboot applied, after reset and after revert, the three
screens after the reboot (`03-applied-reboot-08.png` shutdown,
`-87.png` boot, `-90.png` login), and the unlock prompt against the LUKS
volume.

And R2, in the state it was found in. `after-kill.txt` from this round, under
facts that say `theme omaboot`, `owner installed by omaboot`,
`decided by /etc/sddm.conf.d/zz-omaboot.conf`:

```
no apply finished: the last one was interrupted, and what boots now is what it left
warning: an apply of matte was interrupted after the switch (just now): the rollback point
is recorded but nothing was verified; omaboot revert puts plymouth omarchy back, or apply
again to finish
```

The sentence the first round caught is gone, and what is left agrees with the
facts above it and with what the machine then did: booted Omarchy's own
splash, because the initramfs was never rebuilt (`07-killed-reboot-87.png`),
into omaboot's login screen (`-90.png`).

## Not proven here

- The stranger test M3 asks for: someone who has not read the documentation
  rethemeing their boot screen. Still owed.
- The F6 ten-second path, for the same reason as on 20 September: the guest's
  shell answers too quickly to provoke it.
