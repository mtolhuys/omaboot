//! The apply pipeline.
//!
//! Ten steps, in the order `docs/ARCHITECTURE.md` gives them. Each step is a
//! list of operations, each step can be run on its own, and the system is
//! bootable between any two of them because activation is one theme name and
//! one drop-in file, never a half-copied directory.

mod operation;
mod plan;

use std::path::PathBuf;
use std::time::Duration;

pub use operation::{Executor, Operation};
pub use plan::{ApplyReport, StepReport};

use crate::error::{Error, Result};
use crate::exec::{CommandSpec, Runner, Tools};
use crate::generate::{AssetSource, GeneratedTheme, generate};
use crate::paths::{Layout, STOCK_THEME_ID, THEME_ID};
use crate::state::{
    self, AppliedState, InstalledFile, RollbackPoint, STATE_VERSION, sddm_dropin_contents,
};
use crate::theme::{Theme, ValidTheme};

/// How long the greeter gets to render before the smoke test gives up.
pub const SMOKE_TEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StepId {
    Validate,
    Generate,
    Stage,
    SmokeTest,
    Authorise,
    Install,
    RecordRollback,
    Switch,
    Initramfs,
    Verify,
}

impl StepId {
    pub const ALL: [StepId; 10] = [
        StepId::Validate,
        StepId::Generate,
        StepId::Stage,
        StepId::SmokeTest,
        StepId::Authorise,
        StepId::Install,
        StepId::RecordRollback,
        StepId::Switch,
        StepId::Initramfs,
        StepId::Verify,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Self::Validate => "validate",
            Self::Generate => "generate",
            Self::Stage => "stage",
            Self::SmokeTest => "smoke-test",
            Self::Authorise => "authorise",
            Self::Install => "install",
            Self::RecordRollback => "record-rollback",
            Self::Switch => "switch",
            Self::Initramfs => "initramfs",
            Self::Verify => "verify",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Validate => "Validate theme",
            Self::Generate => "Generate assets",
            Self::Stage => "Stage theme",
            Self::SmokeTest => "Smoke test greeter",
            Self::Authorise => "Authorise",
            Self::Install => "Install into the omaboot directories",
            Self::RecordRollback => "Record rollback point",
            Self::Switch => "Switch",
            Self::Initramfs => "Rebuild initramfs",
            Self::Verify => "Verify",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|step| step.slug() == value)
    }

    /// True from the moment the system starts pointing at the new theme.
    /// A failure at or after this point reverts.
    pub fn switches(self) -> bool {
        self >= Self::Switch
    }
}

/// Watches the pipeline run.
pub trait Observer {
    fn step_started(&mut self, step: StepId) {
        let _ = step;
    }

    fn step_finished(&mut self, step: StepId) {
        let _ = step;
    }
}

/// The observer the CLI uses: it wants the report at the end, not the
/// progress along the way.
pub struct Silent;

impl Observer for Silent {}

#[derive(Debug, Clone)]
pub struct ApplyRequest {
    pub theme: String,
    pub dry_run: bool,
    /// When set, only these steps run. Everything before them that is pure
    /// (validation and generation) still runs, because later steps need it.
    pub only: Option<Vec<StepId>>,
}

impl ApplyRequest {
    pub fn new(theme: impl Into<String>) -> Self {
        Self {
            theme: theme.into(),
            dry_run: false,
            only: None,
        }
    }

    fn wants(&self, step: StepId) -> bool {
        match &self.only {
            None => true,
            Some(steps) => steps.contains(&step),
        }
    }
}

/// The pipeline, bound to one layout and one way of running commands.
#[derive(Debug)]
pub struct Pipeline<'a> {
    layout: &'a Layout,
    runner: &'a dyn Runner,
    assets: AssetSource,
    tools: Tools,
}

impl<'a> Pipeline<'a> {
    pub fn new(layout: &'a Layout, runner: &'a dyn Runner) -> Self {
        Self {
            assets: AssetSource::discover(layout),
            tools: Tools::detect(layout),
            layout,
            runner,
        }
    }

    pub fn with_assets(mut self, assets: AssetSource) -> Self {
        self.assets = assets;
        self
    }

