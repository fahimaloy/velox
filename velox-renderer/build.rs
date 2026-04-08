fn main() {
    // Ensure linker links to the system GL library for native Skia builds on Linux.
    // Note: macOS uses OpenGL.framework, Windows uses opengl32.dll
    #[cfg(all(feature = "skia-native", target_os = "linux"))]
    println!("cargo:rustc-link-lib=GL");
}
