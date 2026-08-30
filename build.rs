use std::env;

fn main() {
    let target = env::var("TARGET").expect("Cargo must provide TARGET");
    println!("cargo:rustc-env=SUNSHINE_MANAGER_BUILD_TARGET={target}");
    println!("cargo:rerun-if-changed=release.json");
}
