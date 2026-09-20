//! The command line.
//!
//! This is the whole interface. The Quattro plugin is a front end over these
//! same commands, with `--json`, not a second implementation: every rule
//! lives once, here and below.

use std::io::Write;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::apply::{ApplyReport, ApplyRequest, Observer, Pipeline, StepId};
use crate::error::{Error, Result};
use crate::exec::{RealRunner, Runner, Tools};
use crate::generate::AssetSource;
use crate::omarchy::OmarchyThemes;
use crate::paths::{Layout, THEME_ID};
use crate::render;
use crate::scaffold;
use crate::state;
use crate::theme::Theme;

#[derive(Debug, Parser)]
#[command(
    name = "omaboot",
    version,
    about = "Design the Plymouth unlock, SDDM login and Plymouth shutdown screens on Omarchy",
    long_about = None
)]
pub struct Cli {
    /// With no subcommand, omaboot opens its Quattro plugin in omarchy-shell.
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Print the exact operations that would be performed, and perform none.
    #[arg(long, global = true)]
    pub dry_run: bool,

    /// Write results as JSON, one object per line for long-running commands.
    /// This is what the plugin reads.
    #[arg(long, global = true)]
    pub json: bool,

    /// Read the sudo password from the first line of stdin, acquire a ticket
    /// with it, and never wait for a terminal afterwards. For front ends.
    #[arg(long, global = true)]
    pub password_stdin: bool,

    /// Treat this directory as the root of the system. Implies that no real
    /// system command runs: the pipeline writes inside the prefix only.
    #[arg(long, global = true, value_name = "PREFIX")]
    pub root: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List the themes you have, and which one is applied
    List,
    /// Show the applied theme, drift, and the rollback target
    Status,
    /// Check a theme without changing anything
    Validate { theme: String },
    /// Scaffold a new theme
    New {
        name: String,
        /// Start from an Omarchy theme: take its colours and its unlock.png
        #[arg(long, value_name = "THEME", conflicts_with = "from_current")]
        from_omarchy_theme: Option<String>,
        /// Start from the screens that boot now: the installed theme's
        /// colours and logo, or a copy of your applied theme
        #[arg(long)]
        from_current: bool,
    },
    /// Show a screen with the real daemons: the greeter in test mode, or
    /// plymouthd drawing into a window. Installs nothing.
    Preview {
        /// One of your themes. Leave it out with --current to show the
        /// screens the system has right now.
        #[arg(required_unless_present = "current")]
        theme: Option<String>,
        /// Show what boots now: the installed Plymouth theme and the login
        /// theme SDDM resolves to. Nothing is staged and nothing is
        /// published; the daemons read what is already there.
        #[arg(long, conflicts_with = "theme")]
        current: bool,
        /// Which screen: unlock, login or shutdown
        #[arg(long, value_name = "SCREEN", default_value = "unlock")]
        screen: String,
        /// How long to keep it up, in seconds, when nothing ends it sooner
        #[arg(long, value_name = "N", default_value_t = 30)]
        seconds: u64,
        /// Save a PNG of the Plymouth preview here (needs ImageMagick)
        #[arg(long, value_name = "FILE")]
        screenshot: Option<PathBuf>,
    },
    /// Show one of your themes: manifest, editable properties, images
    Show { theme: String },
    /// Set properties of a theme, as key=value pairs, and save it
    Set {
        theme: String,
        /// For example colors.background=#1a1b26 or login.clock=false
        #[arg(required = true, value_name = "KEY=VALUE")]
        assignments: Vec<String>,
    },
    /// Change the name a theme is shown under
    Rename { theme: String, name: String },
    /// Remove one of your themes; the system is not touched
    Delete { theme: String },
    /// Copy an image into a theme and use it as its logo, shutdown logo or
    /// login background. The source file is left as it is.
    AddImage {
        theme: String,
        file: PathBuf,
        /// logo, shutdown-logo or background
        #[arg(long = "as", value_name = "ROLE", default_value = "logo")]
        role: String,
    },
    /// Remove an image from a theme, unless the theme still uses it
    RemoveImage { theme: String, name: String },
    /// The colours an image is made of, with a suggested background, text
    /// colour and accent
    Palette { file: PathBuf },
    /// Point SDDM at Omarchy's own login screen, or give the login screen
    /// back to whatever else is configured. No theme is installed.
    Login {
        #[command(subcommand)]
        action: LoginAction,
    },
    /// Read or set the window's preferences (its layout)
    Prefs {
        /// key=value pairs to set; none shows the preferences
        #[arg(value_name = "KEY=VALUE")]
        assignments: Vec<String>,
    },
    /// Install, remove or open the Quattro plugin in omarchy-shell
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },
    /// Run the window as an application of its own, with its own app id
    /// and icon; `app install` adds it to the app launcher
    App {
        #[command(subcommand)]
        action: Option<AppAction>,
    },
    /// Draw a screen of a theme to a PNG, without touching the system
    Render {
        /// One of your themes; leave it out with --current for the
        /// installed screens
        #[arg(required_unless_present = "current")]
        theme: Option<String>,
        /// Draw what boots now, from the installed files
        #[arg(long, conflicts_with = "theme")]
        current: bool,
        /// Which screen to draw
        #[arg(long, value_name = "SCREEN", default_value = "unlock")]
        screen: String,
        /// Where to write the PNG
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
        /// The screen size to draw at, as WIDTHxHEIGHT
        #[arg(long, value_name = "WxH", default_value = "1920x1080")]
        size: String,
        /// How many password characters to show
        #[arg(long, value_name = "N", default_value_t = 5)]
        bullets: u32,
    },
    /// Run the apply pipeline
    Apply {
        theme: String,
        /// Run only these steps, by slug. Repeatable.
        #[arg(long, value_name = "STEP")]
        step: Vec<String>,
    },
    /// Restore the state recorded at the last apply
    Revert,
    /// Return the system to stock Omarchy and remove all omaboot state
    Reset,
}

