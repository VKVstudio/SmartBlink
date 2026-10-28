use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/version.rc");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-env-changed=SMARTBLINK_WINDRES");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    // 1. On MSVC targets, try rc.exe / llvm-rc.exe first:
    if target_env == "msvc" {
        let res_file = out_dir.join("resources.res");
        for rc_bin in ["rc.exe", "llvm-rc.exe"] {
            if let Ok(output) = Command::new(rc_bin)
                .current_dir(&manifest_dir)
                .arg("/nologo")
                .arg("/I")
                .arg("assets")
                .arg("/fo")
                .arg(&res_file)
                .arg("assets/version.rc")
                .output()
            {
                if output.status.success() {
                    println!("cargo:rustc-link-arg={}", res_file.display());
                    return;
                }
            }
        }
    }

    // 2. On gnullvm / gnu targets (or fallback), try SMARTBLINK_WINDRES, x86_64-w64-mingw32-windres, windres:
    let mut candidates = Vec::new();
    if let Ok(custom) = env::var("SMARTBLINK_WINDRES") {
        if !custom.trim().is_empty() {
            candidates.push(custom);
        }
    }
    candidates.push("x86_64-w64-mingw32-windres".to_string());
    candidates.push("windres".to_string());

    let res_obj = out_dir.join("resources.o");
    for windres in &candidates {
        if let Ok(output) = Command::new(windres)
            .current_dir(&manifest_dir)
            .arg("--input")
            .arg("assets/version.rc")
            .arg("--output-format=coff")
            .arg("--output")
            .arg(&res_obj)
            .arg("--include-dir")
            .arg("assets")
            .output()
        {
            if output.status.success() {
                println!("cargo:rustc-link-arg={}", res_obj.display());
                return;
            }
        }
    }

    println!(
        "cargo:warning=No Win32 resource compiler (windres/rc.exe) found in PATH; building smartblink.exe with runtime fallback icon."
    );
}
