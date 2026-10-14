fn main() {
    // Native media layer: AVFoundation / Vision / Core Image / CoreText.
    println!("cargo:rerun-if-changed=native/clipnative.m");
    println!("cargo:rerun-if-changed=native/clipnative.h");
    cc::Build::new()
        .file("native/clipnative.m")
        .flag("-fobjc-arc")
        .flag("-fmodules")
        .flag("-mmacosx-version-min=13.0")
        .flag_if_supported("-Wno-deprecated-declarations")
        .compile("clipnative");
    for f in ["AVFoundation", "CoreImage", "CoreText", "CoreGraphics", "ImageIO", "Vision", "CoreVideo", "CoreMedia", "Foundation"] {
        println!("cargo:rustc-link-lib=framework={f}");
    }
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    tauri_build::build()
}
