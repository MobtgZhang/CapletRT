use std::path::PathBuf;
fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src = manifest.join("memory.x");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let _ = std::fs::copy(&src, out.join("memory.x"));
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rustc-link-arg=-Tlink.x");
}