    pub fn tools(&self) -> &Tools {
        &self.tools
    }

    pub fn layout(&self) -> &Layout {
        self.layout
    }

    /// Load and validate a theme by name.
    pub fn load_theme(&self, name: &str) -> Result<ValidTheme> {
        let dir = self.layout.theme_dir(name);
        if !dir.is_dir() {
            return Err(Error::ThemeNotFound {
                name: name.to_string(),
                dir: self.layout.themes_dir(),
            });
        }
        Theme::load(dir)
    }

    /// Told what the pipeline is doing, step by step, so a UI can fill in a
    /// list while it runs. The CLI passes one that does nothing.
    pub fn apply(&self, request: &ApplyRequest) -> Result<ApplyReport> {
        self.apply_observed(request, &mut Silent)
    }

    pub fn apply_observed(
        &self,
        request: &ApplyRequest,
        observer: &mut dyn Observer,
    ) -> Result<ApplyReport> {
        let executor = Executor::new(self.runner, request.dry_run);
        let mut report = ApplyReport::new(&request.theme, request.dry_run);

        // Validation and generation are pure and always run: every later step
        // is described in terms of what they produce.
        observer.step_started(StepId::Validate);
        let theme = self.load_theme(&request.theme)?;
        report.push(self.step_validate(&theme));
        observer.step_finished(StepId::Validate);

        observer.step_started(StepId::Generate);
        let generated = generate(&theme, &self.assets)?;
        let theme_hash = generated.hash()?;
        report.theme_hash = theme_hash.clone();
        report.push(self.step_generate(&generated)?);
        observer.step_finished(StepId::Generate);

        let staged = self.staged_files(&generated)?;

        for step in [
            StepId::Stage,
            StepId::SmokeTest,
            StepId::Authorise,
            StepId::Install,
            StepId::RecordRollback,
            StepId::Switch,
            StepId::Initramfs,
            StepId::Verify,
        ] {
            if !request.wants(step) {
                report.push(StepReport::skipped(step));
                continue;
            }

            observer.step_started(step);
            let planned = match step {
                StepId::Stage => self.step_stage(&generated),
                StepId::SmokeTest => self.step_smoke_test(request.dry_run),
                StepId::Authorise => self.step_authorise(request.dry_run),
                StepId::Install => self.step_install(&staged, request.dry_run),
                StepId::RecordRollback => self.step_record_rollback(&request.theme, &theme_hash),
                StepId::Switch => self.step_switch(request.dry_run),
                StepId::Initramfs => self.step_initramfs(request.dry_run),
                StepId::Verify => self.step_verify(&staged, &request.theme, &theme_hash),
                // Validation and generation ran before this loop; they are
                // prerequisites of every step in it.
                StepId::Validate | StepId::Generate => continue,
            };

            let planned = match planned {
                Ok(planned) => planned,
                Err(error) => return Err(self.recover(step, error, request.dry_run, &mut report)),
            };

            for operation in &planned.operations {
                if let Err(error) = executor.perform(operation) {
                    report.push(planned.clone());
                    return Err(self.recover(step, error, request.dry_run, &mut report));
                }
            }
            report.push(planned);
            observer.step_finished(step);
        }

        Ok(report)
    }

    /// A failure at or after the switch puts the recorded state back before
    /// the error reaches the caller.
    fn recover(
        &self,
        step: StepId,
        error: Error,
        dry_run: bool,
        report: &mut ApplyReport,
    ) -> Error {
        if dry_run || !step.switches() {
            return error;
        }
        match self.revert(false) {
            Ok(_) => {
                report.reverted = true;
                Error::step(
                    step.slug(),
                    format!("{error}"),
                    "the recorded rollback point was restored, so the system is back where it was; \
                     fix the cause and run omaboot apply again",
                )
            }
            Err(revert_error) => Error::step(
                step.slug(),
                format!("{error}; the automatic revert then failed as well: {revert_error}"),
                "recover from a TTY with: sudo plymouth-set-default-theme omarchy, \
                 sudo rm /etc/sddm.conf.d/zz-omaboot.conf, sudo mkinitcpio -P",
            ),
        }
    }

    // ---------------------------------------------------------------- steps

