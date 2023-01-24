use std::{env, fs::File, io::Write, path::PathBuf};

fn main() {
    anchor_codegen::ConfigBuilder::new()
        .entry("src/main.rs")
        .set_version("rp2040_demo")
        .set_build_versions("")
        .build();

    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");
}