#[derive(Debug, Subcommand)]
pub enum LoginAction {
    /// Write omaboot's drop-in with Current=omarchy, which outranks any
    /// other drop-in without touching it
    Stock,
    /// Remove omaboot's drop-in; refused while a theme of yours is applied
    Release,
}

#[derive(Debug, Subcommand)]
pub enum AppAction {
    /// Write the desktop entry and the icons, so launchers and docks know
    /// omaboot by name and face
    Install,
    /// Remove the desktop entry, the icons and the generated application
    Uninstall,
}

#[derive(Debug, Subcommand)]
pub enum PluginAction {
    /// Link the plugin into ~/.config/omarchy/plugins, put the binaries on
    /// PATH, and enable it in omarchy-shell
    Install,
    /// Remove the link and disable the plugin
    Uninstall,
    /// Summon the plugin in the running shell
    Open,
}

/// Run one command and write its output.
pub fn run(cli: Cli, out: &mut dyn Write) -> Result<()> {
    let dry_run = cli.dry_run;
    let json = cli.json;
    let layout = Layout::discover(cli.root.clone())?;

    if cli.password_stdin {
        if layout.is_prefixed() {
            // A prefixed run never runs sudo, so the password is not needed;
            // it is still consumed so a front end can always send it.
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
            crate::auth::set_non_interactive();
        } else {
            crate::auth::acquire_from_stdin()?;
        }
    }

    let Some(command) = cli.command else {
        // No subcommand: open the plugin, which is the interface.
        return crate::plugin::open(&layout, out);
    };

    // A prefixed run never executes a real system command. That is what makes
    // the whole pipeline safe to exercise in a test and on a development
    // machine.
    let runner: Box<dyn Runner> = if layout.is_prefixed() {
        Box::new(crate::exec::SimulatedRunner)
    } else {
        Box::new(RealRunner)
    };
    let pipeline = Pipeline::new(&layout, runner.as_ref());

    match command {
        Command::List if json => emit(out, &crate::api::inspect(&layout)),
        Command::List => list(&layout, out),
        Command::Status if json => emit(out, &crate::api::inspect(&layout)),
        Command::Status => status(&layout, out),
        Command::Show { theme } => emit(out, &crate::api::show(&layout, &theme)?),
        Command::Set { theme, assignments } => {
            let value = crate::api::set(&layout, &theme, &assignments)?;
            if json {
                emit(out, &value)
            } else {
                writeln!(out, "saved {}", value["dir"].as_str().unwrap_or(&theme)).ok();
                Ok(())
            }
        }
        Command::Rename { theme, name } => {
            let value = crate::api::rename(&layout, &theme, &name)?;
            if json {
                emit(out, &value)
            } else {
                writeln!(out, "renamed {theme} to {name}").ok();
                Ok(())
            }
        }
        Command::Delete { theme } => {
            let value = crate::api::delete(&layout, &theme)?;
            if json {
                emit(out, &value)
            } else {
                writeln!(
                    out,
                    "removed {}; {}",
                    value["removed"].as_str().unwrap_or(&theme),
                    value["note"].as_str().unwrap_or_default()
                )
                .ok();
                Ok(())
            }
        }
        Command::AddImage { theme, file, role } => {
            let role = crate::api::ImageRole::parse(&role)?;
            let value = crate::api::add_image(&layout, &theme, &file, role)?;
            if json {
                emit(out, &value)
            } else {
                writeln!(out, "added {} to {theme}", file.display()).ok();
                Ok(())
            }
        }
        Command::RemoveImage { theme, name } => {
            let value = crate::api::remove_image(&layout, &theme, &name)?;
            if json {
                emit(out, &value)
            } else {
                writeln!(out, "removed {name} from {theme}").ok();
                Ok(())
            }
        }
        Command::Palette { file } => {
            let value = crate::api::palette(&file)?;
            if json {
                emit(out, &value)
            } else {
                for colour in value["colours"].as_array().into_iter().flatten() {
                    writeln!(out, "{}", colour.as_str().unwrap_or_default()).ok();
                }
                writeln!(
                    out,
                    "suggested: background {} text {} accent {}",
                    value["suggested"]["background"]
                        .as_str()
                        .unwrap_or_default(),
                    value["suggested"]["foreground"]
                        .as_str()
                        .unwrap_or_default(),
                    value["suggested"]["accent"].as_str().unwrap_or_default()
                )
                .ok();
                Ok(())
            }
        }
        Command::Login { action } => {
            let report = match action {
                LoginAction::Stock => pipeline.login_stock(dry_run)?,
                LoginAction::Release => pipeline.login_release(dry_run)?,
            };
            if json {
                return emit_report(out, &report);
            }
            write!(out, "{}", report.render()).ok();
            Ok(())
        }
        Command::Prefs { assignments } => {
            let value = if assignments.is_empty() {
                crate::api::prefs(&layout)
            } else {
                crate::api::set_prefs(&layout, &assignments)?
            };
            emit(out, &value)
        }
        Command::Plugin { action } => match action {
            PluginAction::Install => {
                let source = crate::plugin::locate()?;
                crate::plugin::install(&layout, &source, dry_run, out)
            }
            PluginAction::Uninstall => crate::plugin::uninstall(&layout, dry_run, out),
            PluginAction::Open => crate::plugin::open(&layout, out),
        },
        Command::App { action } => match action {
            Some(AppAction::Install) => {
                let plugin = crate::plugin::locate()?;
                crate::app::install(&layout, &plugin, dry_run, out)
            }
            Some(AppAction::Uninstall) => crate::app::uninstall(&layout, dry_run, out),
            None => {
                let plugin = crate::plugin::locate()?;
                crate::app::run(&layout, &plugin, out)
            }
        },
        Command::Validate { theme } => {
            let theme = pipeline.load_theme(&theme)?;
            writeln!(
                out,
                "{} is valid: {} by {}, version {}",
                theme.dir().display(),
                theme.manifest().meta.name,
                if theme.manifest().meta.author.is_empty() {
                    "an unnamed author"
                } else {
                    &theme.manifest().meta.author
                },
                theme.manifest().meta.version
            )
            .ok();
            Ok(())
        }
        Command::New {
            name,
            from_omarchy_theme,
            from_current,
        } => {
            let source = match from_omarchy_theme {
                Some(theme) => scaffold::Source::OmarchyTheme(theme),
                None if from_current => scaffold::Source::Current,
                None => scaffold::Source::Blank,
            };
            if json && !dry_run {
                scaffold::create(&layout, &name, &source)?;
                return emit(out, &crate::api::show(&layout, name.trim())?);
            }
            new_theme(&layout, &name, &source, dry_run, out)
        }
        Command::Preview {
            theme,
            current,
            screen,
            seconds,
            screenshot,
        } => {
            let options = crate::preview::Options {
                screen: parse_screen(&screen)?,
                seconds,
                screenshot,
                ..crate::preview::Options::default()
            };
            let staged = match (&theme, current) {
                (Some(theme), _) => crate::preview::stage(&pipeline, theme)?,
                (None, _) => crate::preview::Staged::installed(&layout)?,
            };
            let plan = crate::preview::plan(&layout, pipeline.tools(), &staged, &options);
            if dry_run || layout.is_prefixed() {
                writeln!(
                    out,
                    "omaboot: preview of {} (would perform)\n",
                    staged.describe
                )
                .ok();
                for line in &plan {
                    writeln!(out, "    {line}").ok();
                }
                if layout.is_prefixed() && !dry_run {
                    writeln!(out, "\na live preview does not run against --root; the commands above are what it would do").ok();
                }
                return Ok(());
            }
            if !json {
                writeln!(out, "Enter here ends the preview early").ok();
            }
            // With the password already read, stdin has done its job; a
            // front end ends the preview by closing the window or waiting.
            let stdin = crate::preview::watch_stdin();
            let mut stop = || !cli.password_stdin && stdin.try_recv().is_ok();
            let mut say = |line: &str| {
                if json {
                    emit_line(out, &serde_json::json!({"event": "say", "message": line}));
                } else {
                    writeln!(out, "{line}").ok();
                }
                out.flush().ok();
            };
            crate::preview::run(
                &layout,
                pipeline.tools(),
                &staged,
                &options,
                &mut say,
                &mut stop,
            )
        }
        Command::Render {
            theme,
            current: _,
            screen,
            out: destination,
            size,
            bullets,
        } => {
            let assets = AssetSource::discover(&layout);
            let (width, height) = parse_size(&size)?;
            let screen = parse_screen(&screen)?;
            let (generated, manifest) = match theme {
                Some(theme) => {
                    let theme = pipeline.load_theme(&theme)?;
                    let generated = crate::generate::generate(&theme, &assets)?;
                    (generated, theme.manifest().clone())
                }
                None => {
                    let snapshot = crate::system::Snapshot::read(&layout);
                    snapshot.picture(screen, &assets).ok_or_else(|| Error::Environment {
                        what: format!(
                            "the installed {} screen cannot be drawn from its files",
                            screen.title().to_lowercase()
                        ),
                        suggestion: "omaboot status says which theme it is; preview --current shows the real thing"
                            .to_string(),
                    })?
                }
            };
            let canvas = render::composite(
                &generated,
                &manifest,
                screen,
                render::Geometry {
                    width,
                    height,
                    bullets,
                },
            )?;
            let bytes = render::encode_png(&canvas)?;
            state::write_atomic(&destination, &bytes)?;
            if json {
                return emit(
                    out,
                    &serde_json::json!({
                        "path": destination.display().to_string(),
                        "width": width,
                        "height": height,
                        "screen": screen.title().to_lowercase(),
                    }),
                );
            }
            writeln!(
                out,
                "wrote {} ({}x{}, {} screen, {} bytes)",
                destination.display(),
                width,
                height,
                screen.title().to_lowercase(),
                bytes.len()
            )
            .ok();
            Ok(())
        }
        Command::Apply { theme, step } => {
            let only = if step.is_empty() {
                None
            } else {
                let mut steps = Vec::new();
                for slug in &step {
                    steps.push(StepId::parse(slug).ok_or_else(|| Error::Environment {
                        what: format!("{slug} is not a step"),
                        suggestion: format!(
                            "use one of: {}",
                            StepId::ALL
                                .iter()
                                .map(|s| s.slug())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    })?);
                }
                Some(steps)
            };
            let request = ApplyRequest {
                theme,
                dry_run,
                only,
            };
            if json {
                let mut events = JsonEvents { out };
                let report = match pipeline.apply_observed(&request, &mut events) {
                    Ok(report) => report,
                    Err(error) => {
                        emit_line(
                            out,
                            &serde_json::json!({"event": "error", "message": error.to_string()}),
                        );
                        return Err(error);
                    }
                };
                return emit_report(out, &report);
            }
            let report = pipeline.apply(&request)?;
            write!(out, "{}", report.render()).ok();
            if let Some(closing) = closing_lines(&report) {
                writeln!(out, "\n{closing}").ok();
            }
            Ok(())
        }
        Command::Revert => {
            let report = pipeline.revert(dry_run)?;
            if json {
                return emit_report(out, &report);
            }
            write!(out, "{}", report.render()).ok();
            Ok(())
        }
        Command::Reset => {
            let report = pipeline.reset(dry_run)?;
            if json {
                return emit_report(out, &report);
            }
            write!(out, "{}", report.render()).ok();
            Ok(())
        }
    }
}

/// One JSON value, on one line, flushed: what a front end reads.
fn emit(out: &mut dyn Write, value: &serde_json::Value) -> Result<()> {
    emit_line(out, value);
    Ok(())
}

fn emit_line(out: &mut dyn Write, value: &serde_json::Value) {
    writeln!(out, "{value}").ok();
    out.flush().ok();
}

fn emit_report(out: &mut dyn Write, report: &crate::apply::ApplyReport) -> Result<()> {
    let steps: Vec<serde_json::Value> = report
        .steps
        .iter()
        .map(|step| {
            serde_json::json!({
                "step": step.id.map(|id| id.slug()),
                "title": step.title,
                "skipped": step.skipped,
                "operations": step.operations.iter().map(|op| op.to_string()).collect::<Vec<_>>(),
                "problems": step.problems,
            })
        })
        .collect();
    emit(
        out,
        &serde_json::json!({
            "event": "done",
            "ok": true,
            "theme": report.theme,
            "dry_run": report.dry_run,
            "reverted": report.reverted,
            "steps": steps,
            "problems": report.problems(),
        }),
    )
}

/// Step progress as JSON lines, for the plugin's progress view.
struct JsonEvents<'a> {
    out: &'a mut dyn Write,
}

