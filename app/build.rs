use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=../assets/windows/app.manifest");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join("../assets/windows/app.manifest");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
}
