use crate::engine::{
    cstring, pack_rgb, with_inner, CalmEngine, CalmProjectInfo, CalmStatus, Inner,
};
use anyhow::Context;
use calumma_io::WorkspaceListItem;
use std::ffi::{c_char, CStr};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::sync::Mutex;

#[repr(C)]
pub struct CalmWorkspaceInfo {
    pub id: *mut c_char,
    pub name: *mut c_char,
    pub accent: u32,
    pub active_project_id: *mut c_char,
    pub opened_at: i64,
}

fn write_workspaces(out: *mut CalmWorkspaceInfo, items: &[WorkspaceListItem], cap: usize) -> usize {
    let n = items.len().min(cap);
    for (i, item) in items.iter().take(n).enumerate() {
        unsafe {
            *out.add(i) = CalmWorkspaceInfo {
                id: cstring(&item.id),
                name: cstring(&item.name),
                accent: pack_rgb(item.accent),
                active_project_id: item
                    .active_project_id
                    .as_deref()
                    .map(cstring)
                    .unwrap_or(ptr::null_mut()),
                opened_at: item.opened_at,
            };
        }
    }
    n
}

fn write_projects(
    out: *mut CalmProjectInfo,
    items: &[calumma_io::ProjectListItem],
    cap: usize,
) -> usize {
    let n = items.len().min(cap);
    for (i, item) in items.iter().take(n).enumerate() {
        unsafe {
            *out.add(i) = CalmProjectInfo {
                id: cstring(&item.id),
                name: cstring(&item.name),
                width: item.width,
                height: item.height,
                opened_at: item.opened_at,
                accent: pack_rgb(item.accent),
            };
        }
    }
    n
}

#[no_mangle]
pub unsafe extern "C" fn calm_workspace_list(
    engine: *mut CalmEngine,
    out: *mut CalmWorkspaceInfo,
    cap: usize,
) -> usize {
    if engine.is_null() || out.is_null() || cap == 0 {
        return 0;
    }
    catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock().ok()?;
        let items = inner.store.list_workspaces(cap).unwrap_or_default();
        Some(write_workspaces(out, &items, cap))
    }))
    .ok()
    .flatten()
    .unwrap_or_default()
}

