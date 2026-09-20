//! omaboot-apply: the privileged half, and nothing else.
//!
//! It receives a directory that `omaboot` has already staged and already
//! validated, and it publishes a fixed list of file names into two fixed
//! destination directories. It parses no theme, resolves no user-controlled
//! path after gaining privilege, and never calls back into `omaboot`.
//!
//! The hardening mirrors `omarchy-plymouth-set`, which is the model reviewers
//! from this community will recognise: destinations are validated as
//! root-owned and not group or world writable, symlinks are refused, each file
//! is published through a sibling temporary file plus an atomic rename, and
//! the copy is compared with its source before the rename.

mod protocol;
mod publish;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use publish::{Job, Mode, Trust};

#[derive(Debug, Parser)]
#[command(
    name = "omaboot-apply",
    version,
    about = "Privileged installer for omaboot. Not meant to be run by hand."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Args, Clone, Default)]
struct Flags {
    /// Print what would be done, and do none of it.
    #[arg(long, global = true)]
    dry_run: bool,

    /// Treat this directory as the root of the system. Implies the unprivileged
    /// trust policy: ownership of the destination is not required, and nothing
    /// outside the prefix is touched. For tests only.
    #[arg(long, global = true, value_name = "PREFIX")]
    root: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Publish a staged theme into the omaboot directories
    Install {
        /// The directory omaboot staged, containing plymouth/ and sddm/
        #[arg(long, value_name = "DIR")]
        staged: PathBuf,
        #[command(flatten)]
        flags: Flags,
    },
    /// Write or remove /etc/sddm.conf.d/zz-omaboot.conf
    Switch {
        /// Point SDDM at the omaboot theme
        #[arg(long, group = "direction")]
        on: bool,
        /// Point SDDM at Omarchy's own theme, outranking any other drop-in
        #[arg(long, group = "direction")]
        stock: bool,
        /// Remove the drop-in, so SDDM uses whatever it used before
        #[arg(long, group = "direction")]
        off: bool,
        #[command(flatten)]
        flags: Flags,
    },
    /// Print the protocol number this build speaks, so omaboot can refuse a
    /// helper built from older source before anything privileged runs
    Protocol,
    /// Remove the omaboot theme directories and the drop-in
    Remove {
        #[command(flatten)]
        flags: Flags,
    },
    /// Put a staged preview theme under /run/plymouth/themes, or take it away
    Preview {
        #[command(subcommand)]
        action: PreviewAction,
    },
}

#[derive(Debug, Subcommand)]
enum PreviewAction {
    /// Publish a staged preview theme into /run/plymouth/themes/omaboot-preview
    Install {
        /// The directory omaboot staged the preview theme in
        #[arg(long, value_name = "DIR")]
        staged: PathBuf,
        #[command(flatten)]
        flags: Flags,
    },
    /// Remove /run/plymouth/themes/omaboot-preview
    Remove {
        #[command(flatten)]
        flags: Flags,
    },
}

impl Command {
    fn flags(&self) -> Option<&Flags> {
        match self {
            Self::Install { flags, .. } | Self::Switch { flags, .. } | Self::Remove { flags } => {
                Some(flags)
            }
            Self::Preview { action } => match action {
                PreviewAction::Install { flags, .. } | PreviewAction::Remove { flags } => {
                    Some(flags)
                }
            },
            Self::Protocol => None,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let Some(flags) = cli.command.flags().cloned() else {
        // `protocol`: no privilege, no prefix, one number on stdout.
        println!("{}", protocol::PROTOCOL);
        return ExitCode::SUCCESS;
    };

    let trust = match &flags.root {
        Some(_) => Trust::Prefixed,
        None => Trust::Strict,
    };
    let mode = Mode {
        root: flags.root.clone(),
        dry_run: flags.dry_run,
        trust,
    };

    let job = match cli.command {
        Command::Install { staged, .. } => Job::Install { staged },
        Command::Switch { on, stock, off, .. } => {
            let target = match (on, stock, off) {
                (true, false, false) => publish::SwitchTarget::Omaboot,
                (false, true, false) => publish::SwitchTarget::Stock,
                (false, false, true) => publish::SwitchTarget::Off,
                _ => {
                    eprintln!("omaboot-apply: pass exactly one of --on, --stock or --off");
                    return ExitCode::FAILURE;
                }
            };
            Job::Switch { target }
        }
        Command::Remove { .. } => Job::Remove,
        Command::Protocol => {
            println!("{}", protocol::PROTOCOL);
            return ExitCode::SUCCESS;
        }
        Command::Preview { action } => match action {
            PreviewAction::Install { staged, .. } => Job::PreviewInstall { staged },
            PreviewAction::Remove { .. } => Job::PreviewRemove,
        },
    };

    match publish::run(&job, &mode) {
        Ok(lines) => {
            for line in lines {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("omaboot-apply: {error:#}");
            ExitCode::FAILURE
        }
    }
}
