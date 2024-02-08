fn main() {
    anchor_codegen::ConfigBuilder::new()
        .entry("src/bin/minimal.rs")
        .set_version("rticjig 0.1")
        .set_build_versions("rust: someversion")
        .build()
}
