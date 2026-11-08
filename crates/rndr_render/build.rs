//! Desktop-only features compile out on wasm32: `native_<feature>` is set
//! only when the feature is on and the target is not a browser.

fn main() {
    let browser = std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32");
    for feature in ["hotkey", "tray", "dialog", "notify", "appearance"] {
        println!("cargo::rustc-check-cfg=cfg(native_{feature})");
        let on = std::env::var_os(format!("CARGO_FEATURE_{}", feature.to_uppercase())).is_some();
        if on && !browser {
            println!("cargo::rustc-cfg=native_{feature}");
        }
    }
}
