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
//! privileged command is a child of this one omaboot process. Whether the
//! ticket holds is checked right after it is taken, with `sudo -n -v`, so a
//! sudoers policy that keeps no ticket is reported at the password dialog.

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

/// The arguments that turn a password on stdin into a ticket. No `-k` here:
/// with `-v`, `-k` makes sudo check the password and then not record it
/// ("will not update the user's cached credentials", sudo(8)), which is
/// exactly a ticket that the `sudo -n` calls after it cannot find.
pub const TICKET_ARGS: [&str; 4] = ["-S", "-v", "-p", ""];

fn acquire(password: &str) -> Result<()> {
    let mut child = Command::new("sudo")
        .args(TICKET_ARGS)
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
    // The ticket has to hold for the `sudo -n` calls that follow, which is
    // sudo's to decide (timestamp_timeout, timestamp_type). Finding out here,
    // at the password dialog, beats finding out at step 5 of an apply.
    let held = Command::new("sudo")
        .args(["-n", "-v"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if !held {
        return Err(Error::Environment {
            what: "sudo accepted the password but kept no ticket for this process, so the privileged steps could not run without a terminal".to_string(),
            suggestion: "check `Defaults timestamp_timeout` and `timestamp_type` in /etc/sudoers (`sudo -l` shows them); a timeout of 0 keeps nothing, and omaboot needs the ticket for the seconds an apply takes".to_string(),
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
    fn the_ticket_is_asked_for_without_k_so_that_sudo_records_it() {
        // `sudo -S -k -v` accepts the password and records nothing; the
        // first real apply failed at step 5 on exactly that (A2).
        assert!(!TICKET_ARGS.contains(&"-k"));
        assert!(TICKET_ARGS.contains(&"-v"));
        assert!(TICKET_ARGS.contains(&"-S"));
    }

    #[test]
    fn sudo_is_plain_until_a_ticket_is_acquired() {
        reset_for_tests();
        assert_eq!(sudo_args(), vec!["sudo".to_string()]);
        set_non_interactive();
        assert_eq!(sudo_args(), vec!["sudo".to_string(), "-n".to_string()]);
        reset_for_tests();
    }
}
