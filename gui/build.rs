fn main() {
    let mcp_devtools = std::env::var_os("CARGO_FEATURE_MCP_DEVTOOLS").is_some();
    let config = slint_build::CompilerConfiguration::new()
        .with_include_paths(vec![
            std::path::PathBuf::from("ui"),
            std::path::PathBuf::from("ui/calm"),
            std::path::PathBuf::from("../design/icons"),
        ])
        .with_debug_info(mcp_devtools);
    slint_build::compile_with_config("ui/app-window.slint", config).unwrap();
}
