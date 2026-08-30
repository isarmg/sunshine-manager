use std::env;

fn main() {
    let target = env::var("TARGET").expect("Cargo must provide TARGET");
    let source_revision =
        env::var("SUNSHINE_MANAGER_SOURCE_REVISION").unwrap_or_else(|_| "unbound".to_owned());
    assert!(
        source_revision == "unbound"
            || (source_revision.len() == 40
                && source_revision
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))),
        "SUNSHINE_MANAGER_SOURCE_REVISION must be a full lowercase 40-hex Git commit"
    );
    println!("cargo:rustc-env=SUNSHINE_MANAGER_BUILD_TARGET={target}");
    println!("cargo:rustc-env=SUNSHINE_MANAGER_SOURCE_REVISION={source_revision}");
    println!("cargo:rerun-if-env-changed=SUNSHINE_MANAGER_SOURCE_REVISION");
    println!("cargo:rerun-if-changed=release.json");
}
