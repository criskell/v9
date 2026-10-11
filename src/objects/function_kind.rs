#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum FunctionKind {
    #[default]
    NormalFunction,
    ArrowFunction,
}

impl FunctionKind {
    pub fn is_arrow_function(self) -> bool {
        self == FunctionKind::ArrowFunction
    }
}
