fn main() {
    let config = slint_build::CompilerConfiguration::new().with_include_paths(vec![
        std::path::PathBuf::from("ui"),
        std::path::PathBuf::from("ui/calm"),
        std::path::PathBuf::from("../design/icons"),
    ]);
    slint_build::compile_with_config("ui/app-window.slint", config).unwrap();
}
