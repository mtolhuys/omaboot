//! The unprivileged entry point.

use std::io::Write as _;
use std::process::ExitCode;

use clap::Parser as _;

fn main() -> ExitCode {
    let cli = omaboot::cli::Cli::parse();
    let json = cli.json;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    match omaboot::cli::run(cli, &mut out) {
        Ok(()) => {
            let _ = out.flush();
            ExitCode::SUCCESS
        }
        Err(error) => {
            let _ = out.flush();
            let mut message = format!("omaboot: {error}");
            let mut source = std::error::Error::source(&error);
            while let Some(cause) = source {
                message.push_str(&format!("\n  caused by: {cause}"));
                source = cause.source();
            }
            if json {
                // A front end reads stdout; the error goes there too, as the
                // same kind of line it expects, and to stderr for the log.
                let line = serde_json::json!({"event": "error", "message": error.to_string()});
                let _ = writeln!(out, "{line}");
                let _ = out.flush();
            }
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
