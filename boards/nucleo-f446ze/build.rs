use std::path::PathBuf;

fn main() {
    let project_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // Copy our link.x to OUT_DIR so the linker can find it
    let link_src = project_dir.join("link.x");
    let link_out = out.join("link.x");
    let _ = std::fs::copy(&link_src, &link_out);

    // Expose OUT_DIR as a link search path
    println!("cargo:rustc-link-search={}", out.display());

    // Tell the linker to use our custom link.x
    println!("cargo:rustc-link-arg=-Tlink.x");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=link.x");
}
