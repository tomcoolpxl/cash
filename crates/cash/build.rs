//! Build script for cash.

fn main() {
    // On Windows, MSVC defaults to a 1MB thread stack. Complex recursive shell scripts
    // easily exhaust 1MB. Set stack reserve to 8MB matching Linux glibc.
    #[cfg(target_os = "windows")]
    println!("cargo:rustc-link-arg=/STACK:8388608");
}
