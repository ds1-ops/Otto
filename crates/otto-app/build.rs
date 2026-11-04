//! Embeds Otto's icon (assets/otto.ico) into otto.exe on Windows.

fn main() {
    println!("cargo:rerun-if-changed=assets/otto.rc");
    println!("cargo:rerun-if-changed=assets/otto.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/otto.rc", embed_resource::NONE).manifest_optional().expect("compile the icon resource");
    }
}
