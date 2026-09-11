use super::clipboard::{read_image_files, NamedImage, IMAGE_EXTENSIONS};
use std::path::Path;

pub fn pick_artwork_files(filter: &str) -> Vec<NamedImage> {
    let paths = rfd::FileDialog::new()
        .add_filter(filter, IMAGE_EXTENSIONS)
        .pick_files()
        .unwrap_or_default();
    read_image_files(&paths)
}

pub fn save_bytes(bytes: &[u8], suggested: &str, extension: &str) -> bool {
    let path = rfd::FileDialog::new()
        .set_file_name(suggested)
        .add_filter(extension, &[extension])
        .save_file();
    match path {
        Some(path) => std::fs::write(path, bytes).is_ok(),
        None => false,
    }
}

pub fn save_text(text: &str, suggested: &str, extension: &str) -> bool {
    save_bytes(text.as_bytes(), suggested, extension)
}

pub fn project_basename(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("export");
    stem.to_string()
}
