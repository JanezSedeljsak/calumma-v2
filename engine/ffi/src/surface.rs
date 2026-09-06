//! Turning a shell's native window into a wgpu surface.
//!
//! macOS hands over a `CAMetalLayer` and nothing else, which is why the original
//! `calm_engine_attach_surface` could take a bare `void *`. Win32 needs an `HWND`, X11 needs a
//! `Display *` **and** an XID, and Wayland needs a `wl_display *` **and** a `wl_surface *` — two
//! values, so one pointer no longer says enough. Everything platform-shaped about attaching
//! lives here; `engine.rs` only knows it gets a `SurfaceTargetUnsafe` back.

use anyhow::{bail, Result};
use num_enum::TryFromPrimitive;
use std::ffi::c_void;

#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, TryFromPrimitive)]
pub enum CalmSurfaceKind {
    MetalLayer = 0,
    Win32Hwnd = 1,
    Xlib = 2,
    Wayland = 3,
}

/// What the shell knows about its own window.
///
/// `kind` is a plain `u32` rather than [`CalmSurfaceKind`] on purpose: this struct is filled in
/// across the ABI, and materialising an out-of-range discriminant straight into a Rust enum
/// would be undefined behaviour. It is parsed and rejected instead. (A C enum of these values
/// is an `int`, which matches a `u32` field in size and alignment — the same pairing
/// `CalmOpOutputKind` already relies on.)
///
/// `display` is unused for Metal and Win32. `window` is a pointer for Metal, Win32 and Wayland;
/// for X11 it carries an XID in its bits rather than an address, because that is what Xlib's
/// `Window` is.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CalmNativeSurface {
    pub kind: u32,
    pub display: *mut c_void,
    pub window: *mut c_void,
}

/// The backends wgpu is compiled with here, matching the per-target features in `Cargo.toml`.
/// Naming them rather than passing `Backends::all()` keeps the failure honest: an adapter that
/// could only be reached through a backend this build does not contain should not be found at
/// all, instead of being found and then failing later at surface creation.
pub(crate) fn preferred_backends() -> wgpu::Backends {
    #[cfg(target_os = "macos")]
    {
        wgpu::Backends::METAL
    }
    #[cfg(target_os = "windows")]
    {
        wgpu::Backends::DX12
    }
    #[cfg(target_os = "linux")]
    {
        wgpu::Backends::VULKAN
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        wgpu::Backends::empty()
    }
}

/// # Safety
///
/// `surface`'s handles must be live for as long as the wgpu surface built from them, and must
/// really be of the kind `surface.kind` claims. Both are the shell's promise: the pointers are
/// opaque here, so nothing in this function can check them beyond null.
pub(crate) unsafe fn surface_target(
    surface: &CalmNativeSurface,
) -> Result<wgpu::SurfaceTargetUnsafe> {
    let Ok(kind) = CalmSurfaceKind::try_from(surface.kind) else {
        bail!("unknown surface kind {}", surface.kind);
    };
    if surface.window.is_null() {
        bail!("attach needs a non-null window handle for {kind:?}");
    }
    match kind {
        CalmSurfaceKind::MetalLayer => metal_target(surface.window),
        CalmSurfaceKind::Win32Hwnd => win32_target(surface.window),
        CalmSurfaceKind::Xlib => xlib_target(surface.display, surface.window),
        CalmSurfaceKind::Wayland => wayland_target(surface.display, surface.window),
    }
}

/// Each of these refuses on the platforms that cannot present it at all, rather than building a
/// handle wgpu would reject later with a message about a backend that was never compiled in.
#[allow(unused_variables)]
fn metal_target(window: *mut c_void) -> Result<wgpu::SurfaceTargetUnsafe> {
    #[cfg(target_os = "macos")]
    {
        Ok(wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(window))
    }
    #[cfg(not(target_os = "macos"))]
    {
        bail!("a CAMetalLayer surface is only presentable on macOS")
    }
}

