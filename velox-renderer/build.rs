fn main() {
    // Ensure linker links to the system GL library for native Skia builds.
    println!("cargo:rustc-link-lib=GL");
}
