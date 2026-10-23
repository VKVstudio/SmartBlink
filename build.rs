use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/version.rc");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-env-changed=SMARTBLINK_WINDRES");

    let windres = env::var("SMARTBLINK_WINDRES")
        .unwrap_or_else(|_| "x86_64-w64-mingw32-windres".to_string());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let res_obj = out_dir.join("resources.o");

    let output = Command::new(&windres)
        .current_dir(&manifest_dir)
        .arg("--input")
        .arg("assets/version.rc")
        .arg("--output-format=coff")
        .arg("--output")
        .arg(&res_obj)
        .arg("--include-dir")
        .arg("assets")
        .output()
        .unwrap_or_else(|e| panic!("failed to invoke windres ({windres}): {e}"));

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        panic!(
            "windres ({windres}) failed with status {}:\nstdout: {stdout}\nstderr: {stderr}",
            output.status
        );
    }

    println!("cargo:rustc-link-arg={}", res_obj.display());
}