    fn step_validate(&self, theme: &ValidTheme) -> StepReport {
        let manifest = theme.manifest();
        let mut operations = vec![
            Operation::check(format!(
                "{} parses, every key is known",
                theme.dir().join(crate::theme::MANIFEST).display()
            )),
            Operation::check(format!(
                "logo {} is a regular file, not a symlink, within the size limit",
                theme.logo().display()
            )),
        ];
        if theme.shutdown_logo() != theme.logo() {
            operations.push(Operation::check(format!(
                "shutdown logo {} is a regular file",
                theme.shutdown_logo().display()
            )));
        }
        if let Some(background) = theme.login_background() {
            operations.push(Operation::check(format!(
                "login background {} is a regular file",
                background.display()
            )));
        }
        operations.push(Operation::check(format!(
            "colours background={} foreground={} accent={} error={}",
            manifest.colors.background,
            manifest.colors.foreground,
            manifest.colors.accent,
            manifest.colors.error
        )));
        StepReport::new(StepId::Validate, operations)
    }

    fn step_generate(&self, generated: &GeneratedTheme) -> Result<StepReport> {
        let mut operations = Vec::new();
        for (destination, files) in [
            (self.layout.plymouth_theme_dir(), &generated.plymouth),
            (self.layout.sddm_theme_dir(), &generated.sddm),
        ] {
            for file in files {
                let bytes = file.bytes()?;
                operations.push(Operation::check(format!(
                    "{} <- {} ({} bytes)",
                    destination.join(&file.name).display(),
                    file.origin(),
                    bytes.len()
                )));
            }
        }
        Ok(StepReport::new(StepId::Generate, operations))
    }

    fn step_stage(&self, generated: &GeneratedTheme) -> Result<StepReport> {
        let stage = self.layout.stage_dir();
        let mut operations = vec![
            Operation::RemoveDirAll {
                path: stage.clone(),
            },
            Operation::MakeDir {
                path: stage.join("plymouth"),
            },
            Operation::MakeDir {
                path: stage.join("sddm"),
            },
        ];
        for (sub, files) in [("plymouth", &generated.plymouth), ("sddm", &generated.sddm)] {
            for file in files {
                operations.push(Operation::write(
                    stage.join(sub).join(&file.name),
                    file.bytes()?,
                    file.origin(),
                ));
            }
        }
        Ok(StepReport::new(StepId::Stage, operations))
    }

    fn step_smoke_test(&self, dry_run: bool) -> Result<StepReport> {
        let staged_sddm = self.layout.stage_dir().join("sddm");
        match self.tools.greeter.as_ref() {
            Some(greeter) => {
                let spec = CommandSpec::new(greeter.path.display().to_string())
                    .arg("--test-mode")
                    .args(["--theme", &staged_sddm.display().to_string()])
                    .env("QT_QPA_PLATFORM", "offscreen")
                    .timeout(SMOKE_TEST_TIMEOUT);
                Ok(StepReport::new(
                    StepId::SmokeTest,
                    vec![Operation::run(
                        spec,
                        "the greeter smoke test, which decides whether this theme is ever shown at login",
                    )],
                ))
            }
            None if dry_run => Ok(StepReport::with_problem(
                StepId::SmokeTest,
                vec![Operation::check(
                    "run the staged greeter under --test-mode with QT_QPA_PLATFORM=offscreen",
                )],
                "neither sddm-greeter-qt6 nor sddm-greeter was found, so this step would fail",
            )),
            None => Err(self.tools.require_greeter().unwrap_err()),
        }
    }

    fn step_authorise(&self, dry_run: bool) -> Result<StepReport> {
        if self.layout.is_prefixed() {
            return Ok(StepReport::new(
                StepId::Authorise,
                vec![Operation::check(
                    "no authorisation is needed: --root writes inside the prefix only",
                )],
            ));
        }
        let _ = dry_run;
        Ok(StepReport::new(
            StepId::Authorise,
            vec![Operation::run(
                CommandSpec::sudo("-v"),
                "one authorisation for the whole operation, rather than one per file",
            )],
        ))
    }

