use super::scale::{downscale_premultiplied, fitted_size, quantize_coverage, resample_coverage};
use super::SubjectError;
use calumma_core::limits::IMPORT_MAX_SIDE;
use objc2::rc::autoreleasepool;
use objc2::runtime::AnyObject;
use objc2::AnyThread;
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo,
};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType, CVPixelBufferGetWidth,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSArray, NSDictionary, NSError};
use objc2_vision::{VNGenerateForegroundInstanceMaskRequest, VNImageOption, VNImageRequestHandler};
use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::NonNull;

pub(crate) const AVAILABLE: bool = true;

const ONE_COMPONENT_8: u32 = u32::from_be_bytes(*b"L008");
const ONE_COMPONENT_F32: u32 = u32::from_be_bytes(*b"L00f");
const BGRA: u32 = u32::from_be_bytes(*b"BGRA");

pub(crate) fn foreground_matte(
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, SubjectError> {
    match catch_unwind(AssertUnwindSafe(|| {
        autoreleasepool(|_| foreground_matte_inner(rgba, width, height))
    })) {
        Ok(result) => result,
        Err(_) => Err(SubjectError::Failed("interrupted".into())),
    }
}

fn foreground_matte_inner(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, SubjectError> {
    let expected = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    if width == 0 || height == 0 || rgba.len() != expected {
        return Err(SubjectError::Failed("empty layer".into()));
    }
    let (sent_w, sent_h) = fitted_size(width, height, IMPORT_MAX_SIDE);
    let sent = downscale_premultiplied(rgba, width, height, sent_w, sent_h);
    let (coverage, mask_w, mask_h) = unsafe { vision_coverage(&sent, sent_w, sent_h)? };
    let coverage = resample_coverage(&coverage, mask_w, mask_h, width, height);
    let matte = quantize_coverage(&coverage);
    if matte.iter().all(|&value| value == 0) {
        return Err(SubjectError::NoSubject);
    }
    Ok(matte)
}

unsafe fn vision_coverage(
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<(Vec<f32>, u32, u32), SubjectError> {
    let provider = CGDataProvider::with_data(
        std::ptr::null_mut(),
        rgba.as_ptr().cast(),
        rgba.len(),
        Some(keep_bytes),
    )
    .ok_or_else(|| SubjectError::Failed("could not read the layer".into()))?;
    let space = CGColorSpace::new_device_rgb()
        .ok_or_else(|| SubjectError::Failed("could not read the layer".into()))?;
    let info =
        CGBitmapInfo(CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0);
    let image = CGImage::new(
        width as usize,
        height as usize,
        8,
        32,
        (width as usize) * 4,
        Some(&space),
        info,
        Some(&provider),
        std::ptr::null(),
        false,
        CGColorRenderingIntent::RenderingIntentDefault,
    )
    .ok_or_else(|| SubjectError::Failed("could not read the layer".into()))?;
    let request = VNGenerateForegroundInstanceMaskRequest::init(
        VNGenerateForegroundInstanceMaskRequest::alloc(),
    );
    request.setPreferBackgroundProcessing(true);
    let requests = NSArray::from_slice(&[request.as_ref()]);
    let options: objc2::rc::Retained<NSDictionary<VNImageOption, AnyObject>> = NSDictionary::new();
    let handler = VNImageRequestHandler::initWithCGImage_options(
        VNImageRequestHandler::alloc(),
        &image,
        &options,
    );
    handler
        .performRequests_error(&requests)
        .map_err(|error| SubjectError::Failed(error_text(&error)))?;
    let Some(results) = request.results() else {
        return Err(SubjectError::NoSubject);
    };
    let Some(observation) = results.firstObject() else {
        return Err(SubjectError::NoSubject);
    };
    let instances = observation.allInstances();
    if instances.count() == 0 {
        return Err(SubjectError::NoSubject);
    }
    let buffer = observation
        .generateScaledMaskForImageForInstances_fromRequestHandler_error(&instances, &handler)
        .map_err(|error| SubjectError::Failed(error_text(&error)))?;
    read_coverage(&buffer)
}

fn error_text(error: &NSError) -> String {
    error.localizedDescription().to_string()
}

unsafe extern "C-unwind" fn keep_bytes(_info: *mut c_void, _data: NonNull<c_void>, _size: usize) {}

unsafe fn read_coverage(buffer: &CVPixelBuffer) -> Result<(Vec<f32>, u32, u32), SubjectError> {
    let flags = CVPixelBufferLockFlags::ReadOnly;
    if CVPixelBufferLockBaseAddress(buffer, flags) != 0 {
        return Err(SubjectError::Failed("could not read the mask".into()));
    }
    let result = read_coverage_locked(buffer);
    CVPixelBufferUnlockBaseAddress(buffer, flags);
    result
}

unsafe fn read_coverage_locked(
    buffer: &CVPixelBuffer,
) -> Result<(Vec<f32>, u32, u32), SubjectError> {
    let width = CVPixelBufferGetWidth(buffer) as u32;
    let height = CVPixelBufferGetHeight(buffer) as u32;
    let stride = CVPixelBufferGetBytesPerRow(buffer);
    let format = CVPixelBufferGetPixelFormatType(buffer);
    let base = CVPixelBufferGetBaseAddress(buffer);
    let bpp: usize = match format {
        ONE_COMPONENT_8 => 1,
        ONE_COMPONENT_F32 | BGRA => 4,
        _ => {
            return Err(SubjectError::Failed(format!(
                "unexpected mask format {format:#x}"
            )));
        }
    };
    let row_bytes = bpp.saturating_mul(width as usize);
    let Some(pixels) = (width as usize).checked_mul(height as usize) else {
        return Err(SubjectError::Failed("could not read the mask".into()));
    };
    if base.is_null() || pixels == 0 || stride < row_bytes || height as usize > usize::MAX / stride
    {
        return Err(SubjectError::Failed("could not read the mask".into()));
    }
    let mut out = vec![0.0; pixels];
    let base = base.cast::<u8>();
    for y in 0..height {
        let row = base.add(y as usize * stride);
        for x in 0..width {
            out[(y * width + x) as usize] = match format {
                ONE_COMPONENT_8 => *row.add(x as usize) as f32 / 255.0,
                ONE_COMPONENT_F32 => {
                    let value = row.add(x as usize * 4).cast::<f32>().read_unaligned();
                    value.clamp(0.0, 1.0)
                }
                BGRA => *row.add(x as usize * 4 + 3) as f32 / 255.0,
                _ => unreachable!(),
            };
        }
    }
    Ok((out, width, height))
}
