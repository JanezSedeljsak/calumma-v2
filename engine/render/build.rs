fn main() {
    let shader = std::fs::read_to_string("src/shaders/board.wgsl").expect("board.wgsl");
    let module = naga::front::wgsl::parse_str(&shader).expect("WGSL parse");
    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    validator.validate(&module).expect("WGSL validate");
    println!("cargo:rerun-if-changed=src/shaders/board.wgsl");
}
