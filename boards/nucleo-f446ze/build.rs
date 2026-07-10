use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let out = Path::new(&out_dir);

    // For cortex-m-rt's own link.x (it INCLUDEs memory.x from CWD).
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();

    // For cortex-m-rt to find memory.x through its OUT_DIR discovery.
    println!("cargo:rustc-link-search={}", out.display());

    // Tell rustc to use cortex-m-rt's link.x (which then includes memory.x).
    println!("cargo:rustc-link-arg=-Tlink.x");

    println!("cargo:rerun-if-changed=memory.x");
}