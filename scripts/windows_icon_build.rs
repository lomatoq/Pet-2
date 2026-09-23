use std::{env, path::PathBuf, process::Command};
pub fn embed(root: PathBuf) {
    println!(
        "cargo:rerun-if-changed={}",
        root.join("assets/windows/app.ico").display()
    );
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let resource = output.join("pet2-icon.rc");
    std::fs::copy(root.join("assets/windows/app.ico"), output.join("app.ico")).unwrap();
    std::fs::write(&resource, "1 ICON \"app.ico\"\n").unwrap();
    let mut compiler = PathBuf::from("rc.exe");
    if let Some(sdk) = env::var_os("WindowsSdkDir")
        && let Some(version) = env::var_os("WindowsSDKVersion")
    {
        compiler = PathBuf::from(sdk)
            .join("bin")
            .join(version)
            .join("x64/rc.exe");
    }
    if !compiler.is_file() {
        let sdk = PathBuf::from(
            env::var_os("ProgramFiles(x86)").unwrap_or_else(|| "C:/Program Files (x86)".into()),
        )
        .join("Windows Kits/10/bin");
        let mut versions: Vec<_> = std::fs::read_dir(sdk)
            .expect("Windows SDK required for application icon")
            .filter_map(Result::ok)
            .map(|e| e.path().join("x64/rc.exe"))
            .filter(|p| p.is_file())
            .collect();
        versions.sort();
        compiler = versions.pop().expect("Windows SDK rc.exe");
    }
    let compiled = output.join("pet2-icon.res");
    let status = Command::new(compiler)
        .arg("/nologo")
        .arg("/fo")
        .arg("pet2-icon.res")
        .arg("pet2-icon.rc")
        .current_dir(&output)
        .status()
        .expect("compile Windows icon");
    assert!(status.success(), "Windows icon resource compilation failed");
    println!("cargo:rustc-link-arg={}", compiled.display());
}