    fn step_install(&self, staged: &[StagedFile], dry_run: bool) -> Result<StepReport> {
        if self.layout.is_prefixed() {
            // Under a prefix the pipeline installs directly, unprivileged. The
            // privileged helper has its own tests; running it here would need
            // root, which a test must never have.
            let mut operations = vec![
                Operation::MakeDir {
                    path: self.layout.plymouth_theme_dir(),
                },
                Operation::MakeDir {
                    path: self.layout.sddm_theme_dir(),
                },
            ];
            for file in staged {
                operations.push(Operation::Copy {
                    from: file.staged.clone(),
                    to: file.installed.clone(),
                    bytes: file.bytes,
                });
            }
            return Ok(StepReport::new(StepId::Install, operations));
        }

        let helper = match self.tools.helper.as_ref() {
            Some(helper) => helper.clone(),
            None if dry_run => PathBuf::from("omaboot-apply"),
            None => return Err(self.tools.require_helper().unwrap_err()),
        };

        let mut operations = vec![Operation::run(
            CommandSpec::sudo(helper.display().to_string())
                .arg("install")
                .args(["--staged", &self.layout.stage_dir().display().to_string()]),
            "the privileged helper, which publishes each file atomically",
        )];
        for file in staged {
            operations.push(Operation::check(format!(
                "the helper will publish {}",
                file.installed.display()
            )));
        }
        Ok(StepReport::new(StepId::Install, operations))
    }

    fn step_record_rollback(&self, theme: &str, theme_hash: &str) -> Result<StepReport> {
        let previous_dropin = state::read_sddm_dropin(self.layout);
        let point = RollbackPoint {
            version: STATE_VERSION,
            recorded_at_unix: state::now_unix(),
            previous_plymouth_theme: state::current_plymouth_theme(self.layout),
            sddm_dropin_existed: previous_dropin.is_some(),
            previous_sddm_dropin: previous_dropin,
            theme: theme.to_string(),
            theme_hash: theme_hash.to_string(),
        };
        Ok(StepReport::new(
            StepId::RecordRollback,
            vec![Operation::write(
                self.layout.rollback_file(),
                state::serialize_rollback(&point).into_bytes(),
                "rollback point, written before anything switches",
            )],
        ))
    }

    fn step_switch(&self, dry_run: bool) -> Result<StepReport> {
        let mut operations = vec![self.set_default_theme(THEME_ID, dry_run)?];
        operations.push(self.write_dropin(dry_run)?);
        Ok(StepReport::new(StepId::Switch, operations))
    }

    fn step_initramfs(&self, dry_run: bool) -> Result<StepReport> {
        match self.tools.initramfs.as_ref() {
            Some(initramfs) => Ok(StepReport::new(
                StepId::Initramfs,
                vec![Operation::run(
                    initramfs.command(),
                    "rebuilding the initramfs so the theme is present at boot",
                )],
            )),
            None if dry_run => Ok(StepReport::with_problem(
                StepId::Initramfs,
                vec![Operation::check("rebuild the initramfs")],
                "neither limine-mkinitcpio nor mkinitcpio was found, so this step would fail",
            )),
            None => Err(self.tools.require_initramfs().unwrap_err()),
        }
    }

    fn step_verify(
        &self,
        staged: &[StagedFile],
        theme: &str,
        theme_hash: &str,
    ) -> Result<StepReport> {
        let mut operations = Vec::new();
        let mut files = Vec::new();
        for file in staged {
            operations.push(Operation::VerifyFile {
                path: file.installed.clone(),
                sha256: file.sha256.clone(),
                bytes: file.bytes,
            });
            files.push(InstalledFile {
                path: file.installed.display().to_string(),
                sha256: file.sha256.clone(),
                bytes: file.bytes,
            });
        }
        operations.push(Operation::VerifyFile {
            path: self.layout.sddm_dropin(),
            sha256: crate::hash::sha256_hex(sddm_dropin_contents(THEME_ID).as_bytes()),
            bytes: sddm_dropin_contents(THEME_ID).len() as u64,
        });
        operations.push(Operation::VerifySddmTheme {
            conf: self.layout.sddm_conf(),
            conf_dir: self.layout.sddm_conf_dir(),
            expected: THEME_ID.to_string(),
        });

        let state = AppliedState {
            version: STATE_VERSION,
            theme: theme.to_string(),
            theme_hash: theme_hash.to_string(),
            applied_at_unix: state::now_unix(),
            files,
        };
        operations.push(Operation::write(
            self.layout.applied_state_file(),
            state::serialize_applied(&state).into_bytes(),
            "applied state",
        ));
        Ok(StepReport::new(StepId::Verify, operations))
    }