#[allow(unused_variables)]
fn win32_target(window: *mut c_void) -> Result<wgpu::SurfaceTargetUnsafe> {
    #[cfg(target_os = "windows")]
    {
        use std::num::NonZeroIsize;
        let Some(hwnd) = NonZeroIsize::new(window as isize) else {
            bail!("attach needs a non-null HWND");
        };
        Ok(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(wgpu::rwh::RawDisplayHandle::Windows(
                wgpu::rwh::WindowsDisplayHandle::new(),
            )),
            raw_window_handle: wgpu::rwh::RawWindowHandle::Win32(
                wgpu::rwh::Win32WindowHandle::new(hwnd),
            ),
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        bail!("an HWND surface is only presentable on Windows")
    }
}

#[allow(unused_variables)]
fn xlib_target(display: *mut c_void, window: *mut c_void) -> Result<wgpu::SurfaceTargetUnsafe> {
    #[cfg(target_os = "linux")]
    {
        use std::ffi::c_ulong;
        use std::ptr::NonNull;
        // Xlib's `Window` is an XID, not an address — Qt's `QWindow::winId()` returns it as a
        // `WId`, and it travels through the `void *` field as those same bits.
        let Some(display) = NonNull::new(display) else {
            bail!("attach needs a non-null X11 Display *");
        };
        Ok(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(wgpu::rwh::RawDisplayHandle::Xlib(
                wgpu::rwh::XlibDisplayHandle::new(Some(display), 0),
            )),
            raw_window_handle: wgpu::rwh::RawWindowHandle::Xlib(wgpu::rwh::XlibWindowHandle::new(
                window as c_ulong,
            )),
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        bail!("an X11 surface is only presentable on Linux")
    }
}

#[allow(unused_variables)]
fn wayland_target(display: *mut c_void, window: *mut c_void) -> Result<wgpu::SurfaceTargetUnsafe> {
    #[cfg(target_os = "linux")]
    {
        use std::ptr::NonNull;
        let (Some(display), Some(surface)) = (NonNull::new(display), NonNull::new(window)) else {
            bail!("attach needs a non-null wl_display * and wl_surface *");
        };
        Ok(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(wgpu::rwh::RawDisplayHandle::Wayland(
                wgpu::rwh::WaylandDisplayHandle::new(display),
            )),
            raw_window_handle: wgpu::rwh::RawWindowHandle::Wayland(
                wgpu::rwh::WaylandWindowHandle::new(surface),
            ),
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        bail!("a Wayland surface is only presentable on Linux")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

    fn described(kind: u32, display: *mut c_void, window: *mut c_void) -> CalmNativeSurface {
        CalmNativeSurface {
            kind,
            display,
            window,
        }
    }

    /// The one value that is never a handle. Nothing dereferences it — `surface_target` only
    /// ever moves these bits into a wgpu handle struct — but every test here stops before that
    /// would matter anyway.
    fn fake() -> *mut c_void {
        0x1 as *mut c_void
    }

    #[test]
    fn a_wire_value_outside_the_enum_is_refused_rather_than_transmuted() {
        for kind in [4u32, 99, u32::MAX] {
            let surface = described(kind, ptr::null_mut(), fake());
            assert!(unsafe { surface_target(&surface) }.is_err(), "kind {kind}");
        }
    }

    #[test]
    fn every_kind_needs_a_window_handle() {
        for kind in 0..=3u32 {
            let surface = described(kind, fake(), ptr::null_mut());
            assert!(unsafe { surface_target(&surface) }.is_err(), "kind {kind}");
        }
    }

    /// X11 and Wayland both carry two handles, and a surface built with a null display would
    /// fail deep inside the backend rather than here.
    #[test]
    fn the_two_handle_kinds_need_a_display_too() {
        for kind in [CalmSurfaceKind::Xlib, CalmSurfaceKind::Wayland] {
            let surface = described(kind as u32, ptr::null_mut(), fake());
            assert!(unsafe { surface_target(&surface) }.is_err(), "{kind:?}");
        }
    }

    /// Cross-platform honesty: asking the macOS build for an HWND surface has to fail here, not
    /// somewhere in wgpu that reports a missing backend instead of a wrong kind.
    #[test]
    fn a_kind_this_build_cannot_present_is_refused_at_the_boundary() {
        let foreign: [CalmSurfaceKind; 3] = if cfg!(target_os = "macos") {
            [
                CalmSurfaceKind::Win32Hwnd,
                CalmSurfaceKind::Xlib,
                CalmSurfaceKind::Wayland,
            ]
        } else if cfg!(target_os = "windows") {
            [
                CalmSurfaceKind::MetalLayer,
                CalmSurfaceKind::Xlib,
                CalmSurfaceKind::Wayland,
            ]
        } else {
            [
                CalmSurfaceKind::MetalLayer,
                CalmSurfaceKind::Win32Hwnd,
                CalmSurfaceKind::MetalLayer,
            ]
        };
        for kind in foreign {
            let surface = described(kind as u32, fake(), fake());
            assert!(unsafe { surface_target(&surface) }.is_err(), "{kind:?}");
        }
    }

    #[test]
    fn the_build_can_present_exactly_one_backend() {
        let backends = preferred_backends();
        assert_eq!(
            backends.iter().count(),
            1,
            "one target, one backend: {backends:?}"
        );
    }
}

/// `Calumma.hpp` is written by hand and nothing cross-checks it against this file, so the one
/// thing that would corrupt every field silently — the two structs disagreeing on layout — is
/// pinned here. The numbers are what a C compiler reports for `CalmNativeSurface`: a 4-byte
/// enum, four bytes of padding, then two pointers.
#[cfg(test)]
mod layout {
    use super::CalmNativeSurface;
    use std::mem::{align_of, offset_of, size_of};

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn the_struct_matches_the_one_in_calumma_h() {
        assert_eq!(size_of::<CalmNativeSurface>(), 24);
        assert_eq!(align_of::<CalmNativeSurface>(), 8);
        assert_eq!(offset_of!(CalmNativeSurface, kind), 0);
        assert_eq!(offset_of!(CalmNativeSurface, display), 8);
        assert_eq!(offset_of!(CalmNativeSurface, window), 16);
    }
}
