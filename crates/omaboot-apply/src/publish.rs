//! Everything the privileged helper does, in one auditable file.

use std::fs::{self, File};
use std::io::Write as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail, ensure};

/// Directories owned by Omarchy. Repeated here rather than shared, so this
/// binary can be audited without reading another crate.
const OMARCHY_OWNED: [&str; 2] = [
    "/usr/share/plymouth/themes/omarchy",
    "/usr/share/sddm/themes/omarchy",
];

const PLYMOUTH_DIR: &str = "/usr/share/plymouth/themes/omaboot";
const SDDM_DIR: &str = "/usr/share/sddm/themes/omaboot";
/// Where a live preview theme goes. plymouthd looks under `/run/plymouth/themes`
/// before `/usr/share/plymouth/themes`, and `/run` is a tmpfs that a reboot
/// empties, so a preview never leaves anything behind and never touches the
/// installed themes.
const PREVIEW_DIR: &str = "/run/plymouth/themes/omaboot-preview";
const SDDM_DROPIN: &str = "/etc/sddm.conf.d/zz-omaboot.conf";
const DROPIN_CONTENTS: &str = "[Theme]\nCurrent=omaboot\n";
/// The drop-in that points SDDM at Omarchy's own login theme. Written when
/// someone wants Omarchy's login screen back without uninstalling whatever
/// third-party theme's drop-in outranks it; omaboot's file sorts last, so it
/// wins, and removing it gives the other theme its turn again.
const STOCK_DROPIN_CONTENTS: &str = "[Theme]\nCurrent=omarchy\n";

/// The same ceiling `omarchy-plymouth-set` enforces.
const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;

/// The fixed list of names this helper will publish, and whether each one has
/// to be there. A staged directory holding anything else is refused: an
/// unexpected file is either a bug in omaboot or someone else's idea.
const PLYMOUTH_FILES: [(&str, bool); 9] = [
    ("omaboot.plymouth", true),
    ("omaboot.script", true),
    ("logo.png", true),
    ("logo-shutdown.png", false),
    ("bullet.png", true),
    ("entry.png", true),
    ("lock.png", true),
    ("progress_bar.png", true),
    ("progress_box.png", true),
];

/// The preview theme is the Plymouth theme under another name, with its
/// `.plymouth` pointing at the preview directory.
const PREVIEW_FILES: [(&str, bool); 9] = [
    ("omaboot-preview.plymouth", true),
    ("omaboot.script", true),
    ("logo.png", true),
    ("logo-shutdown.png", false),
    ("bullet.png", true),
    ("entry.png", true),
    ("lock.png", true),
    ("progress_bar.png", true),
    ("progress_box.png", true),
];

const SDDM_FILES: [(&str, bool); 10] = [
    ("Main.qml", true),
    ("theme.conf", true),
    ("metadata.desktop", true),
    ("logo.png", true),
    ("background.png", false),
    ("bullet.png", true),
    ("entry.png", true),
    ("lock.png", true),
    ("entry-failed.png", true),
    ("lock-failed.png", true),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    /// A real system: the process must be root and every destination
    /// directory, up to `/`, must be root-owned and not group or world
    /// writable.
    Strict,
    /// A prefixed run: ownership is not required, because nothing outside the
    /// prefix can be reached.
    Prefixed,
}

#[derive(Debug, Clone)]
pub struct Mode {
    pub root: Option<PathBuf>,
    pub dry_run: bool,
    pub trust: Trust,
}

impl Mode {
    fn system(&self, absolute: &str) -> PathBuf {
        match &self.root {
            None => PathBuf::from(absolute),
            Some(root) => root.join(absolute.trim_start_matches('/')),
        }
    }
}

/// What the drop-in should say, or that it should go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchTarget {
    /// `Current=omaboot`
    Omaboot,
    /// `Current=omarchy`: Omarchy's own login theme, on purpose.
    Stock,
    /// No drop-in: whatever else is configured decides.
    Off,
}

