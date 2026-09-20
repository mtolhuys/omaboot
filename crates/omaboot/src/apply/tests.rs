//! Pipeline tests.
//!
//! Every test runs against a temporary prefix with fake tools, so nothing here
//! needs root and nothing here can reach the real system.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::exec::CommandOutput;
use crate::exec::testing::RecordingRunner;
use crate::generate::fixture;

const THEME: &str = r##"
[meta]
name = "Test Theme"
author = "test"
version = "1.0.0"

[colors]
background = "#1a1b26"
foreground = "#c0caf5"

[unlock]
prompt = "bullets"
progress = "bar"

[shutdown]
message = "See you"
progress = "spinner"

[login]
clock = true
"##;

/// A prefix with a packaged Omarchy tree, fake tools, and one theme.
struct World {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    layout: Layout,
    assets: AssetSource,
}

impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let config = tmp.path().join("config");
        let state = tmp.path().join("state");

        for tool in [
            "sddm-greeter-qt6",
            "limine-mkinitcpio",
            "plymouth-set-default-theme",
        ] {
            let path = root.join("usr/bin").join(tool);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"#!/bin/sh\nexit 0\n").unwrap();
        }
        let conf = root.join("etc/plymouth/plymouthd.conf");
        fs::create_dir_all(conf.parent().unwrap()).unwrap();
        fs::write(&conf, "[Daemon]\nTheme=omarchy\nShowDelay=0\n").unwrap();
        fs::create_dir_all(root.join("etc/sddm.conf.d")).unwrap();

        let omarchy = fixture::omarchy_tree(&tmp.path().join("omarchy"));
        let layout = Layout::with_dirs(Some(root.clone()), config.clone(), state);
        fixture::theme(&config.join("themes/test"), THEME);

        Self {
            _tmp: tmp,
            root,
            layout,
            assets: AssetSource::at(&omarchy),
        }
    }

    fn pipeline<'a>(&'a self, runner: &'a dyn Runner) -> Pipeline<'a> {
        Pipeline::new(&self.layout, runner).with_assets(self.assets.clone())
    }

    fn installed(&self, relative: &str) -> PathBuf {
        self.root.join("usr/share").join(relative)
    }

    fn plymouth_theme(&self) -> Option<String> {
        crate::state::current_plymouth_theme(&self.layout)
    }

    fn dropin(&self) -> Option<String> {
        crate::state::read_sddm_dropin(&self.layout)
    }

    fn apply(&self, runner: &dyn Runner) -> Result<ApplyReport> {
        self.pipeline(runner).apply(&ApplyRequest::new("test"))
    }
}

fn dry(theme: &str, world: &World, runner: &dyn Runner) -> Result<ApplyReport> {
    let mut request = ApplyRequest::new(theme);
    request.dry_run = true;
    world.pipeline(runner).apply(&request)
}

// ------------------------------------------------------------- happy path

#[test]
fn a_dry_run_lists_every_step_and_changes_nothing() {
    let world = World::new();
    let runner = RecordingRunner::new();
    let report = dry("test", &world, &runner).unwrap();

    let rendered = report.render();
    for step in StepId::ALL {
        assert!(
            rendered.contains(step.title()),
            "{step:?} missing:\n{rendered}"
        );
    }
    assert!(rendered.contains("omaboot.script"), "{rendered}");
    assert!(rendered.contains("Main.qml"), "{rendered}");

    assert!(runner.calls().is_empty(), "a dry run may not run a command");
    assert!(!world.layout.stage_dir().exists());
    assert!(!world.installed("plymouth/themes/omaboot").exists());
    assert!(world.dropin().is_none());
    assert_eq!(world.plymouth_theme().as_deref(), Some("omarchy"));
    assert!(report.problems().is_empty(), "{:?}", report.problems());
}