    // -------------------------------------------------------------- recovery

    /// Restore the recorded rollback point.
    pub fn revert(&self, dry_run: bool) -> Result<ApplyReport> {
        let point = state::require_rollback(self.layout)?;
        let executor = Executor::new(self.runner, dry_run);
        let mut report = ApplyReport::new(&point.theme, dry_run);
        report.theme_hash = point.theme_hash.clone();

        let previous = point
            .previous_plymouth_theme
            .clone()
            .unwrap_or_else(|| STOCK_THEME_ID.to_string());

        let mut operations = vec![self.set_default_theme(&previous, dry_run)?];
        if point.sddm_dropin_existed {
            operations.push(self.write_dropin(dry_run)?);
        } else {
            operations.push(self.remove_dropin(dry_run)?);
        }
        if let Some(initramfs) = self.tools.initramfs.as_ref() {
            operations.push(Operation::run(
                initramfs.command(),
                "rebuilding the initramfs after the revert",
            ));
        }
        operations.push(Operation::RemoveFile {
            path: self.layout.applied_state_file(),
        });
        operations.push(Operation::RemoveFile {
            path: self.layout.rollback_file(),
        });

        for operation in &operations {
            executor.perform(operation)?;
        }
        report.push(StepReport::named("revert", operations));
        Ok(report)
    }

    /// Point SDDM at Omarchy's own login theme with omaboot's drop-in, which
    /// outranks a third-party theme's drop-in without touching that file.
    /// No theme is installed and nothing about Plymouth changes.
    pub fn login_stock(&self, dry_run: bool) -> Result<ApplyReport> {
        let executor = Executor::new(self.runner, dry_run);
        let mut report = ApplyReport::new("(omarchy login screen)", dry_run);
        let operation = if self.layout.is_prefixed() {
            Operation::write(
                self.layout.sddm_dropin(),
                sddm_dropin_contents(STOCK_THEME_ID).into_bytes(),
                "sddm drop-in pointing at omarchy",
            )
        } else {
            let helper = self.helper_path(dry_run)?;
            Operation::run(
                CommandSpec::sudo(helper.display().to_string())
                    .arg("switch")
                    .arg("--stock"),
                format!(
                    "writing {} with Current=omarchy",
                    self.layout.sddm_dropin().display()
                ),
            )
        };
        let operations = vec![
            operation,
            Operation::VerifySddmTheme {
                conf: self.layout.sddm_conf(),
                conf_dir: self.layout.sddm_conf_dir(),
                expected: STOCK_THEME_ID.to_string(),
            },
        ];
        for operation in &operations {
            executor.perform(operation)?;
        }
        report.push(StepReport::named("use Omarchy's login screen", operations));
        Ok(report)
    }

    /// Remove omaboot's drop-in, so whatever else is configured decides the
    /// login screen again. Refused while a theme of yours is applied: that
    /// drop-in belongs to the apply, and revert or reset is the way out.
    pub fn login_release(&self, dry_run: bool) -> Result<ApplyReport> {
        if state::read_applied(self.layout)?.is_some() {
            return Err(Error::Environment {
                what: "a theme of yours is applied, and its drop-in is part of that".to_string(),
                suggestion: "revert the apply, or remove omaboot from the system".to_string(),
            });
        }
        let executor = Executor::new(self.runner, dry_run);
        let mut report = ApplyReport::new("(login screen released)", dry_run);
        let operations = vec![self.remove_dropin(dry_run)?];
        for operation in &operations {
            executor.perform(operation)?;
        }
        report.push(StepReport::named("give the login screen back", operations));
        Ok(report)
    }