#[no_mangle]
pub unsafe extern "C" fn calm_workspace_create(
    engine: *mut CalmEngine,
    name: *const c_char,
) -> *mut c_char {
    if engine.is_null() || name.is_null() {
        return ptr::null_mut();
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex
            .lock()
            .map_err(|_| anyhow::anyhow!("engine mutex poisoned by an earlier panic"))?;
        let name = unsafe { CStr::from_ptr(name) }
            .to_str()
            .unwrap_or(calumma_core::UNTITLED);
        let ws = inner
            .store
            .create_workspace(name, None)
            .context("creating workspace")?;
        Ok::<_, anyhow::Error>(cstring(&ws.id))
    })) {
        Ok(Ok(p)) => p,
        _ => ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_workspace_rename(
    engine: *mut CalmEngine,
    id: *const c_char,
    name: *const c_char,
) -> CalmStatus {
    if id.is_null() || name.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let id = unsafe { CStr::from_ptr(id) }
            .to_str()
            .context("workspace id is not valid UTF-8")?;
        let name = unsafe { CStr::from_ptr(name) }
            .to_str()
            .context("workspace name is not valid UTF-8")?;
        inner
            .store
            .rename_workspace(id, name)
            .context("renaming workspace")?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_workspace_set_accent(
    engine: *mut CalmEngine,
    id: *const c_char,
    accent: u32,
) -> CalmStatus {
    if id.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let id = unsafe { CStr::from_ptr(id) }
            .to_str()
            .context("workspace id is not valid UTF-8")?;
        let rgb = [
            ((accent >> 16) & 0xFF) as u8,
            ((accent >> 8) & 0xFF) as u8,
            (accent & 0xFF) as u8,
        ];
        inner
            .store
            .set_workspace_accent(id, rgb)
            .context("setting workspace accent")?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_workspace_delete(
    engine: *mut CalmEngine,
    id: *const c_char,
) -> CalmStatus {
    if id.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let id = unsafe { CStr::from_ptr(id) }
            .to_str()
            .context("workspace id is not valid UTF-8")?;
        inner
            .store
            .delete_workspace(id)
            .context("deleting workspace")?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_workspace_add_project(
    engine: *mut CalmEngine,
    workspace_id: *const c_char,
    project_id: *const c_char,
) -> CalmStatus {
    if workspace_id.is_null() || project_id.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let workspace_id = unsafe { CStr::from_ptr(workspace_id) }
            .to_str()
            .context("workspace id is not valid UTF-8")?;
        let project_id = unsafe { CStr::from_ptr(project_id) }
            .to_str()
            .context("project id is not valid UTF-8")?;
        inner
            .store
            .add_project_to_workspace(workspace_id, project_id)
            .context("adding project to workspace")?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_workspace_remove_project(
    engine: *mut CalmEngine,
    workspace_id: *const c_char,
    project_id: *const c_char,
) -> CalmStatus {
    if workspace_id.is_null() || project_id.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let workspace_id = unsafe { CStr::from_ptr(workspace_id) }
            .to_str()
            .context("workspace id is not valid UTF-8")?;
        let project_id = unsafe { CStr::from_ptr(project_id) }
            .to_str()
            .context("project id is not valid UTF-8")?;
        inner
            .store
            .remove_project_from_workspace(workspace_id, project_id)
            .context("removing project from workspace")?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_workspace_projects(
    engine: *mut CalmEngine,
    workspace_id: *const c_char,
    out: *mut CalmProjectInfo,
    cap: usize,
) -> usize {
    if engine.is_null() || workspace_id.is_null() || out.is_null() || cap == 0 {
        return 0;
    }
    catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock().ok()?;
        let workspace_id = unsafe { CStr::from_ptr(workspace_id) }.to_str().ok()?;
        let items = inner
            .store
            .workspace_projects(workspace_id)
            .unwrap_or_default();
        Some(write_projects(out, &items, cap))
    }))
    .ok()
    .flatten()
    .unwrap_or_default()
}

#[no_mangle]
pub unsafe extern "C" fn calm_workspace_set_active_project(
    engine: *mut CalmEngine,
    workspace_id: *const c_char,
    project_id: *const c_char,
) -> CalmStatus {
    if workspace_id.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let workspace_id = unsafe { CStr::from_ptr(workspace_id) }
            .to_str()
            .context("workspace id is not valid UTF-8")?;
        let project_id = if project_id.is_null() {
            None
        } else {
            Some(
                unsafe { CStr::from_ptr(project_id) }
                    .to_str()
                    .context("project id is not valid UTF-8")?,
            )
        };
        inner
            .store
            .set_workspace_active_project(workspace_id, project_id)
            .context("setting active project")?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_workspace_switch(
    engine: *mut CalmEngine,
    workspace_id: *const c_char,
    project_id: *const c_char,
) -> CalmStatus {
    if workspace_id.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let workspace_id = unsafe { CStr::from_ptr(workspace_id) }
            .to_str()
            .context("workspace id is not valid UTF-8")?;
        let project_id = if project_id.is_null() {
            None
        } else {
            Some(
                unsafe { CStr::from_ptr(project_id) }
                    .to_str()
                    .context("project id is not valid UTF-8")?,
            )
        };
        inner.switch_workspace(workspace_id, project_id)
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_workspace_touch(
    engine: *mut CalmEngine,
    id: *const c_char,
) -> CalmStatus {
    if id.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let id = unsafe { CStr::from_ptr(id) }
            .to_str()
            .context("workspace id is not valid UTF-8")?;
        inner
            .store
            .touch_workspace(id)
            .context("touching workspace")?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_workspace_get(
    engine: *mut CalmEngine,
    id: *const c_char,
    out: *mut CalmWorkspaceInfo,
) -> CalmStatus {
    if id.is_null() || out.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let id = unsafe { CStr::from_ptr(id) }
            .to_str()
            .context("workspace id is not valid UTF-8")?;
        let item = inner.store.workspace(id).context("loading workspace")?;
        unsafe {
            *out = CalmWorkspaceInfo {
                id: cstring(&item.id),
                name: cstring(&item.name),
                accent: pack_rgb(item.accent),
                active_project_id: item
                    .active_project_id
                    .as_deref()
                    .map(cstring)
                    .unwrap_or(ptr::null_mut()),
                opened_at: item.opened_at,
            };
        }
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_open_workspace_tabs(
    engine: *mut CalmEngine,
    out: *mut *mut c_char,
    cap: usize,
) -> usize {
    if engine.is_null() || out.is_null() || cap == 0 {
        return 0;
    }
    catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex.lock().ok()?;
        let ids = inner.store.open_workspace_tabs().unwrap_or_default();
        let n = ids.len().min(cap);
        for (i, id) in ids.iter().take(n).enumerate() {
            unsafe {
                *out.add(i) = cstring(id);
            }
        }
        Some(n)
    }))
    .ok()
    .flatten()
    .unwrap_or_default()
}

#[no_mangle]
pub unsafe extern "C" fn calm_set_open_workspace_tabs(
    engine: *mut CalmEngine,
    ids: *const *const c_char,
    count: usize,
) -> CalmStatus {
    if ids.is_null() && count > 0 {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let mut owned = Vec::with_capacity(count);
        for i in 0..count {
            let ptr = unsafe { *ids.add(i) };
            if ptr.is_null() {
                continue;
            }
            let id = unsafe { CStr::from_ptr(ptr) }
                .to_str()
                .context("workspace tab id is not valid UTF-8")?;
            owned.push(id.to_string());
        }
        inner
            .store
            .set_open_workspace_tabs(&owned)
            .context("persisting open workspace tabs")?;
        Ok(())
    })
}

#[no_mangle]
pub unsafe extern "C" fn calm_workspace_for_project(
    engine: *mut CalmEngine,
    project_id: *const c_char,
) -> *mut c_char {
    if engine.is_null() || project_id.is_null() {
        return ptr::null_mut();
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex
            .lock()
            .map_err(|_| anyhow::anyhow!("engine mutex poisoned by an earlier panic"))?;
        let project_id = unsafe { CStr::from_ptr(project_id) }
            .to_str()
            .context("project id is not valid UTF-8")?;
        let ws = inner
            .store
            .workspace_containing_project(project_id)
            .context("looking up workspace for project")?;
        Ok::<_, anyhow::Error>(ws.map(|w| cstring(&w.id)).unwrap_or(ptr::null_mut()))
    })) {
        Ok(Ok(p)) => p,
        _ => ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_workspace_create_for_project(
    engine: *mut CalmEngine,
    project_id: *const c_char,
    name: *const c_char,
) -> *mut c_char {
    if engine.is_null() || project_id.is_null() || name.is_null() {
        return ptr::null_mut();
    }
    match catch_unwind(AssertUnwindSafe(|| {
        let mutex = unsafe { &*(engine as *const Mutex<Inner>) };
        let inner = mutex
            .lock()
            .map_err(|_| anyhow::anyhow!("engine mutex poisoned by an earlier panic"))?;
        let project_id = unsafe { CStr::from_ptr(project_id) }
            .to_str()
            .context("project id is not valid UTF-8")?;
        let name = unsafe { CStr::from_ptr(name) }
            .to_str()
            .unwrap_or(calumma_core::UNTITLED);
        let accent = if let Some(doc) = inner.doc.as_ref() {
            if doc.id == project_id {
                doc.accent
            } else {
                calumma_core::palette::color_for_seed(project_id)
            }
        } else {
            calumma_core::palette::color_for_seed(project_id)
        };
        let ws = inner
            .store
            .create_workspace_for_project(project_id, name, accent)
            .context("creating workspace for project")?;
        Ok::<_, anyhow::Error>(cstring(&ws.id))
    })) {
        Ok(Ok(p)) => p,
        _ => ptr::null_mut(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn calm_project_thumbnail(
    engine: *mut CalmEngine,
    project_id: *const c_char,
    out_png: *mut *mut u8,
    out_len: *mut usize,
) -> CalmStatus {
    if project_id.is_null() || out_png.is_null() || out_len.is_null() {
        return CalmStatus::Null;
    }
    with_inner(engine, |inner| {
        let project_id = unsafe { CStr::from_ptr(project_id) }
            .to_str()
            .context("project id is not valid UTF-8")?;
        let png = inner
            .store
            .project_thumbnail(project_id)
            .context("reading project thumbnail")?;
        let mut boxed = png.into_boxed_slice();
        let len = boxed.len();
        let ptr = boxed.as_mut_ptr();
        std::mem::forget(boxed);
        unsafe {
            *out_png = ptr;
            *out_len = len;
        }
        Ok(())
    })
}