#[derive(Debug, Clone)]
pub enum Job {
    Install {
        staged: PathBuf,
    },
    Switch {
        target: SwitchTarget,
    },
    Remove,
    /// Put a staged preview theme under `/run` for `plymouthd` to find.
    PreviewInstall {
        staged: PathBuf,
    },
    PreviewRemove,
}

pub fn run(job: &Job, mode: &Mode) -> Result<Vec<String>> {
    if mode.trust == Trust::Strict && !mode.dry_run {
        let uid = effective_uid().context("reading the effective user id")?;
        ensure!(
            uid == 0,
            "omaboot-apply must run as root. It is invoked by omaboot through sudo; \
             run `omaboot apply <theme>` instead of calling it by hand."
        );
    }

    match job {
        Job::Install { staged } => install(staged, mode),
        Job::Switch { target } => switch(*target, mode),
        Job::Remove => remove(mode),
        Job::PreviewInstall { staged } => preview_install(staged, mode),
        Job::PreviewRemove => preview_remove(mode),
    }
}

fn preview_install(staged: &Path, mode: &Mode) -> Result<Vec<String>> {
    let staged = validated_staging_root(staged)?;
    let destination = mode.system(PREVIEW_DIR);
    let mut done = Vec::new();
    publish_directory(&staged, &destination, &PREVIEW_FILES[..], mode, &mut done)?;
    Ok(done)
}

fn preview_remove(mode: &Mode) -> Result<Vec<String>> {
    let directory = mode.system(PREVIEW_DIR);
    refuse_omarchy(&directory)?;
    if !directory.exists() {
        return Ok(vec![format!("{} was already absent", directory.display())]);
    }
    ensure!(
        !fs::symlink_metadata(&directory)?.file_type().is_symlink(),
        "{} is a symlink; refusing to remove through it",
        directory.display()
    );
    if mode.dry_run {
        return Ok(vec![format!("would remove {}", directory.display())]);
    }
    fs::remove_dir_all(&directory).with_context(|| format!("removing {}", directory.display()))?;
    Ok(vec![format!("removed {}", directory.display())])
}

fn install(staged: &Path, mode: &Mode) -> Result<Vec<String>> {
    let staged = validated_staging_root(staged)?;
    let mut done = Vec::new();

    for (sub, destination, manifest) in [
        ("plymouth", mode.system(PLYMOUTH_DIR), &PLYMOUTH_FILES[..]),
        ("sddm", mode.system(SDDM_DIR), &SDDM_FILES[..]),
    ] {
        let source_dir = staged.join(sub);
        ensure!(
            source_dir.is_dir(),
            "the staged directory has no {sub}/ subdirectory: {}",
            source_dir.display()
        );
        publish_directory(&source_dir, &destination, manifest, mode, &mut done)?;
    }

    Ok(done)
}

/// Publish one staged directory into one fixed destination: refuse anything
/// not in the manifest, require everything the manifest requires, publish
/// each file atomically, then prune what a previous publish left behind.
fn publish_directory(
    source_dir: &Path,
    destination: &Path,
    manifest: &[(&str, bool)],
    mode: &Mode,
    done: &mut Vec<String>,
) -> Result<()> {
    let present = read_source_names(source_dir)?;
    for name in &present {
        ensure!(
            manifest.iter().any(|(known, _)| known == name),
            "{} is not a file omaboot-apply publishes; refusing the whole transaction",
            source_dir.join(name).display()
        );
    }
    for (name, required) in manifest {
        ensure!(
            !required || present.iter().any(|p| p == name),
            "the staged theme is missing {}",
            source_dir.join(name).display()
        );
    }

    refuse_omarchy(destination)?;
    if mode.dry_run {
        if destination.exists() {
            validate_destination_directory(destination, mode)?;
        } else {
            done.push(format!("would create {}", destination.display()));
        }
    } else {
        fs::create_dir_all(destination)
            .with_context(|| format!("creating {}", destination.display()))?;
        set_mode(destination, 0o755)?;
        validate_destination_directory(destination, mode)?;
    }

    let mut published = Vec::new();
    for (name, _) in manifest {
        if !present.iter().any(|p| p == name) {
            continue;
        }
        let source = source_dir.join(name);
        let target = destination.join(name);
        refuse_omarchy(&target)?;
        if mode.dry_run {
            done.push(format!("would publish {}", target.display()));
        } else {
            publish_file(&source, &target)?;
            done.push(format!("published {}", target.display()));
        }
        published.push((*name).to_string());
    }

    // A file left over from a previous apply, such as a shutdown logo the
    // current theme no longer has, would otherwise stay behind and be read
    // by nothing. The directory is omaboot's own, so pruning it is safe.
    for stale in read_source_names(destination)?
        .into_iter()
        .filter(|name| !published.contains(name))
    {
        let path = destination.join(&stale);
        refuse_omarchy(&path)?;
        if mode.dry_run {
            done.push(format!("would remove stale {}", path.display()));
        } else {
            fs::remove_file(&path).with_context(|| format!("removing stale {}", path.display()))?;
            done.push(format!("removed stale {}", path.display()));
        }
    }

    Ok(())
}

