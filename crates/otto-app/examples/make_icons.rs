//! Regenerates the icon files from assets/otto.svg:
//!   cargo run -p otto-app --example make_icons
//! - crates/otto-app/assets/otto.ico (embedded into otto.exe by build.rs)
//! - packaging/claude-extension/icon.png (the Claude desktop extension)

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let ico = otto_app::logo::ico().expect("render the icon");
    std::fs::write(root.join("assets").join("otto.ico"), ico).unwrap();
    let png = otto_app::logo::png(512).expect("render the icon");
    std::fs::write(root.join("../../packaging/claude-extension/icon.png"), png).unwrap();
    println!("wrote assets/otto.ico and packaging/claude-extension/icon.png");
}
