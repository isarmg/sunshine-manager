fn main() {
    let target = std::env::var("TARGET").expect("Cargo target");
    assert!(
        matches!(
            target.as_str(),
            "x86_64-unknown-linux-gnu" | "x86_64-pc-windows-msvc"
        ),
        "Sunshine Agent supports only Linux GNU x86_64 and Windows MSVC x86_64"
    );
    println!("cargo:rerun-if-env-changed=SUNSHINE_AGENT_BUILD_SHA");
    let sha = std::env::var("SUNSHINE_AGENT_BUILD_SHA").unwrap_or_else(|_| "development".into());
    assert!(
        sha == "development"
            || (sha.len() == 40
                && sha
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())),
        "Agent build identity must be a full lowercase Git SHA"
    );
    println!("cargo:rustc-env=SUNSHINE_AGENT_BUILD_SHA={sha}");
}
