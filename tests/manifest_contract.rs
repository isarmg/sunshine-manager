use serde_json::Value;
use std::path::Path;

fn route<'a>(manifest: &'a Value, id: &str) -> &'a Value {
    manifest["backend"]["routes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|route| route["id"] == id)
        .unwrap_or_else(|| panic!("missing manifest route {id}"))
}

#[test]
fn manifest_routes_match_the_worker_http_contract() {
    let manifest: Value = serde_json::from_str(include_str!("../manifest.json")).unwrap();

    let create = route(&manifest, "hosts-create");
    assert_eq!(create["path"], "/hosts");
    assert_eq!(create["upstream_path"], "/api/services/sunshine/hosts");
    assert_eq!(create["methods"], serde_json::json!(["POST"]));

    let write = route(&manifest, "hosts-write");
    assert_eq!(write["path"], "/hosts/{*path}");
    assert_eq!(
        write["upstream_path"],
        "/api/services/sunshine/hosts/{*path}"
    );
    assert_eq!(
        write["methods"],
        serde_json::json!(["POST", "PATCH", "DELETE"])
    );

    // PUT is not implemented by the worker. PATCH is used by host updates and POST by actions.
    assert!(
        !manifest["backend"]["routes"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|route| route["methods"].as_array().unwrap())
            .any(|method| method == "PUT")
    );
}

#[test]
fn manifest_bundle_is_self_consistent() {
    let manifest: Value = serde_json::from_str(include_str!("../manifest.json")).unwrap();
    let version: Value = serde_json::from_str(include_str!("../version.json")).unwrap();
    let permissions: Value = serde_json::from_str(include_str!("../permissions.json")).unwrap();
    let config: Value = serde_json::from_str(include_str!("../config/schema.json")).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    assert_eq!(manifest["manifest_version"], 1);
    assert_eq!(manifest["id"], "sunshine");
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(manifest["version"], version["version"]);
    assert_eq!(manifest["id"], version["id"]);
    assert_eq!(manifest["compatibility"], version["compatibility"]);
    assert_eq!(manifest["version_metadata"]["channel"], version["channel"]);
    assert_eq!(
        manifest["version_metadata"]["distribution"],
        version["distribution"]
    );
    assert_eq!(manifest["version_metadata"]["license"], version["license"]);
    assert_eq!(manifest["permissions"], permissions);

    assert_eq!(manifest["execution"]["mode"], "process");
    assert_eq!(manifest["execution"]["bind"]["host"], "127.0.0.1");
    assert_eq!(manifest["backend"]["base_path"], "/api/modules/sunshine");
    assert_eq!(manifest["configuration"]["schema"], "config/schema.json");
    assert_eq!(config["type"], "object");
    assert_eq!(config["additionalProperties"], false);

    for environment in manifest["execution"]["environment"].as_array().unwrap() {
        let pointer = environment["config_pointer"].as_str().unwrap();
        let property = pointer.strip_prefix('/').unwrap();
        assert!(
            config["properties"].get(property).is_some(),
            "environment mapping refers to missing configuration property {property}"
        );
    }

    let frontend = &manifest["frontend"];
    assert!(root.join(frontend["entry"].as_str().unwrap()).is_file());
    for stylesheet in frontend["styles"].as_array().unwrap() {
        assert!(root.join(stylesheet.as_str().unwrap()).is_file());
    }
    assert!(root.join("migrations/202608270001_initial.sql").is_file());
    assert!(root.join("LICENSE-APACHE").is_file());
}
