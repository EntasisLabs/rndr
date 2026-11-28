fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=icons/app.rc");
        println!("cargo:rerun-if-changed=icons/app.ico");
        embed_resource::compile_for(
            "icons/app.rc",
            ["desktop"],
            embed_resource::ParamsIncludeDirs(["icons"]),
        )
        .manifest_required()
        .expect("embed the Desktop Workshop executable icon");
    }
}