impl Observer for JsonEvents<'_> {
    fn step_started(&mut self, step: StepId) {
        emit_line(
            self.out,
            &serde_json::json!({"event": "step", "step": step.slug(), "title": step.title(), "state": "started"}),
        );
    }

    fn step_finished(&mut self, step: StepId) {
        emit_line(
            self.out,
            &serde_json::json!({"event": "step", "step": step.slug(), "title": step.title(), "state": "finished"}),
        );
    }
}

fn parse_screen(value: &str) -> Result<render::Screen> {
    match value.to_ascii_lowercase().as_str() {
        "unlock" | "boot" => Ok(render::Screen::Unlock),
        "login" | "greeter" => Ok(render::Screen::Login),
        "shutdown" | "reboot" => Ok(render::Screen::Shutdown),
        other => Err(Error::Environment {
            what: format!("{other} is not a screen"),
            suggestion: "use unlock, login or shutdown".to_string(),
        }),
    }
}

fn parse_size(value: &str) -> Result<(u32, u32)> {
    let bad = || Error::Environment {
        what: format!("{value} is not a screen size"),
        suggestion: "write it as WIDTHxHEIGHT, for example 1920x1080".to_string(),
    };
    let (width, height) = value.split_once(['x', 'X']).ok_or_else(bad)?;
    let width: u32 = width.trim().parse().map_err(|_| bad())?;
    let height: u32 = height.trim().parse().map_err(|_| bad())?;
    if !(16..=16384).contains(&width) || !(16..=16384).contains(&height) {
        return Err(Error::Environment {
            what: format!("{value} is outside the sizes omaboot draws"),
            suggestion: "use a size between 16x16 and 16384x16384".to_string(),
        });
    }
    Ok((width, height))
}

