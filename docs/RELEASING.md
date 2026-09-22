# Releasing

omaboot can rewrite what a machine boots, so a release is not a tag on a green
test run. The order below is the order the proofs depend on each other in:
nothing that only a booted machine can prove is skipped because CI was green.

`docs/SPEC.md`, "Distribution", says which packages exist and why the AUR is
the channel.

## 1. The bench

All four must pass, from a clean worktree:

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                      # and once more with OMARCHY_PATH set
cargo build --release && scripts/ci-pipeline.sh
shellcheck -S warning scripts/*.sh plugin/harness/*.sh
/usr/lib/qt6/bin/qmllint plugin/*.qml plugin/harness/*.qml
env -u XDG_CONFIG_HOME -u XDG_STATE_HOME OMARCHY_PATH=/usr/share/omarchy \
  PATH=$PWD/.harness/venv/bin:$PATH bash plugin/harness/shots.sh .harness/release
# and the same wrapper for exercise.sh and theme-follow.sh
```

Everything above the harness is what CI runs (`.github/workflows/ci.yml`). The
harness is not: it renders the window with the real shell widgets, which needs
PySide6 (the git-ignored venv at `.harness/venv`) and an Omarchy tree. A
release is the moment to run it and **look at the pictures**
(`docs/UI.md`, "Seeing it without a shell"); a green exit only means nothing
crashed and the edits landed.

## 2. The version, in the three places that carry it

1. `Cargo.toml`, `[workspace.package] version` — the binaries, and the
   `omaboot-git` package's `pkgver()`.
2. `plugin/manifest.json`, `version` — what the shell shows.
   `plugin::tests::the_manifest_carries_the_crate_version_and_the_plugin_id`
   fails if this drifts, so the test suite catches a forgotten bump.
3. `packaging/PKGBUILD-release`, `pkgver`.

Then the prose: the status line in `README.md` and the milestone status in
`docs/SPEC.md` say what is proven, not what is hoped.

## 3. Tag and push

```bash
git commit -am "release: v<version>"
git tag -a v<version> -m "omaboot v<version>"
git push origin master --follow-tags
```

CI runs on the tag as well as the branch, and keeps the built package as an
artefact.

## 4. Prove it on a machine that can be broken

Take the package CI built (or `cd packaging && makepkg -p PKGBUILD-release`
after step 5) and run the round in a disposable guest:

```bash
scripts/vm-round.sh <package.pkg.tar.zst> <theme dir> docs/evidence/release-<version>
```

Apply, reboot, the three screens; reset, reboot, stock; an apply killed during
the initramfs rebuild, then revert. Commit the evidence directory. A change to
the apply pipeline without this round is not released.

## 5. The AUR

The checksum in `packaging/PKGBUILD-release` is `SKIP` in the repository
because the tarball does not exist until the tag is pushed. Fill it from the
real tarball, never publish `SKIP`:

```bash
cd packaging
updpkgsums -p PKGBUILD-release      # pacman-contrib
makepkg -p PKGBUILD-release -f      # builds, and check() runs the suite
namcap PKGBUILD-release ./omaboot-<version>-1-x86_64.pkg.tar.zst
```

Then, in the AUR repository (`ssh://aur@aur.archlinux.org/omaboot.git`, one
repository per package, the file named `PKGBUILD` there):

```bash
cp packaging/PKGBUILD-release <aur-checkout>/PKGBUILD
cd <aur-checkout> && makepkg --printsrcinfo > .SRCINFO
git commit -am "omaboot <version>" && git push
```

`omaboot-git` is its own AUR repository and only needs a push when the
PKGBUILD itself changes; its `pkgver()` follows the branch.

## 6. The release page

GitHub release on the tag: what changed, what is proven and where its evidence
is, and the two install lines.

```bash
omarchy pkg aur add omaboot        # the release
omarchy pkg aur add omaboot-git    # the tip
omaboot plugin install             # links the plugin into the shell
```
