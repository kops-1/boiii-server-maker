use embed_manifest::{embed_manifest, new_manifest};

const ICON: &str = "../../assets/branding/icon.ico";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={ICON}");

    if std::env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }

    // "asInvoker" matters here: without a manifest Windows may guess that a file
    // named "...Setup.exe" needs administrator rights. This installer is per-user.
    embed_manifest(new_manifest("BoiiiServerMakerSetup")).expect("unable to embed manifest");

    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon_with_id(ICON, "1")
        .set("ProductName", "Boiii Server Maker")
        .set("FileDescription", "Boiii Server Maker Setup")
        .set("LegalCopyright", "MIT License");
    resource.compile().expect("unable to embed the icon and version info");
}