fn list(layout: &Layout, out: &mut dyn Write) -> Result<()> {
    let applied = state::read_applied(layout)?;
    let dir = layout.themes_dir();
    let mut names: Vec<String> = match std::fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(source) => return Err(Error::read(dir.clone(), source)),
    };
    names.sort();

    if names.is_empty() {
        writeln!(out, "no themes in {}", dir.display()).ok();
        writeln!(out, "run `omaboot new <name>` to scaffold one").ok();
        return Ok(());
    }

    for name in names {
        let marker = match &applied {
            Some(state) if state.theme == name => "*",
            _ => " ",
        };
        let detail = match Theme::read(layout.theme_dir(&name)) {
            Ok(theme) => theme.manifest.meta.name,
            Err(error) => format!("unreadable: {error}"),
        };
        writeln!(out, "{marker} {name}  {detail}").ok();
    }
    Ok(())
}

/// What to say after an apply that was performed: the reboot line only
/// when the switch ran, since a partial run (`--step`) changes nothing
/// about what boots.
fn closing_lines(report: &ApplyReport) -> Option<String> {
    if report.dry_run {
        return None;
    }
    let switched = report
        .steps
        .iter()
        .any(|step| step.id == Some(StepId::Switch) && !step.skipped);
    if switched {
        return Some(
            "Reboot to see it. `omaboot revert` puts everything back.\n\
             If a screen ever fails, recover from a TTY with:\n  \
             sudo plymouth-set-default-theme omarchy\n  \
             sudo rm /etc/sddm.conf.d/zz-omaboot.conf\n  \
             sudo mkinitcpio -P"
                .to_string(),
        );
    }
    let ran: Vec<&str> = report
        .steps
        .iter()
        .filter(|step| !step.skipped)
        .filter_map(|step| step.id.map(StepId::slug))
        .collect();
    Some(format!(
        "Only {} ran; nothing was switched, what boots is unchanged.",
        ran.join(", ")
    ))
}