fn switch(target: SwitchTarget, mode: &Mode) -> Result<Vec<String>> {
    let dropin = mode.system(SDDM_DROPIN);
    refuse_omarchy(&dropin)?;
    let parent = dropin.parent().unwrap_or(Path::new("/"));

    let contents = match target {
        SwitchTarget::Omaboot => Some(DROPIN_CONTENTS),
        SwitchTarget::Stock => Some(STOCK_DROPIN_CONTENTS),
        SwitchTarget::Off => None,
    };
    if let Some(contents) = contents {
        if mode.dry_run {
            return Ok(vec![format!(
                "would write {} ({})",
                dropin.display(),
                contents.trim().replace('\n', " ")
            )]);
        }
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        validate_destination_directory(parent, mode)?;
        write_atomic(&dropin, contents.as_bytes(), 0o644)?;
        Ok(vec![format!(
            "wrote {} ({})",
            dropin.display(),
            contents.trim().replace('\n', " ")
        )])
    } else {
        if mode.dry_run {
            return Ok(vec![format!("would remove {}", dropin.display())]);
        }
        match fs::remove_file(&dropin) {
            Ok(()) => Ok(vec![format!("removed {}", dropin.display())]),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(vec![format!("{} was already absent", dropin.display())])
            }
            Err(error) => Err(error).with_context(|| format!("removing {}", dropin.display())),
        }
    }
}

fn remove(mode: &Mode) -> Result<Vec<String>> {
    let mut done = switch(SwitchTarget::Off, mode)?;
    for directory in [mode.system(PLYMOUTH_DIR), mode.system(SDDM_DIR)] {
        refuse_omarchy(&directory)?;
        if !directory.exists() {
            continue;
        }
        ensure!(
            !fs::symlink_metadata(&directory)?.file_type().is_symlink(),
            "{} is a symlink; refusing to remove through it",
            directory.display()
        );
        if mode.dry_run {
            done.push(format!("would remove {}", directory.display()));
        } else {
            fs::remove_dir_all(&directory)
                .with_context(|| format!("removing {}", directory.display()))?;
            done.push(format!("removed {}", directory.display()));
        }
    }
    Ok(done)
}

// ------------------------------------------------------------------ checks

/// Refuse anything inside a directory Omarchy owns, before touching it.
fn refuse_omarchy(path: &Path) -> Result<()> {
    let text = path.to_string_lossy();
    for owned in OMARCHY_OWNED {
        if text == owned || text.contains(&format!("{owned}/")) || text.ends_with(owned) {
            bail!(
                "refusing to touch {}: {owned} belongs to Omarchy and omaboot never writes inside it",
                path.display()
            );
        }
    }
    Ok(())
}

fn validated_staging_root(staged: &Path) -> Result<PathBuf> {
    ensure!(
        staged.is_absolute(),
        "the staged directory must be an absolute path, got {}",
        staged.display()
    );
    let metadata =
        fs::symlink_metadata(staged).with_context(|| format!("reading {}", staged.display()))?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "{} is a symlink; refusing to publish through it",
        staged.display()
    );
    ensure!(metadata.is_dir(), "{} is not a directory", staged.display());
    let canonical =
        fs::canonicalize(staged).with_context(|| format!("resolving {}", staged.display()))?;
    ensure!(
        canonical == staged,
        "{} resolves to {}; pass the resolved path",
        staged.display(),
        canonical.display()
    );
    Ok(canonical)
}

