use std::process::Command;

use sunshine_manager::release_contract::{BinaryIdentity, SOURCE_REVISION, embedded};

#[test]
fn identity_command_reports_the_embedded_current_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_sunshine-manager"))
        .arg("identity")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let reported: BinaryIdentity = serde_json::from_slice(&output.stdout).unwrap();
    let embedded = embedded().unwrap();
    assert_eq!(reported.manifest_format, embedded.manifest_format);
    assert_eq!(reported.application, embedded.application);
    assert_eq!(reported.version, embedded.version);
    assert_eq!(reported.api_prefix, embedded.api_prefix);
    assert_eq!(reported.schema_revision, embedded.schema_revision);
    assert_eq!(reported.schema_sha256, embedded.schema_sha256);
    assert_eq!(reported.target, embedded.target);
    assert_eq!(reported.source_revision, SOURCE_REVISION);
}

#[test]
fn release_tooling_targets_only_the_current_product_contract() {
    let package_release = include_str!("../scripts/package-release.py");
    let manifest_writer = include_str!("../scripts/write-release-manifest.py");
    let systemd_unit = include_str!("../deploy/sunshine-manager.service");

    assert!(package_release.contains("VERSION = \"0.8.0\""));
    assert!(manifest_writer.contains("VERSION = \"0.8.0\""));
    assert!(manifest_writer.contains("identity[\"schema_revision\"] != 2"));
    assert!(systemd_unit.contains("/releases/0.8.0/"));

    for source in [package_release, manifest_writer, systemd_unit] {
        assert!(!source.contains("0.7.0"));
    }
}
