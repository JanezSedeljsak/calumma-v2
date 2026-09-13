use crate::types::{Op, OpError, OpInput, OpKind, OpOutput, OpParams};
use rustc_hash::FxHashMap;

#[derive(Default)]
pub struct OpRegistry {
    ops: FxHashMap<OpKind, Box<dyn Op>>,
}

impl OpRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, op: Box<dyn Op>) {
        self.ops.insert(op.kind(), op);
    }

    pub fn resolve(&self, kind: OpKind) -> Option<&dyn Op> {
        self.ops
            .get(&kind)
            .filter(|op| op.available())
            .map(|op| op.as_ref())
    }

    pub fn available(&self, kind: OpKind) -> bool {
        self.resolve(kind).is_some()
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
