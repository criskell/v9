#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum FunctionKind {
    #[default]
    NormalFunction,
    BaseConstructor,
    DerivedConstructor,
    ArrowFunction,
}

impl FunctionKind {
    pub fn is_arrow_function(self) -> bool {
        self == FunctionKind::ArrowFunction
    }

    pub fn is_derived_constructor(self) -> bool {
        self == FunctionKind::DerivedConstructor
    }
}
