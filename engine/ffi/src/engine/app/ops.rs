use super::{Engine, Inner};
use crate::engine::base_op_registry;
use crate::platform::{CalmPlatformOps, PlatformOp};
use anyhow::{bail, Context, Result};
use calumma_core::MemoryPressureLevel;
use calumma_ops::{apply_output, layer_input, run_op_on_document, Backend, Op, OpKind, OpParams};

impl Engine {
    pub fn install_platform_ops(&mut self, ops: CalmPlatformOps) {
        let mut inner = self.inner.lock();
        inner.platform_ops = Some(ops);
        inner.registry = base_op_registry();
        inner
            .registry
            .register_platform(Box::new(PlatformOp::remove_background(ops)));
    }

    pub fn set_memory_pressure(&mut self, level: MemoryPressureLevel) {
        let mut inner = self.inner.lock();
        if let Some(renderer) = &mut inner.renderer {
            renderer.set_memory_pressure(level);
        }
    }

    pub fn run_op(&mut self, kind: OpKind, layer_index: usize, params: OpParams) -> Result<()> {
        let mut inner = self.inner.lock();
        match inner.registry.backend_for(kind) {
            Some(Backend::Core) => {
                let Inner {
                    doc,
                    registry,
                    dirty_save,
                    ..
                } = &mut *inner;
                let doc = doc.as_mut().context("no project is open")?;
                run_op_on_document(registry, doc, layer_index, kind, &params)
                    .context("running the core op")?;
                *dirty_save = true;
                inner.invalidate_renderer();
                Ok(())
            }
            Some(Backend::Platform) => {
                if !inner.registry.available(kind) {
                    bail!("op {kind:?} has no registered platform backend");
                }
                let ops = inner.platform_ops.context("no platform ops registered")?;
                let doc = inner.doc.as_ref().context("no project is open")?;
                let input =
                    layer_input(doc, layer_index).context("reading the layer for the op")?;
                let output = PlatformOp::for_kind(kind, ops)
                    .run(input, &params)
                    .context("running the platform op")?;
                let Inner {
                    doc, dirty_save, ..
                } = &mut *inner;
                let doc = doc.as_mut().context("no project is open")?;
                apply_output(doc, layer_index, output).context("applying the op result")?;
                *dirty_save = true;
                inner.invalidate_renderer();
                Ok(())
            }
            None => bail!("op {kind:?} is not available on this platform"),
        }
    }
}
