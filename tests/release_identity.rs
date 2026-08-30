use std::process::Command;

use sunshine_manager::release_contract::{ReleaseContract, embedded};

#[test]
fn identity_command_reports_the_embedded_current_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_sunshine-manager"))
        .arg("identity")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let reported: ReleaseContract = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reported, embedded().unwrap());
}
