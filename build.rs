use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "windows" {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let resource = out_dir.join("warmer.res");

    let status = Command::new("rc.exe")
        .arg("/nologo")
        .arg(format!("/fo{}", resource.display()))
        .arg("assets\\warmer.rc")
        .status()
        .expect("failed to run rc.exe; install Microsoft C++ Build Tools with the Windows SDK");

    if !status.success() {
        panic!("rc.exe failed with status {status}");
    }

    println!("cargo:rustc-link-arg={}", resource.display());
    println!("cargo:rerun-if-changed=assets/warmer.rc");
    println!("cargo:rerun-if-changed=assets/Warmer.ico");
}