fn status(layout: &Layout, out: &mut dyn Write) -> Result<()> {
    let now = state::now_unix();
    let snapshot = crate::system::Snapshot::read(layout);

    writeln!(out, "what boots now").ok();
    for screen in [render::Screen::Unlock, render::Screen::Login] {
        let heading = match screen {
            render::Screen::Login => "  login (SDDM)",
            _ => "  boot and shutdown (Plymouth)",
        };
        writeln!(out, "{heading}").ok();
        for fact in snapshot.facts(screen) {
            if fact.label == "record" {
                continue;
            }
            writeln!(out, "    {:<12}{}", fact.label, fact.value).ok();
        }
    }

    writeln!(out).ok();
    match &snapshot.applied {
        None => {
            writeln!(
                out,
                "nothing is applied by omaboot; the system is on its own themes"
            )
            .ok();
        }
        Some(applied) => {
            writeln!(
                out,
                "applied: {} ({}), theme hash {}",
                applied.theme,
                state::describe_age(applied.applied_at_unix, now),
                applied.theme_hash.chars().take(16).collect::<String>()
            )
            .ok();

            let mut drifted = Vec::new();
            for file in &applied.files {
                let path = PathBuf::from(&file.path);
                match std::fs::read(&path) {
                    Ok(bytes) if crate::hash::sha256_hex(&bytes) == file.sha256 => {}
                    Ok(_) => drifted.push(format!("{} changed on disk", file.path)),
                    Err(_) => drifted.push(format!("{} is missing", file.path)),
                }
            }
            if drifted.is_empty() {
                writeln!(out, "drift:   none, {} files match", applied.files.len()).ok();
            } else {
                writeln!(out, "drift:   {} problem(s)", drifted.len()).ok();
                for problem in drifted {
                    writeln!(out, "         {problem}").ok();
                }
            }
        }
    }
    for warning in snapshot.warnings() {
        writeln!(out, "warning: {warning}").ok();
    }

    match state::read_rollback(layout)? {
        None => {
            writeln!(out, "rollback target:  (none recorded)").ok();
        }
        Some(point) => {
            writeln!(
                out,
                "rollback target:  plymouth {}, drop-in {} ({})",
                point
                    .previous_plymouth_theme
                    .as_deref()
                    .unwrap_or("(not set)"),
                if point.sddm_dropin_existed {
                    "restored"
                } else {
                    "removed"
                },
                state::describe_age(point.recorded_at_unix, now)
            )
            .ok();
        }
    }

    let tools = Tools::detect(layout);
    writeln!(
        out,
        "tools:            greeter {}, initramfs {}",
        tools
            .greeter
            .as_ref()
            .map(|g| g.name.clone())
            .unwrap_or_else(|| "(none found)".to_string()),
        tools
            .initramfs
            .as_ref()
            .map(|i| i.path.display().to_string())
            .unwrap_or_else(|| "(none found)".to_string())
    )
    .ok();
    let _ = THEME_ID;
    Ok(())
}