    /// Return the system to stock Omarchy and remove every trace of omaboot.
    pub fn reset(&self, dry_run: bool) -> Result<ApplyReport> {
        let executor = Executor::new(self.runner, dry_run);
        let mut report = ApplyReport::new("(stock omarchy)", dry_run);

        let mut operations = vec![
            self.set_default_theme(STOCK_THEME_ID, dry_run)?,
            self.remove_dropin(dry_run)?,
        ];

        if self.layout.is_prefixed() {
            operations.push(Operation::RemoveDirAll {
                path: self.layout.plymouth_theme_dir(),
            });
            operations.push(Operation::RemoveDirAll {
                path: self.layout.sddm_theme_dir(),
            });
        } else {
            let helper = self.helper_path(dry_run)?;
            operations.push(Operation::run(
                CommandSpec::sudo(helper.display().to_string()).arg("remove"),
                "removing the omaboot theme directories",
            ));
        }

        if let Some(initramfs) = self.tools.initramfs.as_ref() {
            operations.push(Operation::run(
                initramfs.command(),
                "rebuilding the initramfs after returning to stock",
            ));
        }
        operations.push(Operation::RemoveDirAll {
            path: self.layout.state_dir(),
        });

        for operation in &operations {
            executor.perform(operation)?;
        }
        report.push(StepReport::named("reset", operations));
        Ok(report)
    }

    // --------------------------------------------------------------- helpers

    fn helper_path(&self, dry_run: bool) -> Result<PathBuf> {
        match self.tools.helper.as_ref() {
            Some(helper) => Ok(helper.clone()),
            None if dry_run => Ok(PathBuf::from("omaboot-apply")),
            None => Err(self.tools.require_helper().unwrap_err()),
        }
    }

    fn set_default_theme(&self, theme: &str, dry_run: bool) -> Result<Operation> {
        let tool = if self.layout.is_prefixed() {
            None
        } else {
            match self.tools.plymouth_set_default.as_ref() {
                Some(tool) => Some(tool.clone()),
                None if dry_run => Some(PathBuf::from("plymouth-set-default-theme")),
                None => return Err(self.tools.require_plymouth_set_default().unwrap_err()),
            }
        };
        Ok(Operation::SetPlymouthDefault {
            theme: theme.to_string(),
            tool,
            conf: self.layout.plymouthd_conf(),
        })
    }

    fn write_dropin(&self, dry_run: bool) -> Result<Operation> {
        if self.layout.is_prefixed() {
            return Ok(Operation::write(
                self.layout.sddm_dropin(),
                sddm_dropin_contents(THEME_ID).into_bytes(),
                "sddm drop-in",
            ));
        }
        let helper = self.helper_path(dry_run)?;
        Ok(Operation::run(
            CommandSpec::sudo(helper.display().to_string())
                .arg("switch")
                .arg("--on"),
            format!("writing {}", self.layout.sddm_dropin().display()),
        ))
    }

    fn remove_dropin(&self, dry_run: bool) -> Result<Operation> {
        if self.layout.is_prefixed() {
            return Ok(Operation::RemoveFile {
                path: self.layout.sddm_dropin(),
            });
        }
        let helper = self.helper_path(dry_run)?;
        Ok(Operation::run(
            CommandSpec::sudo(helper.display().to_string())
                .arg("switch")
                .arg("--off"),
            format!("removing {}", self.layout.sddm_dropin().display()),
        ))
    }

    fn staged_files(&self, generated: &GeneratedTheme) -> Result<Vec<StagedFile>> {
        let stage = self.layout.stage_dir();
        let mut out = Vec::new();
        for (sub, destination, files) in [
            (
                "plymouth",
                self.layout.plymouth_theme_dir(),
                &generated.plymouth,
            ),
            ("sddm", self.layout.sddm_theme_dir(), &generated.sddm),
        ] {
            for file in files {
                let bytes = file.bytes()?;
                out.push(StagedFile {
                    staged: stage.join(sub).join(&file.name),
                    installed: destination.join(&file.name),
                    sha256: crate::hash::sha256_hex(&bytes),
                    bytes: bytes.len() as u64,
                });
            }
        }
        Ok(out)
    }
}

/// One file on its way from the stage to its destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedFile {
    pub staged: PathBuf,
    pub installed: PathBuf,
    pub sha256: String,
    pub bytes: u64,
}

#[cfg(test)]
mod tests;
