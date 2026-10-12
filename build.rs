use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=src/private_storage_macos.c");
    println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build_macos_storage();
    }
    println!("cargo:rustc-check-cfg=cfg(windows_release)");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=packaging/windows/application.rc");
    println!("cargo:rerun-if-changed=assets/direct-payment-timesheets.ico");
    println!("cargo:rerun-if-env-changed=RC");
    println!("cargo:rerun-if-env-changed=DPT_INSTALLATION_KIND");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows")
        || env::var("PROFILE").as_deref() != Ok("release")
    {
        return;
    }
    assert_eq!(
        env::var("CARGO_CFG_TARGET_ENV").as_deref(),
        Ok("msvc"),
        "Windows release resources require the MSVC target and Windows SDK rc.exe"
    );

    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let parts: Vec<u16> = version
        .split('.')
        .map(|part| {
            part.parse()
                .expect("Windows resources require a numeric major.minor.patch version")
        })
        .collect();
    assert_eq!(
        parts.len(),
        3,
        "Windows resources require major.minor.patch"
    );
    let numeric_version = format!("{},{},{},0", parts[0], parts[1], parts[2]);
    let icon = root.join("assets/direct-payment-timesheets.ico");
    let icon = icon
        .to_str()
        .expect("Icon path must be UTF-8")
        .replace('\\', "\\\\");
    let resource = include_str!("packaging/windows/application.rc")
        .replace("@VERSION@", &version)
        .replace("@NUMERIC_VERSION@", &numeric_version)
        .replace("@ICON@", &icon);
    let source = out.join("application.rc");
    let compiled = out.join("application.res");
    fs::write(&source, resource).expect("Could not write Windows resource source to OUT_DIR");
    let compiler = env::var_os("RC").unwrap_or_else(|| "rc.exe".into());
    let status = Command::new(compiler)
        .arg("/nologo")
        .arg("/fo")
        .arg(&compiled)
        .arg(&source)
        .status()
        .expect("Could not run Windows SDK rc.exe; use a Visual Studio developer shell or set RC");
    assert!(status.success(), "Windows resource compilation failed");
    println!(
        "cargo:rustc-link-arg-bin=direct_payment_timesheets={}",
        compiled.display()
    );
    println!("cargo:rustc-cfg=windows_release");
}

fn build_macos_storage() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let object = out.join("dpt-private-storage.o");
    let archive = out.join("libdpt_private_storage.a");
    let architecture = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        _ => panic!("Unsupported macOS storage target"),
    };
    let mut compiler = Command::new("/usr/bin/clang");
    compiler
        .args(["-arch", architecture, "-Wall", "-Wextra", "-Werror", "-c"])
        .arg(root.join("src/private_storage_macos.c"))
        .arg("-o")
        .arg(&object);
    if let Ok(minimum) = env::var("MACOSX_DEPLOYMENT_TARGET") {
        compiler.arg(format!("-mmacosx-version-min={minimum}"));
    }
    assert!(
        compiler
            .status()
            .expect("macOS storage requires the installed Apple SDK/clang")
            .success(),
        "macOS creation-time ACL helper compilation failed"
    );
    assert!(
        Command::new("/usr/bin/ar")
            .arg("crs")
            .arg(&archive)
            .arg(&object)
            .status()
            .expect("macOS archiver unavailable")
            .success(),
        "macOS storage archive failed"
    );
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=dpt_private_storage");
}
