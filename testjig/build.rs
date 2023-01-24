fn main() {
    anchor_codegen::ConfigBuilder::new()
        .entry("src/main.rs")
        .set_version("jig")
        .set_build_versions("rust: someversion")
        .build()
}
