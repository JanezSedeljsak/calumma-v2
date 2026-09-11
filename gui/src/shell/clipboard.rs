use std::path::{Path, PathBuf};

pub const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "avif", "heic", "heif", "ico", "psd", "svg", "tif", "tiff",
];

const RAW_IMAGE_TYPES: &[&str] = &[
    "public.svg-image",
    "public.png",
    "org.webmproject.webp",
    "public.avif",
    "public.heic",
    "public.heif",
    "public.jpeg",
    "com.microsoft.ico",
    "public.tiff",
];

fn looks_like_svg_text(text: &str) -> bool {
    let head = text.trim_start();
    head.starts_with("<svg") || (head.starts_with("<?xml") && head.contains("<svg"))
}

pub struct NamedImage {
    pub name: String,
    pub bytes: Vec<u8>,
}

pub enum ClipboardContent {
    Images(Vec<NamedImage>),
    Text(String),
    Empty,
}

pub fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

pub fn read_image_files(paths: &[PathBuf]) -> Vec<NamedImage> {
    paths
        .iter()
        .filter(|path| is_image_path(path))
        .filter_map(|path| {
            let bytes = std::fs::read(path).ok()?;
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_string();
            Some(NamedImage { name, bytes })
        })
        .collect()
}

#[cfg(target_os = "macos")]
pub fn read_clipboard() -> ClipboardContent {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypeString};
    use objc2_foundation::{NSString, NSURL};

    let board = NSPasteboard::generalPasteboard();
    let mut files = Vec::new();
    let mut images = Vec::new();
    if let Some(items) = board.pasteboardItems() {
        let raw_types: Vec<_> = RAW_IMAGE_TYPES
            .iter()
            .map(|kind| NSString::from_str(kind))
            .collect();
        for item in items.iter() {
            let file_url = item.stringForType(unsafe { NSPasteboardTypeFileURL });
            if let Some(url) = file_url.and_then(|s| NSURL::URLWithString(&s)) {
                if let Some(path) = url.path() {
                    files.push(PathBuf::from(path.to_string()));
                }
                continue;
            }
            if let Some(data) = raw_types.iter().find_map(|kind| item.dataForType(kind)) {
                images.push(NamedImage {
                    name: String::new(),
                    bytes: data.to_vec(),
                });
            }
        }
    }
    images.extend(read_image_files(&files));
    if !images.is_empty() {
        return ClipboardContent::Images(images);
    }
    let text = board
        .stringForType(unsafe { NSPasteboardTypeString })
        .map(|text| text.to_string())
        .unwrap_or_default();
    if looks_like_svg_text(&text) {
        return ClipboardContent::Images(vec![NamedImage {
            name: String::new(),
            bytes: text.into_bytes(),
        }]);
    }
    if text.is_empty() {
        ClipboardContent::Empty
    } else {
        ClipboardContent::Text(text)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn read_clipboard() -> ClipboardContent {
    ClipboardContent::Empty
}