#[test]
fn an_apply_installs_switches_and_records() {
    let world = World::new();
    let runner = RecordingRunner::new();
    let report = world.apply(&runner).unwrap();

    assert!(
        world
            .installed("plymouth/themes/omaboot/omaboot.script")
            .is_file()
    );
    assert!(
        world
            .installed("plymouth/themes/omaboot/omaboot.plymouth")
            .is_file()
    );
    assert!(world.installed("sddm/themes/omaboot/Main.qml").is_file());
    assert_eq!(world.plymouth_theme().as_deref(), Some("omaboot"));
    assert_eq!(
        world.dropin().as_deref(),
        Some("[Theme]\nCurrent=omaboot\n")
    );

    let applied = crate::state::read_applied(&world.layout).unwrap().unwrap();
    assert_eq!(applied.theme, "test");
    assert_eq!(applied.theme_hash, report.theme_hash);
    assert!(!applied.files.is_empty());

    let rollback = crate::state::read_rollback(&world.layout).unwrap().unwrap();
    assert_eq!(rollback.previous_plymouth_theme.as_deref(), Some("omarchy"));
    assert!(!rollback.sddm_dropin_existed);

    // The greeter really was smoke tested, and the initramfs rebuilt.
    assert!(runner.ran_containing("sddm-greeter-qt6 --test-mode"));
    assert!(runner.ran_containing("limine-mkinitcpio"));
}

#[test]
fn applying_twice_is_idempotent() {
    let world = World::new();
    let runner = RecordingRunner::new();
    let first = world.apply(&runner).unwrap();
    let before = fs::read(world.installed("plymouth/themes/omaboot/omaboot.script")).unwrap();
    let second = world.apply(&runner).unwrap();
    let after = fs::read(world.installed("plymouth/themes/omaboot/omaboot.script")).unwrap();

    assert_eq!(first.theme_hash, second.theme_hash);
    assert_eq!(before, after);
    // The second apply records the first apply's state as what to go back to.
    let rollback = crate::state::read_rollback(&world.layout).unwrap().unwrap();
    assert_eq!(rollback.previous_plymouth_theme.as_deref(), Some("omaboot"));
    assert!(rollback.sddm_dropin_existed);
}

#[test]
fn no_operation_anywhere_targets_a_directory_omarchy_owns() {
    let world = World::new();
    let runner = RecordingRunner::new();
    let report = dry("test", &world, &runner).unwrap();
    for operation in report.operations() {
        let printed = operation.to_string();
        assert!(
            !printed.contains("themes/omarchy"),
            "operation touches an Omarchy path: {printed}"
        );
    }
}

#[test]
fn only_the_selected_steps_run() {
    let world = World::new();
    let runner = RecordingRunner::new();
    let mut request = ApplyRequest::new("test");
    request.only = Some(vec![StepId::Stage]);
    let report = world.pipeline(&runner).apply(&request).unwrap();

    assert!(
        world
            .layout
            .stage_dir()
            .join("plymouth/omaboot.script")
            .is_file()
    );
    assert!(!world.installed("plymouth/themes/omaboot").exists());
    assert_eq!(world.plymouth_theme().as_deref(), Some("omarchy"));
    assert!(
        report
            .steps
            .iter()
            .any(|step| step.id == Some(StepId::Verify) && step.skipped)
    );
}

#[test]
fn a_step_can_be_re_run_on_its_own_after_a_full_apply() {
    let world = World::new();
    let runner = RecordingRunner::new();
    world.apply(&runner).unwrap();

    let mut request = ApplyRequest::new("test");
    request.only = Some(vec![StepId::Verify]);
    world
        .pipeline(&runner)
        .apply(&request)
        .expect("verify alone must pass right after an apply");
}

// --------------------------------------------------------- failure paths

#[test]
fn step_1_a_theme_that_does_not_validate_installs_nothing() {
    let world = World::new();
    let broken = world.layout.theme_dir("broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(
        broken.join("theme.toml"),
        "[meta]\nname = \"b\"\nsurprise = 1\n",
    )
    .unwrap();
    fs::write(broken.join("logo.png"), b"bytes").unwrap();

    let runner = RecordingRunner::new();
    let error = world
        .pipeline(&runner)
        .apply(&ApplyRequest::new("broken"))
        .unwrap_err()
        .to_string();

    assert!(error.contains("surprise"), "{error}");
    assert!(!world.installed("plymouth/themes/omaboot").exists());
    assert!(runner.calls().is_empty());
}

