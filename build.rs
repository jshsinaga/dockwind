use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=assets/dockwind.manifest");
    println!("cargo:rerun-if-changed=assets/dockwind.ico");
    println!("cargo:rerun-if-changed=assets/dockwind.rc");

    #[cfg(target_os = "windows")]
    embed_windows_resources();
}

#[cfg(target_os = "windows")]
fn embed_windows_resources() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo"));
    let package_version =
        std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION is set by cargo");
    let numeric_version = package_version
        .split('-')
        .next()
        .expect("a package version has a numeric prefix");

    let mut parts = numeric_version.split('.');
    let mut file_version = [0u32; 4];
    for part in &mut file_version {
        *part = parts
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
    }
    let file_version = file_version
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(".");

    let manifest = fs::read_to_string("assets/dockwind.manifest")
        .expect("failed to read assets/dockwind.manifest");
    let manifest = manifest.replace(
        "version=\"0.1.0.0\"",
        &format!("version=\"{file_version}\""),
    );
    assert!(
        manifest.contains(&format!("version=\"{file_version}\"")),
        "the manifest template must contain a version attribute"
    );

    let rc = fs::read_to_string("assets/dockwind.rc").expect("failed to read assets/dockwind.rc");

    fs::write(out_dir.join("dockwind.manifest"), manifest)
        .expect("failed to write the generated manifest");
    fs::write(out_dir.join("dockwind.rc"), rc)
        .expect("failed to write the generated resource file");
    fs::copy("assets/dockwind.ico", out_dir.join("dockwind.ico"))
        .expect("failed to copy the Dockwind icon");

    embed_resource::compile(out_dir.join("dockwind.rc"), embed_resource::NONE)
        .manifest_required()
        .expect("failed to embed the Dockwind Windows resources");
}
