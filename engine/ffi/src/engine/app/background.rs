use super::Engine;
use crate::engine::Inner;
use crate::subject::{self, crop_rgba, expand_matte, SubjectError};
use calumma_core::Document;
use parking_lot::Mutex;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum BackgroundNotice {
    Done,
    NoSubject,
    Unavailable,
    Failed(String),
}

struct RemovalJob {
    doc_id: String,
    layer_id: String,
    doc_width: u32,
    doc_height: u32,
    origin_x: u32,
    origin_y: u32,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

enum Prepared {
    Skip,
    Empty,
    Ready(RemovalJob),
}

impl Engine {
    pub fn background_removal_available(&self) -> bool {
        subject::AVAILABLE
    }

    pub fn background_removal_running(&self) -> bool {
        self.inner
            .lock()
            .doc
            .as_ref()
            .is_some_and(|doc| doc.background_removal_animating())
    }

    pub fn can_remove_background(&self, index: usize) -> bool {
        if !subject::AVAILABLE {
            return false;
        }
        let inner = self.inner.lock();
        let Some(doc) = inner.doc.as_ref() else {
            return false;
        };
        !doc.background_removal_animating() && doc.can_create_layer_mask(index)
    }

    pub fn take_background_notice(&mut self) -> Option<BackgroundNotice> {
        self.inner.lock().background_notice.take()
    }

    pub fn take_background_status(&mut self) -> (Option<BackgroundNotice>, bool) {
        let mut inner = self.inner.lock();
        let notice = inner.background_notice.take();
        let running = inner
            .doc
            .as_ref()
            .is_some_and(|doc| doc.background_removal_animating());
        (notice, running)
    }

    pub fn remove_background(&mut self, index: usize) -> bool {
        if !subject::AVAILABLE {
            self.inner.lock().background_notice = Some(BackgroundNotice::Unavailable);
            return false;
        }
        let mut inner = self.inner.lock();
        let prepared = (|| {
            let doc = inner.doc.as_mut()?;
            if doc.background_removal_animating() || !doc.can_create_layer_mask(index) {
                return Some(Prepared::Skip);
            }
            let Some((origin_x, origin_y, width, height)) = subject_rect(doc, index) else {
                return Some(Prepared::Empty);
            };
            let (doc_width, doc_height, rgba) = doc.layer_rgba(index)?;
            let rgba = crop_rgba(
                rgba, doc_width, doc_height, origin_x, origin_y, width, height,
            );
            let doc_id = doc.id.clone();
            let layer_id = doc.layers.get(index)?.id.clone();
            doc.begin_background_removal(layer_id.clone());
            Some(Prepared::Ready(RemovalJob {
                doc_id,
                layer_id,
                doc_width,
                doc_height,
                origin_x,
                origin_y,
                width,
                height,
                rgba,
            }))
        })();
        let job = match prepared {
            Some(Prepared::Ready(job)) => job,
            Some(Prepared::Empty) => {
                inner.background_notice = Some(BackgroundNotice::NoSubject);
                return false;
            }
            None | Some(Prepared::Skip) => return false,
        };
        inner.invalidate_overlay();
        drop(inner);
        let inner = Arc::clone(&self.inner);
        let spawned = std::thread::Builder::new()
            .name("miw-remove-background".into())
            .spawn(move || finish_removal(inner, job));
        if spawned.is_err() {
            let mut inner = self.inner.lock();
            if let Some(doc) = inner.doc.as_mut() {
                doc.clear_background_removal();
            }
            inner.background_notice = Some(BackgroundNotice::Failed("could not start".into()));
            inner.invalidate_overlay();
            return false;
        }
        true
    }
}

fn finish_removal(inner: Arc<Mutex<Inner>>, job: RemovalJob) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        subject::foreground_matte(&job.rgba, job.width, job.height)
    }));
    let result = match result {
        Ok(result) => result,
        Err(_) => Err(SubjectError::Failed("interrupted".into())),
    };
    apply_removal(&mut inner.lock(), &job, result);
}

fn apply_removal(inner: &mut Inner, job: &RemovalJob, result: Result<Vec<u8>, SubjectError>) {
    let same_document = inner.doc.as_ref().is_some_and(|doc| doc.id == job.doc_id);
    if let Some(doc) = inner.doc.as_mut() {
        if same_document {
            doc.clear_background_removal_for(&job.layer_id);
        }
    }
    if !same_document {
        return;
    }
    let index = inner
        .doc
        .as_ref()
        .and_then(|doc| doc.layers.iter().position(|layer| layer.id == job.layer_id));
    let Some(index) = index else {
        inner.invalidate_overlay();
        return;
    };
    match result {
        Ok(matte) => {
            let expected = (job.width as usize).saturating_mul(job.height as usize);
            if matte.len() != expected {
                inner.invalidate_overlay();
                inner.background_notice =
                    Some(BackgroundNotice::Failed("could not read the mask".into()));
                return;
            }
            let matte = expand_matte(
                job.doc_width,
                job.doc_height,
                job.origin_x,
                job.origin_y,
                job.width,
                job.height,
                &matte,
            );
            let applied = inner
                .doc
                .as_mut()
                .is_some_and(|doc| doc.create_layer_mask_from_matte(index, &matte));
            if applied {
                inner.dirty_save = true;
                inner.invalidate_renderer();
                inner.background_notice = Some(BackgroundNotice::Done);
            } else {
                inner.invalidate_overlay();
                inner.background_notice =
                    Some(BackgroundNotice::Failed("the layer changed".into()));
            }
        }
        Err(SubjectError::NoSubject) => {
            inner.invalidate_overlay();
            inner.background_notice = Some(BackgroundNotice::NoSubject);
        }
        Err(SubjectError::Unavailable) => {
            inner.invalidate_overlay();
            inner.background_notice = Some(BackgroundNotice::Unavailable);
        }
        Err(SubjectError::Failed(message)) => {
            inner.invalidate_overlay();
            inner.background_notice = Some(BackgroundNotice::Failed(message));
        }
    }
}

fn subject_rect(doc: &Document, index: usize) -> Option<(u32, u32, u32, u32)> {
    let layer = doc.layers.get(index)?;
    let bounds = layer.content_bounds()?;
    let (x0, y0, x1, y1) = layer.transform.unwrap_or_default().transformed_aabb(bounds);
    let doc_w = doc.width;
    let doc_h = doc.height;
    let left = axis_start(x0, doc_w);
    let top = axis_start(y0, doc_h);
    let right = axis_end(x1, doc_w);
    let bottom = axis_end(y1, doc_h);
    if right <= left || bottom <= top {
        return None;
    }
    Some((left, top, right - left, bottom - top))
}

fn axis_start(value: f32, limit: u32) -> u32 {
    let n = (value.floor() as i64).saturating_sub(1);
    n.clamp(0, limit as i64) as u32
}

fn axis_end(value: f32, limit: u32) -> u32 {
    let n = (value.ceil() as i64).saturating_add(1);
    n.clamp(0, limit as i64) as u32
}
