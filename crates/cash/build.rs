//! Build script for cash.

fn main() -> std::io::Result<()> {
    // On Windows, MSVC defaults to a 1MB thread stack. Complex recursive shell scripts
    // easily exhaust 1MB. Set stack reserve to 8MB matching Linux glibc.
    println!("cargo:rustc-link-arg=/STACK:8388608");

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
