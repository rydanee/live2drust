fn main() {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("Failed to get CARGO_MANIFEST_DIR");

    cc::Build::new()
        .cpp(true)
        .file("src/sdk/interaction.cpp")
        .include("src/sdk")
        .compile("interaction");
    println!("cargo:rustc-link-search=native={}/src/libs", manifest_dir);
    println!("cargo:rerun-if-changed=src/sdk/interaction.cpp");

    println!("cargo:rustc-link-lib=static=Live2DCubismCore");
}
