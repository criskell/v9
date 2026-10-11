/// `Runtime::FunctionId`
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u16)]
pub enum FunctionId {
    DefineAccessorPropertyUnchecked,
}

/// `Runtime::Function`
pub struct Function {
    pub function_id: FunctionId,
    pub name: &'static str,
    pub nargs: i32,
    pub result_size: i32,
}

const K_INTRINSIC_FUNCTIONS: &[Function] = &[Function {
    function_id: FunctionId::DefineAccessorPropertyUnchecked,
    name: "DefineAccessorPropertyUnchecked",
    nargs: 5,
    result_size: 1,
}];

impl FunctionId {
    pub fn to_u16(self) -> u16 {
        self as u16
    }

    /// `Runtime::FunctionForId`
    pub fn from_u16(id: u16) -> Option<&'static Function> {
        K_INTRINSIC_FUNCTIONS.get(id as usize)
    }

    pub fn function(self) -> &'static Function {
        &K_INTRINSIC_FUNCTIONS[self as usize]
    }
}

impl crate::execution::isolate::Isolate {
    pub(crate) fn call_runtime(
        &mut self,
        function_id: u16,
        args: &[crate::objects::tagged::Tagged],
    ) -> crate::interpreter::interpreter::ExecutionResult {
        let function = FunctionId::from_u16(function_id).expect("a runtime function");
        match function.function_id {
            FunctionId::DefineAccessorPropertyUnchecked => {
                self.runtime_define_accessor_property_unchecked(args)
            }
        }
    }
}
