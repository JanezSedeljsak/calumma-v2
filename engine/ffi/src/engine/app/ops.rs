use super::Engine;
use calumma_core::MemoryPressureLevel;

impl Engine {
    pub fn set_memory_pressure(&mut self, level: MemoryPressureLevel) {
        let mut inner = self.inner.lock();
        if let Some(renderer) = &mut inner.renderer {
            renderer.set_memory_pressure(level);
        }
    }
}
