use super::layout::BoardRect;
use anyhow::{Context, Result};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::ffi::c_void;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn, RGN_DIFF,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, RegisterClassExW, SetWindowPos,
    ShowWindow, CS_HREDRAW, CS_OWNDC, CS_VREDRAW, HWND_TOP, SWP_NOACTIVATE, SWP_NOZORDER, SW_HIDE,
    SW_SHOWNA, WM_NCHITTEST, WNDCLASSEXW, WS_CHILD, WS_CLIPSIBLINGS, WS_EX_NOREDIRECTIONBITMAP,
};

const CLASS_NAME: PCWSTR = w!("MiwBoardSurface");

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCHITTEST {
        return LRESULT(-1);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn register_class() -> Result<()> {
    let instance = unsafe { GetModuleHandleW(None) }?;
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
        lpfnWndProc: Some(wnd_proc),
        hInstance: instance.into(),
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };
    if unsafe { RegisterClassExW(&class) } == 0 {
        let err = unsafe { GetLastError() };
        if err != ERROR_CLASS_ALREADY_EXISTS {
            return Err(windows::core::Error::from(err).into());
        }
    }
    Ok(())
}

fn parent_hwnd(winit_window: &winit::window::Window) -> Result<HWND> {
    let handle = winit_window
        .window_handle()
        .context("the Slint window has no native handle yet")?;
    match handle.as_raw() {
        RawWindowHandle::Win32(win32) => Ok(HWND(win32.hwnd.get() as *mut c_void)),
        other => anyhow::bail!("expected a Win32 window handle, got {other:?}"),
    }
}

fn px(value: f64, scale: f64) -> i32 {
    (value * scale).round() as i32
}

pub struct BoardSurface {
    hwnd: HWND,
}

impl BoardSurface {
    pub fn install(winit_window: &winit::window::Window, _scale: f64) -> Result<Self> {
        register_class()?;
        let parent = parent_hwnd(winit_window)?;
        let instance = unsafe { GetModuleHandleW(None) }?;
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP,
                CLASS_NAME,
                PCWSTR::null(),
                WS_CHILD | WS_CLIPSIBLINGS,
                0,
                0,
                1,
                1,
                Some(parent),
                None,
                Some(instance.into()),
                None,
            )
        }?;
        Ok(Self { hwnd })
    }

    pub fn set_frame(
        &self,
        x: f64,
        y_top: f64,
        width: f64,
        height: f64,
        _content_height: f64,
        scale: f64,
    ) {
        if width < 1.0 || height < 1.0 {
            self.set_hidden(true);
            return;
        }
        let _ = unsafe {
            SetWindowPos(
                self.hwnd,
                Some(HWND_TOP),
                px(x, scale),
                px(y_top, scale),
                px(width, scale).max(1),
                px(height, scale).max(1),
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
        };
        self.set_hidden(false);
    }

    pub fn set_hidden(&self, hidden: bool) {
        let cmd = if hidden { SW_HIDE } else { SW_SHOWNA };
        let _ = unsafe { ShowWindow(self.hwnd, cmd) };
    }

    pub fn set_holes(&self, holes: &[BoardRect], scale: f64) {
        let mut rect = RECT::default();
        if unsafe { GetClientRect(self.hwnd, &mut rect) }.is_err() {
            return;
        }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            return;
        }
        let full = unsafe { CreateRectRgn(0, 0, width, height) };
        if full.is_invalid() {
            return;
        }
        for hole in holes {
            let x = px(hole.x as f64, scale);
            let y = px(hole.y as f64, scale);
            let w = px(hole.width as f64, scale).max(1);
            let h = px(hole.height as f64, scale).max(1);
            let radius = px(hole.radius as f64, scale).max(0);
            let punch = if radius > 0 {
                unsafe { CreateRoundRectRgn(x, y, x + w, y + h, radius * 2, radius * 2) }
            } else {
                unsafe { CreateRectRgn(x, y, x + w, y + h) }
            };
            if punch.is_invalid() {
                continue;
            }
            let _ = unsafe { CombineRgn(Some(full), Some(full), Some(punch), RGN_DIFF) };
            let _ = unsafe { DeleteObject(punch.into()) };
        }
        let _ = unsafe { SetWindowRgn(self.hwnd, Some(full), true) };
    }

    pub fn native(&self) -> calumma_app::NativeSurface {
        calumma_app::NativeSurface::Win32Hwnd { hwnd: self.hwnd.0 }
    }
}

impl Drop for BoardSurface {
    fn drop(&mut self) {
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }
}
