//! Embeds `assets/spartanport.ico` into the Windows executable, so Explorer, the taskbar and a
//! shortcut to the exe show the SpartanPort icon (R92). Nothing happens on other systems.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/spartanport.ico");
    #[cfg(windows)]
    embed_icon();
}

#[cfg(windows)]
fn embed_icon() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("assets/spartanport.ico");
    // Without the Windows SDK's resource compiler the exe is still built, without its icon.
    if let Err(error) = resource.compile() {
        println!("cargo:warning=the launcher is built without its icon: {error}");
    }
}
