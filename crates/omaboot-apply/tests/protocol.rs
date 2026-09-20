//! The built helper answers `protocol` with the shared number, unprivileged,
//! so the engine can refuse a helper from an older build before anything
//! privileged runs.

use std::process::Command;

#[path = "../src/protocol.rs"]
mod protocol;

#[test]
fn the_helper_answers_protocol_with_the_shared_number_without_privilege() {
    let output = Command::new(env!("CARGO_BIN_EXE_omaboot-apply"))
        .arg("protocol")
        .output()
        .expect("the helper binary runs");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        protocol::PROTOCOL.to_string()
    );
    assert!(output.stderr.is_empty(), "{output:?}");
}
