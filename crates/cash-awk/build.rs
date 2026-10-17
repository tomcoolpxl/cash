//! Build script for cash-awk.

fn main() {
    // The `awk` program the tests run gets the 8 MB stack cash.exe has (`crates/cash/build.rs`),
    // where the bundled awk runs; MSVC's default of 1 MB ran out before 40 levels of nested
    // parentheses, which gawk and cash take.
    println!("cargo:rustc-link-arg-bins=/STACK:8388608");
}
