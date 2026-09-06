use std::collections::{BTreeMap, BTreeSet};
use sunshine_agent_protocol::{config::*, *};
use uuid::Uuid;

fn task(command: Command, permission: Permission) -> Task {
    Task {
        protocol: PROTOCOL.into(),
        operation_id: "op_00000000-0000-4000-8000-000000000001".into(),
        binding: Binding {
            manager_id: Uuid::from_u128(1),
            device_id: Uuid::from_u128(2),
            installation_id: Uuid::from_u128(3),
        },
        permission,
        command,
    }
}

#[test]
fn closed_protocol_rejects_arbitrary_execution_and_unknown_fields() {
    for bytes in [
        br#"{"type":"http","url":"https://localhost/"}"#.as_slice(),
        br#"{"type":"revoked","script":"evil"}"#,
    ] {
        assert!(decode_manager_message(bytes).is_err());
    }
    assert!(decode_manager_message(&vec![b' '; MAX_MESSAGE_BYTES + 1]).is_err());
}

#[test]
fn credentials_commands_network_and_paths_are_never_managed_fields() {
    for key in [
        "global_prep_cmd",
        "prep-cmd",
        "cmd",
        "file_apps",
        "pkey",
        "credentials_file",
        "upnp",
        "port",
        "csrf_allowed_origins",
        "origin_web_ui_allowed",
        "install_steam_audio_drivers",
    ] {
        assert!(
            validate_field(key, &FieldValue::Text("anything".into())).is_err(),
            "{key}"
        );
        let task = task(
            Command::PatchConfig {
                expected_revision: "a".repeat(64),
                set: BTreeMap::new(),
                remove: BTreeSet::from([key.into()]),
                restart_policy: RestartPolicy::Manual,
            },
            Permission::WriteConfig,
        );
        assert!(task.validate(&task.binding, true).is_err(), "{key}");
    }
}

#[test]
fn field_types_ranges_and_config_line_injection_are_rejected() {
    for value in [
        FieldValue::Integer(-1),
        FieldValue::Integer(52),
        FieldValue::Text("28".into()),
        FieldValue::Boolean(true),
    ] {
        assert!(validate_field("qp", &value).is_err());
    }
    for value in [
        "host\nupnp = enabled".into(),
        "a".repeat(33),
        "x#y".into(),
        "x\rname".into(),
        "".into(),
    ] {
        assert!(validate_field("sunshine_name", &FieldValue::Text(value)).is_err());
    }
    assert!(validate_field("sunshine_name", &FieldValue::Text("猫".repeat(32))).is_ok());
    assert!(validate_field("sunshine_name", &FieldValue::Text("🎮".repeat(32))).is_ok());
    assert!(validate_field("min_log_level", &FieldValue::Text("debug".into())).is_err());
    assert!(validate_field("qp", &FieldValue::Integer(28)).is_ok());
}

#[test]
fn binding_protocol_and_permissions_are_enforced() {
    let mut task = task(Command::ReadConfig {}, Permission::ReadConfig);
    assert!(task.validate(&task.binding, false).is_ok());
    let mut wrong = task.binding.clone();
    wrong.device_id = Uuid::from_u128(4);
    assert_eq!(
        task.validate(&wrong, false),
        Err(Rejection::BindingMismatch)
    );
    task.permission = Permission::WriteConfig;
    assert_eq!(
        task.validate(&task.binding, false),
        Err(Rejection::PermissionDenied)
    );
    task.protocol = "sunshine-management/2".into();
    assert_eq!(
        task.validate(&task.binding, false),
        Err(Rejection::InvalidTask)
    );
}

#[test]
fn restart_requires_both_local_policy_and_administrator_confirmation() {
    let mut task = task(
        Command::Restart {
            expected_revision: "f".repeat(64),
            administrator_confirmed: true,
        },
        Permission::Restart,
    );
    assert!(task.validate(&task.binding, true).is_ok());
    assert_eq!(
        task.validate(&task.binding, false),
        Err(Rejection::PermissionDenied)
    );
    task.command = Command::Restart {
        expected_revision: "f".repeat(64),
        administrator_confirmed: false,
    };
    assert_eq!(
        task.validate(&task.binding, true),
        Err(Rejection::PermissionDenied)
    );
}

#[test]
fn fingerprint_covers_ownership_permissions_and_content() {
    let mut task = task(Command::ReadConfig {}, Permission::ReadConfig);
    let original = task.fingerprint().unwrap();
    task.binding.installation_id = Uuid::from_u128(4);
    assert_ne!(original, task.fingerprint().unwrap());
    let before = task.fingerprint().unwrap();
    task.permission = Permission::Restart;
    assert_ne!(before, task.fingerprint().unwrap());
}
