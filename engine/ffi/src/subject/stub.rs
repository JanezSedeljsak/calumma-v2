use super::SubjectError;

pub(crate) const AVAILABLE: bool = false;

pub(crate) fn foreground_matte(
    _rgba: &[u8],
    _width: u32,
    _height: u32,
) -> Result<Vec<u8>, SubjectError> {
    Err(SubjectError::Unavailable)
}
