//! Acquiring privilege without a terminal.
//!
//! Every privileged command omaboot runs goes through `sudo`. From a shell
//! `sudo` asks for the password itself. From the Quattro plugin there is no
//! terminal, so the plugin asks once, in its own dialog, and hands the answer
//! to the engine on stdin. The engine turns it into a `sudo` ticket with
//! `sudo -S -v` and from then on runs every `sudo` with `-n`, so a missing
//! ticket is an error rather than a process waiting forever for a prompt
//! nobody can see.
//!
//! The ticket is shared between the calls that follow because, without a
//! terminal, sudo keys its timestamp on the parent process, and every
//! privileged command is a child of this one omaboot process.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{Error, Result};

static NON_INTERACTIVE: AtomicBool = AtomicBool::new(false);

/// The `sudo` invocation to prefix a privileged command with: plain `sudo`
/// when a terminal can answer, `sudo -n` once a ticket was acquired here.
pub fn sudo_args() -> Vec<String> {
    if NON_INTERACTIVE.load(Ordering::Relaxed) {
        vec!["sudo".to_string(), "-n".to_string()]
    } else {
        vec!["sudo".to_string()]
    }
}

/// Read one line from stdin and turn it into a sudo ticket. The password is
/// held only long enough to hand it to `sudo`, and is overwritten afterwards.
pub fn acquire_from_stdin() -> Result<()> {
    let mut password = String::new();
    std::io::stdin()
        .read_line(&mut password)
        .map_err(|source| Error::Environment {
            what: format!("the password could not be read from stdin: {source}"),
            suggestion: "write the password followed by a newline to omaboot's stdin".to_string(),
        })?;
    while password.ends_with('\n') || password.ends_with('\r') {
        password.pop();
    }
    let result = acquire(&password);
    // Best effort: the String is dropped right after, but overwriting means
    // its bytes do not linger in a freed page.
    password.clear();
    password.push_str(&" ".repeat(64));
    result
}

fn acquire(password: &str) -> Result<()> {
    let mut child = Command::new("sudo")
        .args(["-S", "-k", "-v", "-p", ""])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| Error::Environment {
            what: format!("sudo could not be started: {source}"),
            suggestion: "install sudo, or run omaboot from a shell".to_string(),
        })?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(password.as_bytes());
        let _ = stdin.write_all(b"\n");
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    let status = child.wait().map_err(|source| Error::Environment {
        what: format!("sudo did not finish: {source}"),
        suggestion: "try again".to_string(),
    })?;
    if !status.success() {
        return Err(Error::Environment {
            what: "the password was not accepted".to_string(),
            suggestion: "try again; sudo said: ".to_string()
                + stderr.lines().last().unwrap_or("nothing"),
        });
    }
    NON_INTERACTIVE.store(true, Ordering::Relaxed);
    Ok(())
}

/// Declare that no terminal can answer a prompt, without acquiring a ticket:
/// every sudo call runs with `-n` and fails fast if there is none.
pub fn set_non_interactive() {
    NON_INTERACTIVE.store(true, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn reset_for_tests() {
    NON_INTERACTIVE.store(false, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sudo_is_plain_until_a_ticket_is_acquired() {
        reset_for_tests();
        assert_eq!(sudo_args(), vec!["sudo".to_string()]);
        set_non_interactive();
        assert_eq!(sudo_args(), vec!["sudo".to_string(), "-n".to_string()]);
        reset_for_tests();
    }
}