/// The names of the regular files directly inside a directory, sorted.
/// A subdirectory, a symlink or a device file is refused rather than skipped.
fn read_source_names(dir: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    if !dir.exists() {
        return Ok(names);
    }
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        ensure!(
            !file_type.is_symlink(),
            "{} is a symlink; refusing the whole transaction",
            entry.path().display()
        );
        ensure!(
            file_type.is_file(),
            "{} is not a regular file; refusing the whole transaction",
            entry.path().display()
        );
        names.push(name);
    }
    names.sort();
    Ok(names)
}

/// Every directory from here to `/` must be root-owned and not group or world
/// writable, exactly as the Omarchy publisher requires.
fn validate_destination_directory(directory: &Path, mode: &Mode) -> Result<()> {
    let metadata = fs::symlink_metadata(directory)
        .with_context(|| format!("reading {}", directory.display()))?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "{} is a symlink; refusing to publish into it",
        directory.display()
    );
    ensure!(
        metadata.is_dir(),
        "{} is not a directory",
        directory.display()
    );

    if mode.trust == Trust::Prefixed {
        return Ok(());
    }

    let mut current = Some(directory);
    while let Some(path) = current {
        let metadata =
            fs::symlink_metadata(path).with_context(|| format!("reading {}", path.display()))?;
        ensure!(
            metadata.uid() == 0,
            "{} is not root-owned; refusing to publish into it",
            path.display()
        );
        ensure!(
            metadata.mode() & 0o022 == 0,
            "{} is group or world writable; refusing to publish into it",
            path.display()
        );
        current = path.parent();
        if path == Path::new("/") {
            break;
        }
    }
    Ok(())
}

// --------------------------------------------------------------- publishing

/// Copy one file into place: validate the source, write a sibling temporary,
/// compare it with the source, flush it, then rename. A reader of the
/// destination sees either the old file or the new one, never a partial write.
fn publish_file(source: &Path, target: &Path) -> Result<()> {
    let metadata =
        fs::symlink_metadata(source).with_context(|| format!("reading {}", source.display()))?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "{} is a symlink; refusing to publish it",
        source.display()
    );
    ensure!(
        metadata.is_file(),
        "{} is not a regular file",
        source.display()
    );
    ensure!(
        metadata.len() > 0 && metadata.len() <= MAX_ASSET_BYTES,
        "{} is {} bytes; the accepted range is 1 to {MAX_ASSET_BYTES}",
        source.display(),
        metadata.len()
    );

    if let Ok(existing) = fs::symlink_metadata(target) {
        ensure!(
            !existing.file_type().is_symlink(),
            "{} is a symlink; refusing to publish through it",
            target.display()
        );
    }

    let bytes = fs::read(source).with_context(|| format!("reading {}", source.display()))?;
    ensure!(
        bytes.len() as u64 == metadata.len(),
        "{} changed size while it was being read",
        source.display()
    );
    write_atomic(target, &bytes, 0o644)?;

    let written = fs::read(target).with_context(|| format!("re-reading {}", target.display()))?;
    ensure!(
        written == bytes,
        "{} does not match its source after the copy",
        target.display()
    );
    Ok(())
}

fn write_atomic(target: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let parent = target.parent().unwrap_or(Path::new("/"));
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "omaboot".to_string());
    let temporary = parent.join(format!(".{name}.omaboot-new"));

    {
        let mut file = File::create(&temporary)
            .with_context(|| format!("creating {}", temporary.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("writing {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("flushing {}", temporary.display()))?;
    }
    set_mode(&temporary, mode)?;
    fs::rename(&temporary, target)
        .with_context(|| format!("renaming {} onto {}", temporary.display(), target.display()))?;
    Ok(())
}

fn set_mode(path: &Path, mode: u32) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("setting the mode of {}", path.display()))
}

