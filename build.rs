//! Embeds the logo in the Windows executable as icon resource 1, which is
//! what Explorer, the taskbar and gpui's window class all pick up. The .ico
//! is generated from `assets/logo.svg`; regenerate it when the logo changes.

fn main() {
    println!("cargo:rerun-if-changed=assets/logo.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/logo.ico");
        // Task Manager shows FileDescription as the process name.
        res.set("FileDescription", "Darkroom");
        res.set("ProductName", "Darkroom");
        res.compile().expect("embedding the Windows icon");
    }
}
