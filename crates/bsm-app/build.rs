use embed_manifest::{embed_manifest, new_manifest};

const ICON: &str = "../../assets/branding/icon.ico";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={ICON}");

    if std::env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }

    // Common-controls v6 (modern look), DPI awareness and "asInvoker" (no UAC prompt).
    embed_manifest(new_manifest("BoiiiServerMaker")).expect("unable to embed manifest");

    // Icon (resource id 1, which Explorer and the taskbar pick up) and the
    // version details shown in the file's Properties dialog.
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon_with_id(ICON, "1")
        .set("ProductName", "Boiii Server Maker")
        .set("FileDescription", "Boiii Server Maker")
        .set("LegalCopyright", "MIT License");
    resource.compile().expect("unable to embed the icon and version info");
}
