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

    fs::write(&rc, format!("1 ICON \"{}\"\n", icon.display().to_string().replace('\\', "/")))
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
