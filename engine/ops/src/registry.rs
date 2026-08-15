use crate::types::{Backend, Op, OpError, OpInput, OpKind, OpOutput, OpParams};
use rustc_hash::FxHashMap;

#[derive(Default)]
pub struct OpRegistry {
    core: FxHashMap<OpKind, Box<dyn Op>>,
    platform: FxHashMap<OpKind, Box<dyn Op>>,
}

impl OpRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_core(&mut self, op: Box<dyn Op>) {
        self.core.insert(op.kind(), op);
    }

    pub fn register_platform(&mut self, op: Box<dyn Op>) {
        self.platform.insert(op.kind(), op);
    }

    pub fn resolve(&self, kind: OpKind) -> Option<&dyn Op> {
        if let Some(op) = self.platform.get(&kind) {
            if op.available() {
                return Some(op.as_ref());
            }
        }
        self.core
            .get(&kind)
            .filter(|op| op.available())
            .map(|op| op.as_ref())
    }

    pub fn available(&self, kind: OpKind) -> bool {
        self.resolve(kind).is_some()
    }

    pub fn backend_for(&self, kind: OpKind) -> Option<Backend> {
        self.resolve(kind).map(|op| op.backend())
    }

    pub fn run(
        &self,
        kind: OpKind,
        input: OpInput,
        params: &OpParams,
    ) -> Result<OpOutput, OpError> {
        let op = self.resolve(kind).ok_or(OpError::Unavailable)?;
        op.run(input, params)
    }
}