/// The effective user id, read from `/proc/self/status` so this binary needs
/// no `unsafe` and no libc.
fn effective_uid() -> Result<u32> {
    let status = fs::read_to_string("/proc/self/status").context("reading /proc/self/status")?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            let mut fields = rest.split_whitespace();
            let _real = fields.next();
            let effective = fields
                .next()
                .context("/proc/self/status has no effective uid")?;
            return effective
                .parse()
                .context("the effective uid is not a number");
        }
    }
    bail!("/proc/self/status has no Uid line")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct World {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        staged: PathBuf,
    }

    impl World {
        fn new() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let root = tmp.path().join("root");
            let staged = tmp.path().join("stage");
            fs::create_dir_all(staged.join("plymouth")).unwrap();
            fs::create_dir_all(staged.join("sddm")).unwrap();
            for (name, required) in PLYMOUTH_FILES {
                if required {
                    fs::write(staged.join("plymouth").join(name), format!("p {name}")).unwrap();
                }
            }
            for (name, required) in SDDM_FILES {
                if required {
                    fs::write(staged.join("sddm").join(name), format!("s {name}")).unwrap();
                }
            }
            fs::create_dir_all(&root).unwrap();
            Self {
                _tmp: tmp,
                root,
                staged,
            }
        }

        fn mode(&self, dry_run: bool) -> Mode {
            Mode {
                root: Some(self.root.clone()),
                dry_run,
                trust: Trust::Prefixed,
            }
        }

        fn install(&self) -> Result<Vec<String>> {
            run(
                &Job::Install {
                    staged: self.staged.clone(),
                },
                &self.mode(false),
            )
        }

        fn installed(&self, relative: &str) -> PathBuf {
            self.root.join("usr/share").join(relative)
        }
    }

    #[test]
    fn a_complete_stage_is_published() {
        let world = World::new();
        world.install().unwrap();
        assert_eq!(
            fs::read_to_string(world.installed("plymouth/themes/omaboot/omaboot.script")).unwrap(),
            "p omaboot.script"
        );
        assert_eq!(
            fs::read_to_string(world.installed("sddm/themes/omaboot/Main.qml")).unwrap(),
            "s Main.qml"
        );
        let mode = fs::metadata(world.installed("sddm/themes/omaboot/Main.qml"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o644);
    }

    #[test]
    fn publishing_leaves_no_temporary_file_behind() {
        let world = World::new();
        world.install().unwrap();
        let leftovers: Vec<_> = fs::read_dir(world.installed("plymouth/themes/omaboot"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with('.'))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn a_dry_run_publishes_nothing() {
        let world = World::new();
        let lines = run(
            &Job::Install {
                staged: world.staged.clone(),
            },
            &world.mode(true),
        )
        .unwrap();
        assert!(lines.iter().all(|line| line.starts_with("would")));
        assert!(!world.installed("plymouth/themes/omaboot/logo.png").exists());
    }

    #[test]
    fn an_unexpected_file_in_the_stage_refuses_the_whole_transaction() {
        let world = World::new();
        fs::write(world.staged.join("plymouth/evil.sh"), b"rm -rf /").unwrap();
        let error = world.install().unwrap_err().to_string();
        assert!(error.contains("evil.sh"), "{error}");
        assert!(!world.installed("plymouth/themes/omaboot/logo.png").exists());
    }

    #[test]
    fn a_missing_required_file_is_refused() {
        let world = World::new();
        fs::remove_file(world.staged.join("sddm/Main.qml")).unwrap();
        let error = world.install().unwrap_err().to_string();
        assert!(error.contains("Main.qml"), "{error}");
    }

    #[test]
    fn an_optional_file_may_be_absent_and_is_published_when_present() {
        let world = World::new();
        world.install().unwrap();
        assert!(
            !world
                .installed("plymouth/themes/omaboot/logo-shutdown.png")
                .exists()
        );

        fs::write(world.staged.join("plymouth/logo-shutdown.png"), b"bye").unwrap();
        world.install().unwrap();
        assert!(
            world
                .installed("plymouth/themes/omaboot/logo-shutdown.png")
                .is_file()
        );
    }

    #[test]
    fn a_stale_file_from_a_previous_apply_is_pruned() {
        let world = World::new();
        fs::write(world.staged.join("plymouth/logo-shutdown.png"), b"bye").unwrap();
        world.install().unwrap();
        fs::remove_file(world.staged.join("plymouth/logo-shutdown.png")).unwrap();
        world.install().unwrap();
        assert!(
            !world
                .installed("plymouth/themes/omaboot/logo-shutdown.png")
                .exists()
        );
        assert!(
            world
                .installed("plymouth/themes/omaboot/logo.png")
                .is_file()
        );
    }

    #[test]
    fn a_symlinked_source_is_refused() {
        let world = World::new();
        let victim = world.root.join("secret");
        fs::write(&victim, b"root only").unwrap();
        fs::remove_file(world.staged.join("plymouth/logo.png")).unwrap();
        std::os::unix::fs::symlink(&victim, world.staged.join("plymouth/logo.png")).unwrap();

        let error = world.install().unwrap_err().to_string();
        assert!(error.contains("symlink"), "{error}");
    }

    #[test]
    fn a_symlinked_destination_file_is_refused() {
        let world = World::new();
        world.install().unwrap();
        let victim = world.root.join("victim");
        fs::write(&victim, b"precious").unwrap();
        let target = world.installed("sddm/themes/omaboot/Main.qml");
        fs::remove_file(&target).unwrap();
        std::os::unix::fs::symlink(&victim, &target).unwrap();

        let error = world.install().unwrap_err().to_string();
        assert!(error.contains("symlink"), "{error}");
        assert_eq!(fs::read(&victim).unwrap(), b"precious");
    }

    #[test]
    fn an_empty_file_is_refused() {
        let world = World::new();
        fs::write(world.staged.join("plymouth/logo.png"), b"").unwrap();
        let error = world.install().unwrap_err().to_string();
        assert!(error.contains("0 bytes"), "{error}");
    }

    #[test]
    fn an_oversized_file_is_refused() {
        let world = World::new();
        let file = File::create(world.staged.join("plymouth/logo.png")).unwrap();
        file.set_len(MAX_ASSET_BYTES + 1).unwrap();
        drop(file);
        let error = world.install().unwrap_err().to_string();
        assert!(error.contains("accepted range"), "{error}");
    }

    #[test]
    fn a_relative_staging_path_is_refused() {
        let world = World::new();
        let error = run(
            &Job::Install {
                staged: PathBuf::from("stage"),
            },
            &world.mode(false),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("absolute"), "{error}");
    }

    #[test]
    fn a_symlinked_staging_directory_is_refused() {
        let world = World::new();
        let link = world.root.join("stage-link");
        std::os::unix::fs::symlink(&world.staged, &link).unwrap();
        let error = run(&Job::Install { staged: link }, &world.mode(false))
            .unwrap_err()
            .to_string();
        assert!(error.contains("symlink"), "{error}");
    }

    #[test]
    fn a_subdirectory_in_the_stage_is_refused() {
        let world = World::new();
        fs::create_dir(world.staged.join("plymouth/nested")).unwrap();
        let error = world.install().unwrap_err().to_string();
        assert!(error.contains("not a regular file"), "{error}");
    }

    #[test]
    fn the_switch_writes_and_removes_one_line() {
        let world = World::new();
        fs::create_dir_all(world.root.join("etc/sddm.conf.d")).unwrap();
        run(
            &Job::Switch {
                target: SwitchTarget::Omaboot,
            },
            &world.mode(false),
        )
        .unwrap();
        let dropin = world.root.join("etc/sddm.conf.d/zz-omaboot.conf");
        assert_eq!(fs::read_to_string(&dropin).unwrap(), DROPIN_CONTENTS);

        run(
            &Job::Switch {
                target: SwitchTarget::Stock,
            },
            &world.mode(false),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&dropin).unwrap(), STOCK_DROPIN_CONTENTS);

        run(
            &Job::Switch {
                target: SwitchTarget::Off,
            },
            &world.mode(false),
        )
        .unwrap();
        assert!(!dropin.exists());
        // Removing it twice is not an error: every step is re-runnable.
        run(
            &Job::Switch {
                target: SwitchTarget::Off,
            },
            &world.mode(false),
        )
        .unwrap();
    }

    #[test]
    fn remove_takes_away_the_directories_and_the_drop_in() {
        let world = World::new();
        world.install().unwrap();
        run(
            &Job::Switch {
                target: SwitchTarget::Omaboot,
            },
            &world.mode(false),
        )
        .unwrap();

        run(&Job::Remove, &world.mode(false)).unwrap();
        assert!(!world.installed("plymouth/themes/omaboot").exists());
        assert!(!world.installed("sddm/themes/omaboot").exists());
        assert!(!world.root.join("etc/sddm.conf.d/zz-omaboot.conf").exists());
    }

    #[test]
    fn a_preview_theme_lands_under_run_and_leaves_the_installed_theme_alone() {
        let world = World::new();
        world.install().unwrap();
        let before = fs::read(world.installed("plymouth/themes/omaboot/omaboot.script")).unwrap();

        let staged = world.root.join("preview-stage");
        fs::create_dir_all(&staged).unwrap();
        for (name, required) in PREVIEW_FILES {
            if required {
                fs::write(staged.join(name), format!("preview {name}")).unwrap();
            }
        }
        run(&Job::PreviewInstall { staged }, &world.mode(false)).unwrap();

        let preview = world.root.join("run/plymouth/themes/omaboot-preview");
        assert_eq!(
            fs::read_to_string(preview.join("omaboot-preview.plymouth")).unwrap(),
            "preview omaboot-preview.plymouth"
        );
        assert_eq!(
            fs::read(world.installed("plymouth/themes/omaboot/omaboot.script")).unwrap(),
            before,
            "the installed theme is untouched"
        );

        run(&Job::PreviewRemove, &world.mode(false)).unwrap();
        assert!(!preview.exists());
        // Twice is fine: a preview that was already cleaned up is not an error.
        run(&Job::PreviewRemove, &world.mode(false)).unwrap();
    }

    #[test]
    fn a_preview_stage_with_the_installed_theme_name_is_refused() {
        // The preview manifest wants omaboot-preview.plymouth, so a stage
        // holding the installed theme's omaboot.plymouth is the wrong one.
        let world = World::new();
        let staged = world.root.join("preview-stage");
        fs::create_dir_all(&staged).unwrap();
        for (name, required) in PLYMOUTH_FILES {
            if required {
                fs::write(staged.join(name), b"x").unwrap();
            }
        }
        let error = run(&Job::PreviewInstall { staged }, &world.mode(false))
            .unwrap_err()
            .to_string();
        assert!(error.contains("omaboot.plymouth"), "{error}");
    }

    #[test]
    fn nothing_this_binary_does_can_name_an_omarchy_directory() {
        for path in [
            "/usr/share/plymouth/themes/omarchy",
            "/usr/share/sddm/themes/omarchy/Main.qml",
            "/tmp/prefix/usr/share/plymouth/themes/omarchy/logo.png",
        ] {
            assert!(refuse_omarchy(Path::new(path)).is_err(), "{path}");
        }
        refuse_omarchy(Path::new(PLYMOUTH_DIR)).unwrap();
        refuse_omarchy(Path::new(SDDM_DIR)).unwrap();
    }

    #[test]
    fn a_world_writable_destination_is_refused_under_the_strict_policy() {
        let world = World::new();
        let destination = world.installed("plymouth/themes/omaboot");
        fs::create_dir_all(&destination).unwrap();
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o777)).unwrap();

        let strict = Mode {
            root: Some(world.root.clone()),
            dry_run: false,
            trust: Trust::Strict,
        };
        let error = validate_destination_directory(&destination, &strict)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("world writable") || error.contains("not root-owned"),
            "{error}"
        );
    }

    #[test]
    fn the_effective_uid_can_be_read_without_unsafe() {
        let uid = effective_uid().unwrap();
        assert_eq!(uid, std::fs::metadata("/proc/self").unwrap().uid());
    }
}