fn new_theme(
    layout: &Layout,
    name: &str,
    source: &scaffold::Source,
    dry_run: bool,
    out: &mut dyn Write,
) -> Result<()> {
    if dry_run {
        scaffold::check_name(layout, name)?;
        let dir = layout.theme_dir(name.trim());
        writeln!(out, "would create {}", dir.join("theme.toml").display()).ok();
        match source {
            scaffold::Source::Current => {
                let derived = scaffold::describe_current(layout, name)?;
                writeln!(out, "would take its values from {}", derived.from).ok();
                for (from, to) in &derived.files {
                    writeln!(
                        out,
                        "would copy {} to {}",
                        from.display(),
                        dir.join(to).display()
                    )
                    .ok();
                }
            }
            scaffold::Source::OmarchyTheme(theme) => {
                let unlock = OmarchyThemes::discover(layout).unlock_image(theme)?;
                writeln!(
                    out,
                    "would copy {} to {}",
                    unlock.display(),
                    dir.join("logo.png").display()
                )
                .ok();
            }
            scaffold::Source::Blank => {
                writeln!(
                    out,
                    "would leave {} for you to add",
                    dir.join("logo.png").display()
                )
                .ok();
            }
        }
        return Ok(());
    }

    let created = scaffold::create(layout, name, source)?;
    writeln!(out, "created {}", created.dir.display()).ok();
    match created.logo_from {
        Some(unlock) => {
            writeln!(
                out,
                "copied {} to logo.png, and took its colours",
                unlock.display()
            )
            .ok();
            writeln!(
                out,
                "run `omaboot apply {name} --dry-run` to see what it would do"
            )
            .ok();
        }
        None => {
            writeln!(
                out,
                "add a logo.png, then run `omaboot apply {name} --dry-run`"
            )
            .ok();
            writeln!(
                out,
                "or scaffold with --from-omarchy-theme <theme> to start from one you already use"
            )
            .ok();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_changing_command_takes_dry_run_and_root() {
        for argv in [
            vec!["omaboot", "apply", "test", "--dry-run", "--root", "/tmp/p"],
            vec!["omaboot", "revert", "--dry-run", "--root", "/tmp/p"],
            vec!["omaboot", "reset", "--dry-run", "--root", "/tmp/p"],
            vec!["omaboot", "new", "x", "--dry-run", "--root", "/tmp/p"],
            vec![
                "omaboot",
                "render",
                "x",
                "--out",
                "x.png",
                "--dry-run",
                "--root",
                "/tmp/p",
            ],
        ] {
            let cli = Cli::try_parse_from(&argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
            assert!(cli.dry_run, "{argv:?}");
            assert_eq!(cli.root.as_deref(), Some(std::path::Path::new("/tmp/p")));
        }
    }

    #[test]
    fn no_subcommand_means_the_terminal_interface() {
        let cli = Cli::try_parse_from(["omaboot"]).unwrap();
        assert!(cli.command.is_none());
    }

    #[test]
    fn a_partial_apply_does_not_promise_a_new_boot_screen() {
        use crate::apply::{ApplyReport, StepReport};
        let mut partial = ApplyReport::new("t", false);
        partial.push(StepReport::new(StepId::Validate, vec![]));
        partial.push(StepReport::new(StepId::Generate, vec![]));
        partial.push(StepReport::new(StepId::Stage, vec![]));
        partial.push(StepReport::new(StepId::SmokeTest, vec![]));
        partial.push(StepReport::skipped(StepId::Switch));
        let text = closing_lines(&partial).unwrap();
        assert!(!text.contains("Reboot"), "{text}");
        assert_eq!(
            text,
            "Only validate, generate, stage, smoke-test ran; nothing was switched, what boots is unchanged."
        );

        let mut full = ApplyReport::new("t", false);
        full.push(StepReport::new(StepId::Switch, vec![]));
        assert!(
            closing_lines(&full)
                .unwrap()
                .starts_with("Reboot to see it")
        );

        let mut dry = ApplyReport::new("t", true);
        dry.push(StepReport::new(StepId::Switch, vec![]));
        assert_eq!(closing_lines(&dry), None);
    }

    #[test]
    fn an_unknown_step_lists_the_known_ones() {
        let cli = Cli::try_parse_from(["omaboot", "apply", "t", "--step", "nonsense"]).unwrap();
        let mut out = Vec::new();
        let error = run(cli, &mut out).unwrap_err().to_string();
        assert!(error.contains("smoke-test"), "{error}");
    }

    /// A layout whose system prefix holds a packaged Omarchy tree.
    fn scaffolding_world() -> (tempfile::TempDir, Layout) {
        let tmp = tempfile::tempdir().unwrap();
        let layout = Layout::with_dirs(
            Some(tmp.path().join("root")),
            tmp.path().join("config/omaboot"),
            tmp.path().join("state"),
        );
        (tmp, layout)
    }

    fn omarchy_theme(tmp: &tempfile::TempDir, name: &str) {
        let dir = tmp.path().join("root/usr/share/omarchy/themes").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("colors.toml"),
            "mode = \"dark\"\nbackground = \"#1e1e2e\"\nforeground = \"#cdd6f4\"\naccent = \"#89b4fa\"\nred = \"#f38ba8\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("unlock.png"), format!("unlock image of {name}")).unwrap();
    }

    #[test]
    fn the_scaffold_it_writes_is_a_theme_it_can_read() {
        let (tmp, layout) = scaffolding_world();
        let mut out = Vec::new();
        new_theme(&layout, "fresh", &scaffold::Source::Blank, false, &mut out).unwrap();
        std::fs::write(layout.theme_dir("fresh").join("logo.png"), b"bytes").unwrap();
        Theme::load(layout.theme_dir("fresh")).expect("the scaffold must validate");
        drop(tmp);
    }

    #[test]
    fn scaffolding_over_an_existing_theme_is_refused() {
        let (_tmp, layout) = scaffolding_world();
        let mut out = Vec::new();
        new_theme(&layout, "fresh", &scaffold::Source::Blank, false, &mut out).unwrap();
        let error = new_theme(&layout, "fresh", &scaffold::Source::Blank, false, &mut out)
            .unwrap_err()
            .to_string();
        assert!(error.contains("already exists"), "{error}");
    }

    #[test]
    fn scaffolding_from_an_omarchy_theme_needs_no_further_steps() {
        let (tmp, layout) = scaffolding_world();
        omarchy_theme(&tmp, "catppuccin");
        let mut out = Vec::new();
        new_theme(
            &layout,
            "mine",
            &scaffold::Source::OmarchyTheme("catppuccin".into()),
            false,
            &mut out,
        )
        .unwrap();

        // It validates as it stands: no logo to add by hand.
        let theme = Theme::load(layout.theme_dir("mine")).expect("must validate unaided");
        assert_eq!(theme.manifest().colors.background, "#1e1e2e");
        assert_eq!(theme.manifest().colors.foreground, "#cdd6f4");
        assert_eq!(theme.manifest().colors.accent, "#89b4fa");
        assert_eq!(theme.manifest().colors.error, "#f38ba8");
        assert_eq!(
            std::fs::read(theme.logo()).unwrap(),
            b"unlock image of catppuccin"
        );
    }

    #[test]
    fn scaffolding_from_an_unknown_omarchy_theme_leaves_nothing_behind() {
        let (tmp, layout) = scaffolding_world();
        omarchy_theme(&tmp, "gruvbox");
        let mut out = Vec::new();
        let error = new_theme(
            &layout,
            "mine",
            &scaffold::Source::OmarchyTheme("tokyo-night".into()),
            false,
            &mut out,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("gruvbox"), "{error}");
        assert!(!layout.theme_dir("mine").exists(), "nothing may be created");
    }

    #[test]
    fn a_dry_run_scaffold_says_where_the_logo_would_come_from() {
        let (tmp, layout) = scaffolding_world();
        omarchy_theme(&tmp, "nord");
        let mut out = Vec::new();
        new_theme(
            &layout,
            "mine",
            &scaffold::Source::OmarchyTheme("nord".into()),
            true,
            &mut out,
        )
        .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("would copy"), "{text}");
        assert!(text.contains("nord/unlock.png"), "{text}");
        assert!(!layout.theme_dir("mine").exists());
    }

    #[test]
    fn a_screen_name_or_a_size_that_makes_no_sense_says_what_is_accepted() {
        assert_eq!(parse_screen("Login").unwrap(), render::Screen::Login);
        assert!(
            parse_screen("desktop")
                .unwrap_err()
                .to_string()
                .contains("unlock")
        );
        assert_eq!(parse_size("1280x720").unwrap(), (1280, 720));
        assert!(
            parse_size("1280")
                .unwrap_err()
                .to_string()
                .contains("WIDTHxHEIGHT")
        );
        assert!(
            parse_size("2x2")
                .unwrap_err()
                .to_string()
                .contains("between")
        );
        assert!(
            parse_size("wide x tall")
                .unwrap_err()
                .to_string()
                .contains("WIDTHxHEIGHT")
        );
    }

    #[test]
    fn status_reports_an_empty_system_without_failing() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = Layout::with_dirs(
            Some(tmp.path().to_path_buf()),
            tmp.path().join("config"),
            tmp.path().join("state"),
        );
        let mut out = Vec::new();
        status(&layout, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("nothing is applied"), "{text}");
        assert!(text.contains("(none recorded)"), "{text}");
        assert!(text.contains("what boots now"), "{text}");
        assert!(text.contains("none set"), "{text}");
    }

    #[test]
    fn a_dry_run_scaffold_from_the_current_screens_says_what_it_copies() {
        let (tmp, layout) = scaffolding_world();
        std::fs::create_dir_all(layout.plymouthd_conf().parent().unwrap()).unwrap();
        std::fs::write(layout.plymouthd_conf(), "[Daemon]\nTheme=omarchy\n").unwrap();
        let installed = layout.plymouth_theme_root().join("omarchy");
        std::fs::create_dir_all(&installed).unwrap();
        std::fs::write(installed.join("logo.png"), b"installed logo").unwrap();
        let mut out = Vec::new();
        new_theme(&layout, "now", &scaffold::Source::Current, true, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("would take its values from the installed omarchy theme"),
            "{text}"
        );
        assert!(text.contains("omarchy/logo.png"), "{text}");
        assert!(!layout.theme_dir("now").exists());
        drop(tmp);
    }

    #[test]
    fn list_says_where_themes_go_when_there_are_none() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = Layout::with_dirs(None, tmp.path().join("config"), tmp.path().join("state"));
        let mut out = Vec::new();
        list(&layout, &mut out).unwrap();
        assert!(String::from_utf8(out).unwrap().contains("omaboot new"));
    }
}
