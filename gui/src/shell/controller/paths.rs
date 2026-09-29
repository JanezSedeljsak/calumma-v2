//! Where the shell finds its resources: the repo checkout in development, the app bundle when
//! shipped.

use super::*;

pub fn workspace_root() -> PathBuf {
    if let Some(root) = bundle_resources_root() {
        return root;
    }
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..8 {
        if is_workspace_root(&dir) {
            return dir;
        }
        if !dir.pop() {
            break;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(super) fn is_workspace_root(dir: &Path) -> bool {
    dir.join("design").join("icon.png").is_file() && dir.join("translations").is_dir()
}

pub(super) fn bundle_resources_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    if exe_dir.file_name()?.to_str()? == "MacOS" {
        let resources = exe_dir.parent()?.join("Resources");
        if is_workspace_root(&resources) {
            return Some(resources);
        }
    }
    if is_workspace_root(exe_dir) {
        return Some(exe_dir.to_path_buf());
    }
    let share = exe_dir.parent()?.join("share").join("miw");
    is_workspace_root(&share).then_some(share)
}
