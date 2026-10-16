//! Build script for cash.

fn main() -> std::io::Result<()> {
    // On Windows, MSVC defaults to a 1MB thread stack. Complex recursive shell scripts
    // easily exhaust 1MB. Set stack reserve to 8MB matching Linux glibc.
    println!("cargo:rustc-link-arg=/STACK:8388608");

    // The system DLLs only some commands need are loaded when first called, not at every
    // start: user32 and gdi32 set the process up for windows it never opens, shell32 brings
    // a dozen more DLLs, and a bundled tool (`cat`) is a start of cash.exe too. Windows'
    // own cmd.exe loads only its core API sets.
    for dll in [
        "shell32", "user32", "gdi32", "ole32", "combase", "netapi32", "iphlpapi", "mpr",
        "rstrtmgr", "wtsapi32", "userenv", "version", "psapi", "ws2_32",
    ] {
        println!("cargo:rustc-link-arg-bins=/DELAYLOAD:{dll}.dll");
    }
    println!("cargo:rustc-link-arg-bins=delayimp.lib");

    // The icon and version resource that Explorer, Task Manager and Apps & features show
    // (ROADMAP item 18). The version comes from Cargo.toml; `assets/make-icon.ps1` makes
    // the icon from the logo.
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/cash.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    winresource::WindowsResource::new()
        .set_icon(&icon.to_string_lossy())
        .set("FileDescription", "Cool Again Shell")
        .set("ProductName", "cash")
        .set("OriginalFilename", "cash.exe")
        .set(
            "LegalCopyright",
            "Copyright (c) 2026 Tom Cool. MIT License.",
        )
        .compile()?;
    Ok(())
}
