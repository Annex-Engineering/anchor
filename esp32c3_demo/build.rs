fn main() {
    anchor_codegen::ConfigBuilder::new()
        .entry("src/main.rs")
        .set_version("esp32c3_demo")
        .set_build_versions("")
        .build()
}