#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod scale;

#[cfg(not(target_os = "macos"))]
mod stub;
#[cfg(target_os = "macos")]
mod vision;

#[derive(Debug)]
pub(crate) enum SubjectError {
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    Unavailable,
    NoSubject,
    Failed(String),
}

#[cfg(not(target_os = "macos"))]
pub(crate) use stub::{foreground_matte, AVAILABLE};
#[cfg(target_os = "macos")]
pub(crate) use vision::{foreground_matte, AVAILABLE};

pub(crate) use scale::{crop_rgba, expand_matte};
