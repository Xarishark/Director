use std::{env, fs, path::PathBuf, process::Command};
pub fn embed(svg: &[u8], binary: &str) {
    println!("cargo:rerun-if-env-changed=WINDRES");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("build output directory"));
    let tree =
        resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default()).expect("parse app SVG");
    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let images: Vec<Vec<u8>> = sizes
        .iter()
        .map(|&size| {
            let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).expect("allocate icon");
            resvg::render(
                &tree,
                resvg::tiny_skia::Transform::from_scale(size as f32 / 256., size as f32 / 256.),
                &mut pixmap.as_mut(),
            );
            pixmap.encode_png().expect("encode icon")
        })
        .collect();
    let mut ico = vec![0, 0, 1, 0, sizes.len() as u8, 0];
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&size, png) in sizes.iter().zip(&images) {
        ico.extend_from_slice(&[size as u8, size as u8, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for png in images {
        ico.extend_from_slice(&png);
    }
    let icon = out.join("director.ico");
    fs::write(&icon, ico).expect("write icon resource");
    let rc = out.join("director.rc");
    let version = env::var("CARGO_PKG_VERSION").expect("package version");
    let fixed_version = [
        "CARGO_PKG_VERSION_MAJOR",
        "CARGO_PKG_VERSION_MINOR",
        "CARGO_PKG_VERSION_PATCH",
    ]
    .map(|key| {
        env::var(key)
            .expect("version component")
            .parse::<u16>()
            .expect("Windows version number")
    });
    let [major, minor, patch] = fixed_version;
    let publisher = rc_text(&env::var("CARGO_PKG_AUTHORS").expect("package authors"));
    let description = rc_text(&env::var("CARGO_PKG_DESCRIPTION").expect("package description"));
    let icon_path = icon.display().to_string().replace('\\', "/");
    // RT_MANIFEST (24), resource 1: declares Windows 11 support and DPI awareness.
    println!("cargo:rerun-if-changed=packaging/director.manifest");
    let manifest =
        std::path::Path::new(&env::var_os("CARGO_MANIFEST_DIR").expect("crate directory"))
            .join("packaging/director.manifest")
            .display()
            .to_string()
            .replace('\\', "/");
    fs::write(
        &rc,
        format!(
            r#"1 ICON "{icon_path}"
1 24 "{manifest}"
1 VERSIONINFO
 FILEVERSION {major},{minor},{patch},0
 PRODUCTVERSION {major},{minor},{patch},0
 FILEOS 0x40004
 FILETYPE 0x1
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904B0"
        BEGIN
            VALUE "CompanyName", L"{publisher}\0"
            VALUE "FileDescription", L"{description}\0"
            VALUE "FileVersion", "{version}\0"
            VALUE "InternalName", "{binary}\0"
            VALUE "OriginalFilename", "{binary}.exe\0"
            VALUE "ProductName", "Director\0"
            VALUE "ProductVersion", "{version}\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#
        ),
    )
    .expect("write resource script");
    let msvc = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let resource = out.join(if msvc {
        "director.res"
    } else {
        "director-icon.o"
    });
    let status = if msvc {
        Command::new("rc.exe")
            .arg("/nologo")
            .arg(format!("/fo{}", resource.display()))
            .arg(&rc)
            .status()
    } else {
        Command::new(env::var_os("WINDRES").unwrap_or_else(|| "windres".into()))
            .args(["-J", "rc", "-O", "coff", "-i"])
            .arg(&rc)
            .arg("-o")
            .arg(&resource)
            .status()
    }
    .expect("run Windows resource compiler (rc.exe or windres on PATH)");
    assert!(status.success(), "compile app icon resource");
    println!("cargo:rustc-link-arg-bin={binary}={}", resource.display());
}

fn rc_text(text: &str) -> String {
    use std::fmt::Write;
    let mut escaped = String::new();
    for unit in text.encode_utf16() {
        if (0x20..=0x7e).contains(&unit) && unit != u16::from(b'"') && unit != u16::from(b'\\') {
            escaped.push(char::from_u32(u32::from(unit)).expect("ASCII character"));
        } else {
            write!(escaped, "\\x{unit:04X}").expect("write to string");
        }
    }
    escaped
}
