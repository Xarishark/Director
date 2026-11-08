#[path = "packaging/windows_icon.rs"]
mod windows_icon;
fn main() {
    println!("cargo:rerun-if-changed=assets/director.svg");
    windows_icon::embed(include_bytes!("assets/director.svg"), "director");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("build output"));
    let profile = out.ancestors().nth(3).expect("Cargo profile directory");
    println!("cargo:rerun-if-changed=assets/orientations");
    let icons = profile.join("orientations");
    std::fs::create_dir_all(&icons).expect("create orientation asset folder");
    for degree in [0, 90, 180, 270] {
        let file = format!("{degree}.svg");
        std::fs::copy(
            std::path::Path::new("assets/orientations").join(&file),
            icons.join(file),
        )
        .expect("stage orientation SVG");
    }
    println!("cargo:rerun-if-changed=assets/icons");
    let actions = profile.join("icons");
    std::fs::create_dir_all(&actions).expect("create action icon folder");
    for file in ["remote.svg", "delete.svg"] {
        std::fs::copy(
            std::path::Path::new("assets/icons").join(file),
            actions.join(file),
        )
        .expect("stage action SVG");
    }
    std::fs::copy(out.join("director.ico"), profile.join("director.ico"))
        .expect("stage window icon");
}
