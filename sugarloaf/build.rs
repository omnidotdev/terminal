fn main() {
    println!("cargo::rustc-check-cfg=cfg(desktop_platform)");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    // Desktop platform: not WASM, not Android
    // Used to gate font-kit (system font discovery), librashader (C++ filters), walkdir
    if target_arch != "wasm32" && target_os != "android" {
        println!("cargo:rustc-cfg=desktop_platform");
    }
}