#[test]
fn step_1_an_unknown_theme_name_says_where_themes_live() {
    let world = World::new();
    let runner = RecordingRunner::new();
    let error = world
        .pipeline(&runner)
        .apply(&ApplyRequest::new("nope"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("omaboot list"), "{error}");
}

#[test]
fn step_2_generation_fails_when_the_packaged_assets_are_missing() {
    let world = World::new();
    let runner = RecordingRunner::new();
    let pipeline = Pipeline::new(&world.layout, &runner)
        .with_assets(AssetSource::at(world.root.join("nowhere")));
    let error = pipeline
        .apply(&ApplyRequest::new("test"))
        .unwrap_err()
        .to_string();

    assert!(error.contains("bullet.png"), "{error}");
    assert!(!world.installed("plymouth/themes/omaboot").exists());
}

#[test]
fn step_3_staging_onto_an_unusable_path_stops_before_the_greeter_runs() {
    let world = World::new();
    // A regular file where the stage directory belongs: rm -r cannot clear it.
    fs::create_dir_all(world.layout.state_dir()).unwrap();
    fs::write(world.layout.stage_dir(), b"in the way").unwrap();

    let runner = RecordingRunner::new();
    let error = world.apply(&runner).unwrap_err().to_string();

    assert!(error.contains("stage"), "{error}");
    assert!(
        !runner.ran_containing("sddm-greeter"),
        "the greeter must not run"
    );
    assert!(!world.installed("plymouth/themes/omaboot").exists());
}

#[test]
fn step_4_a_greeter_that_fails_its_smoke_test_is_never_installed() {
    let world = World::new();
    let runner = RecordingRunner::new().fail_containing(
        "sddm-greeter",
        CommandOutput::failure(1, "file:///Main.qml:12 Expected token `}`"),
    );
    let error = world.apply(&runner).unwrap_err().to_string();

    assert!(error.contains("Expected token"), "{error}");
    assert!(error.contains("instead of staying up"), "{error}");
    assert!(error.contains("login is untouched"), "{error}");
    assert!(!world.installed("sddm/themes/omaboot").exists());
    assert!(world.dropin().is_none());
    assert_eq!(world.plymouth_theme().as_deref(), Some("omarchy"));
    assert!(
        crate::state::read_rollback(&world.layout)
            .unwrap()
            .is_none()
    );
}

#[test]
fn an_apply_that_lock_screen_explorer_would_hide_is_refused_before_staging() {
    let world = World::new();
    let state = world
        .layout
        .state_base()
        .join(crate::state::LOCK_EXPLORER_BOOT_STATE);
    fs::create_dir_all(state.parent().unwrap()).unwrap();
    fs::write(&state, "terminal\n").unwrap();

    let runner = RecordingRunner::new();
    let error = world.apply(&runner).unwrap_err().to_string();
    assert!(error.contains("Lock Screen Explorer"), "{error}");
    assert!(error.contains("set to terminal"), "{error}");
    assert!(error.contains("setBoot stock"), "{error}");
    assert!(runner.calls().is_empty(), "{:?}", runner.calls());
    assert!(!world.layout.stage_dir().join("sddm").exists());
    assert!(!world.installed("sddm/themes/omaboot").exists());

    // A dry run says the same as a problem on the validate step and goes on.
    let report = dry("test", &world, &runner).unwrap();
    let problems = report.problems();
    assert!(
        problems.iter().any(|p| p.contains("Lock Screen Explorer")),
        "{problems:?}"
    );

    // Set back to stock, the same apply goes through.
    fs::write(&state, "stock\n").unwrap();
    world.apply(&runner).unwrap();
}

#[test]
fn a_helper_from_an_older_build_is_refused_before_anything_privileged_runs() {
    let check = Operation::CheckHelper {
        helper: PathBuf::from("/home/me/.local/bin/omaboot-apply"),
    };

    let older = RecordingRunner::new()
        .fail_containing("omaboot-apply protocol", CommandOutput::saying("1\n"));
    let error = Executor::new(&older, false)
        .perform(&check)
        .unwrap_err()
        .to_string();
    assert!(error.contains("speaks protocol 1"), "{error}");
    assert!(error.contains("cargo build --release"), "{error}");
    assert!(!older.ran_containing("sudo"), "{:?}", older.calls());

    let unaware = RecordingRunner::new().fail_containing(
        "omaboot-apply protocol",
        CommandOutput::failure(2, "error: unrecognized subcommand 'protocol'"),
    );
    let error = Executor::new(&unaware, false)
        .perform(&check)
        .unwrap_err()
        .to_string();
    assert!(error.contains("does not answer `protocol`"), "{error}");
    assert!(error.contains("unrecognized subcommand"), "{error}");

    let fresh = RecordingRunner::new();
    Executor::new(&fresh, false).perform(&check).unwrap();
    assert_eq!(
        fresh.calls(),
        vec!["/home/me/.local/bin/omaboot-apply protocol".to_string()]
    );
}

#[test]
fn every_plan_that_runs_the_helper_asks_its_protocol_first() {
    // A layout without a prefix is the only one that runs the helper; in a
    // dry run nothing is performed, so the plans can be read on any machine.
    let tmp = tempfile::tempdir().unwrap();
    let layout = Layout::with_dirs(None, tmp.path().join("config"), tmp.path().join("state"));
    fs::create_dir_all(layout.state_dir()).unwrap();
    let runner = RecordingRunner::new();
    let pipeline = Pipeline::new(&layout, &runner);
    let first = |report: ApplyReport| {
        let step = report.steps.into_iter().next().expect("one step");
        step.operations.into_iter().next().expect("one operation")
    };
    let is_check = |operation: &Operation| matches!(operation, Operation::CheckHelper { .. });

    assert!(is_check(&first(pipeline.reset(true).unwrap())));
    assert!(is_check(&first(pipeline.login_stock(true).unwrap())));
    assert!(is_check(&first(pipeline.login_release(true).unwrap())));

    let point = crate::state::RollbackPoint {
        version: crate::state::STATE_VERSION,
        recorded_at_unix: 0,
        previous_plymouth_theme: Some("omarchy".to_string()),
        sddm_dropin_existed: false,
        previous_sddm_dropin: None,
        theme: "test".to_string(),
        theme_hash: "x".to_string(),
    };
    fs::write(
        layout.rollback_file(),
        crate::state::serialize_rollback(&point),
    )
    .unwrap();
    assert!(is_check(&first(pipeline.revert(true).unwrap())));

    let install = pipeline.step_install(&[], true).unwrap();
    assert!(is_check(&install.operations[0]), "{:?}", install.operations);
    assert!(
        install.operations[1].to_string().contains("sudo"),
        "{}",
        install.operations[1]
    );
}

#[test]
fn step_4_a_greeter_that_stays_up_but_complains_about_the_theme_is_never_installed() {
    let world = World::new();
    let runner = RecordingRunner::new().fail_containing(
        "sddm-greeter",
        CommandOutput::stays_up_saying(
            "[12:00:00.300] (WW) GREETER: file:///stage/sddm/Main.qml:12:5: Expected token `}`\n\
             [12:00:00.301] (WW) GREETER: Fallback to embedded theme",
        ),
    );
    let error = world.apply(&runner).unwrap_err().to_string();
    assert!(error.contains("stayed up but complained"), "{error}");
    assert!(error.contains("Fallback to embedded theme"), "{error}");
    assert!(!world.installed("sddm/themes/omaboot").exists());
    assert!(world.dropin().is_none());
}

#[test]
fn step_4_a_greeter_that_exits_cleanly_too_soon_is_still_a_failure() {
    // The greeter in test mode only exits when its window is closed, which
    // cannot happen offscreen; an early exit 0 is not a pass.
    let world = World::new();
    let runner =
        RecordingRunner::new().fail_containing("sddm-greeter", CommandOutput::failure(0, ""));
    let error = world.apply(&runner).unwrap_err().to_string();
    assert!(error.contains("exit code 0"), "{error}");
    assert!(error.contains("instead of staying up"), "{error}");
    assert!(!world.installed("sddm/themes/omaboot").exists());
}

#[test]
fn step_4_the_smoke_test_requires_the_greeter_to_stay_up_offscreen() {
    let world = World::new();
    let runner = RecordingRunner::new();
    world.apply(&runner).unwrap();
    let call = runner
        .calls()
        .into_iter()
        .find(|call| call.contains("--test-mode"))
        .expect("the greeter ran");
    assert!(
        call.starts_with("QT_FORCE_STDERR_LOGGING=1 QT_QPA_PLATFORM=offscreen "),
        "{call}"
    );
    assert!(call.contains("--theme"), "{call}");
}

#[test]
fn step_4_a_missing_greeter_is_a_refusal_on_a_real_run_and_a_warning_in_a_dry_run() {
    let world = World::new();
    fs::remove_file(world.root.join("usr/bin/sddm-greeter-qt6")).unwrap();
    let runner = RecordingRunner::new();

    let error = world.apply(&runner).unwrap_err().to_string();
    assert!(error.contains("sddm"), "{error}");

    let report = dry("test", &world, &runner).unwrap();
    assert!(
        report.problems().iter().any(|p| p.contains("sddm-greeter")),
        "{:?}",
        report.problems()
    );
    assert!(report.render().contains("would stop a real run"));
}

#[test]
fn step_6_installing_over_a_symlink_is_refused() {
    let world = World::new();
    let victim = world.root.join("victim");
    fs::write(&victim, b"precious").unwrap();
    let destination = world.installed("plymouth/themes/omaboot");
    fs::create_dir_all(&destination).unwrap();
    std::os::unix::fs::symlink(&victim, destination.join("logo.png")).unwrap();

    let runner = RecordingRunner::new();
    let error = world.apply(&runner).unwrap_err().to_string();

    assert!(error.contains("symlink"), "{error}");
    assert_eq!(fs::read(&victim).unwrap(), b"precious");
    assert!(world.dropin().is_none());
}

#[test]
fn step_7_the_rollback_point_is_on_disk_before_anything_switches() {
    let world = World::new();
    // Fail the switch by making the file it writes unwritable through a symlink.
    let conf = world.layout.plymouthd_conf();
    fs::remove_file(&conf).unwrap();
    std::os::unix::fs::symlink(world.root.join("elsewhere"), &conf).unwrap();

    let runner = RecordingRunner::new();
    let error = world.apply(&runner).unwrap_err().to_string();

    assert!(error.contains("symlink"), "{error}");
    let rollback = crate::state::read_rollback(&world.layout).unwrap();
    assert!(
        rollback.is_some(),
        "the rollback point must exist even though the switch failed"
    );
}

#[test]
fn step_9_a_failed_initramfs_rebuild_reverts_to_the_recorded_point() {
    let world = World::new();
    let runner = RecordingRunner::new().fail_once_containing(
        "limine-mkinitcpio",
        CommandOutput::failure(1, "==> ERROR: missing module"),
    );
    let error = world.apply(&runner).unwrap_err().to_string();

    assert!(error.contains("missing module"), "{error}");
    assert!(error.contains("back where it was"), "{error}");
    assert_eq!(world.plymouth_theme().as_deref(), Some("omarchy"));
    assert!(world.dropin().is_none(), "the drop-in must be gone again");
    assert!(crate::state::read_applied(&world.layout).unwrap().is_none());
}

#[test]
fn step_10_a_file_that_does_not_match_what_was_staged_reverts() {
    let world = World::new();
    let runner = RecordingRunner::new();
    world.apply(&runner).unwrap();

    // Something changed the installed theme behind our back.
    fs::write(
        world.installed("plymouth/themes/omaboot/omaboot.script"),
        b"tampered",
    )
    .unwrap();

    let mut request = ApplyRequest::new("test");
    request.only = Some(vec![StepId::Verify]);
    let error = world
        .pipeline(&runner)
        .apply(&request)
        .unwrap_err()
        .to_string();

    assert!(error.contains("does not match the system"), "{error}");
    assert!(error.contains("back where it was"), "{error}");
    assert_eq!(world.plymouth_theme().as_deref(), Some("omarchy"));
    assert!(world.dropin().is_none());
}

#[test]
fn step_10_a_drop_in_that_outranks_ours_is_drift_and_names_the_file() {
    let world = World::new();
    let runner = RecordingRunner::new();
    world.apply(&runner).unwrap();

    // A login theme installed later, with a drop-in that sorts after ours.
    let rival = world.layout.sddm_conf_dir().join("zz-z-late-comer.conf");
    fs::write(&rival, "[Theme]\nCurrent=late-comer\n").unwrap();

    let mut request = ApplyRequest::new("test");
    request.only = Some(vec![StepId::Verify]);
    let error = world
        .pipeline(&runner)
        .apply(&request)
        .unwrap_err()
        .to_string();

    assert!(error.contains("zz-z-late-comer.conf"), "{error}");
    assert!(error.contains("late-comer"), "{error}");
    assert!(error.contains("rename or remove it"), "{error}");
}

#[test]
fn a_drop_in_that_sorts_before_ours_does_not_matter() {
    let world = World::new();
    let runner = RecordingRunner::new();
    fs::create_dir_all(world.layout.sddm_conf_dir()).unwrap();
    fs::write(
        world
            .layout
            .sddm_conf_dir()
            .join("99-z-omarchy-onscreen-keyboard.conf"),
        "[Theme]\nCurrent=omarchy-onscreen-keyboard\n",
    )
    .unwrap();
    let report = world.apply(&runner).unwrap();
    assert!(
        report.steps.iter().all(|step| step.problems.is_empty()),
        "{report:?}"
    );
    assert_eq!(
        crate::state::effective_sddm_theme(&world.layout)
            .unwrap()
            .name,
        "omaboot"
    );
}

#[test]
fn a_dry_run_says_it_will_check_which_drop_in_wins() {
    let world = World::new();
    let runner = RecordingRunner::new();
    let report = dry("test", &world, &runner).unwrap();
    let text: Vec<String> = report
        .steps
        .iter()
        .flat_map(|step| step.operations.iter().map(|op| op.to_string()))
        .collect();
    assert!(
        text.iter()
            .any(|line| line.contains("resolve [Theme] Current= to omaboot")),
        "{}",
        text.join("\n")
    );
}

#[test]
fn the_login_screen_can_be_given_to_omarchy_and_back_without_an_apply() {
    let world = World::new();
    let runner = RecordingRunner::new();
    fs::create_dir_all(world.layout.sddm_conf_dir()).unwrap();
    fs::write(
        world
            .layout
            .sddm_conf_dir()
            .join("99-z-omarchy-onscreen-keyboard.conf"),
        "[Theme]\nCurrent=omarchy-onscreen-keyboard\n",
    )
    .unwrap();
    assert_eq!(
        crate::state::effective_sddm_theme(&world.layout)
            .unwrap()
            .name,
        "omarchy-onscreen-keyboard"
    );

    world.pipeline(&runner).login_stock(false).unwrap();
    assert_eq!(
        world.dropin().as_deref(),
        Some("[Theme]\nCurrent=omarchy\n")
    );
    assert_eq!(
        crate::state::effective_sddm_theme(&world.layout)
            .unwrap()
            .name,
        "omarchy"
    );
    assert_eq!(
        world.plymouth_theme().as_deref(),
        Some("omarchy"),
        "Plymouth is untouched"
    );
    assert!(crate::state::read_applied(&world.layout).unwrap().is_none());

    world.pipeline(&runner).login_release(false).unwrap();
    assert!(world.dropin().is_none());
    assert_eq!(
        crate::state::effective_sddm_theme(&world.layout)
            .unwrap()
            .name,
        "omarchy-onscreen-keyboard"
    );
}

#[test]
fn the_login_screen_is_not_released_from_under_an_applied_theme() {
    let world = World::new();
    let runner = RecordingRunner::new();
    world.apply(&runner).unwrap();
    let error = world
        .pipeline(&runner)
        .login_release(false)
        .unwrap_err()
        .to_string();
    assert!(error.contains("revert"), "{error}");
    assert!(world.dropin().is_some());
}

#[test]
fn a_revert_that_fails_too_prints_the_rescue_commands() {
    let world = World::new();
    let runner = RecordingRunner::new().fail_containing(
        "limine-mkinitcpio",
        CommandOutput::failure(1, "==> ERROR: missing module"),
    );
    let error = world.apply(&runner).unwrap_err().to_string();

    assert!(
        error.contains("the automatic revert then failed as well"),
        "{error}"
    );
    assert!(
        error.contains("sudo plymouth-set-default-theme omarchy"),
        "{error}"
    );
    assert!(
        error.contains("sudo rm /etc/sddm.conf.d/zz-omaboot.conf"),
        "{error}"
    );
}

#[test]
fn a_revert_without_a_recorded_point_says_to_reset_instead() {
    let world = World::new();
    let runner = RecordingRunner::new();
    let error = world
        .pipeline(&runner)
        .revert(false)
        .unwrap_err()
        .to_string();
    assert!(error.contains("omaboot reset"), "{error}");
}

#[test]
fn revert_restores_the_previous_theme_and_removes_the_drop_in() {
    let world = World::new();
    let runner = RecordingRunner::new();
    world.apply(&runner).unwrap();
    assert_eq!(world.plymouth_theme().as_deref(), Some("omaboot"));

    world.pipeline(&runner).revert(false).unwrap();

    assert_eq!(world.plymouth_theme().as_deref(), Some("omarchy"));
    assert!(world.dropin().is_none());
    assert!(crate::state::read_applied(&world.layout).unwrap().is_none());
    assert!(
        crate::state::read_rollback(&world.layout)
            .unwrap()
            .is_none()
    );
    // The installed theme directory is left in place: it is inert once nothing
    // points at it, and reset is the command that removes it.
    assert!(
        world
            .installed("plymouth/themes/omaboot/omaboot.script")
            .is_file()
    );
}

#[test]
fn a_dry_run_revert_changes_nothing() {
    let world = World::new();
    let runner = RecordingRunner::new();
    world.apply(&runner).unwrap();
    world.pipeline(&runner).revert(true).unwrap();
    assert_eq!(world.plymouth_theme().as_deref(), Some("omaboot"));
    assert!(world.dropin().is_some());
}

#[test]
fn reset_returns_the_prefix_to_stock_and_removes_every_trace() {
    let world = World::new();
    let runner = RecordingRunner::new();
    world.apply(&runner).unwrap();

    world.pipeline(&runner).reset(false).unwrap();

    assert_eq!(world.plymouth_theme().as_deref(), Some("omarchy"));
    assert!(world.dropin().is_none());
    assert!(!world.installed("plymouth/themes/omaboot").exists());
    assert!(!world.installed("sddm/themes/omaboot").exists());
    assert!(!world.layout.state_dir().exists());
    // And the stock theme is untouched, because nothing ever wrote to it.
    assert!(!world.installed("plymouth/themes/omarchy").exists());
}

#[test]
fn the_prefix_keeps_every_write_inside_itself() {
    let world = World::new();
    let runner = RecordingRunner::new();
    let report = dry("test", &world, &runner).unwrap();
    for operation in report.operations() {
        let Some(destination) = operation.destination() else {
            continue;
        };
        let inside_prefix = destination.starts_with(&world.root);
        let inside_user_dirs = destination.starts_with(world.layout.state_dir())
            || destination.starts_with(world.layout.themes_dir());
        assert!(
            inside_prefix || inside_user_dirs,
            "{} escapes the test world",
            destination.display()
        );
    }
}

fn _assert_paths_are_paths(_: &Path) {}
