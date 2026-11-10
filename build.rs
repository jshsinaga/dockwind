fn main() {
    println!("cargo:rerun-if-changed=assets/dockwind.manifest");
    println!("cargo:rerun-if-changed=assets/dockwind.rc");

    #[cfg(target_os = "windows")]
    embed_resource::compile("assets/dockwind.rc", embed_resource::NONE)
        .manifest_required()
        .expect("failed to embed the Dockwind Windows manifest");
}
