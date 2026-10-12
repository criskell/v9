#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u16)]
pub enum FunctionId {
    DefineAccessorPropertyUnchecked,
    DefineClass,
    ToFastProperties,
    ThrowStaticPrototypeError,
    CreatePrivateNameSymbol,
    CreatePrivateBrandSymbol,
    CreatePrivateAccessors,
    LoadPrivateGetter,
    LoadPrivateSetter,
}

pub struct Function {
    pub function_id: FunctionId,
    pub name: &'static str,
    pub nargs: i32,
    pub result_size: i32,
}

const K_INTRINSIC_FUNCTIONS: &[Function] = &[
    Function {
        function_id: FunctionId::DefineAccessorPropertyUnchecked,
        name: "DefineAccessorPropertyUnchecked",
        nargs: 5,
        result_size: 1,
    },
    Function {
        function_id: FunctionId::DefineClass,
        name: "DefineClass",
        nargs: -1,
        result_size: 1,
    },
    Function {
        function_id: FunctionId::ToFastProperties,
        name: "ToFastProperties",
        nargs: 1,
        result_size: 1,
    },
    Function {
        function_id: FunctionId::ThrowStaticPrototypeError,
        name: "ThrowStaticPrototypeError",
        nargs: 0,
        result_size: 1,
    },
    Function {
        function_id: FunctionId::CreatePrivateNameSymbol,
        name: "CreatePrivateNameSymbol",
        nargs: 1,
        result_size: 1,
    },
    Function {
        function_id: FunctionId::CreatePrivateBrandSymbol,
        name: "CreatePrivateBrandSymbol",
        nargs: 1,
        result_size: 1,
    },
    Function {
        function_id: FunctionId::CreatePrivateAccessors,
        name: "CreatePrivateAccessors",
        nargs: 2,
        result_size: 1,
    },
    Function {
        function_id: FunctionId::LoadPrivateGetter,
        name: "LoadPrivateGetter",
        nargs: 1,
        result_size: 1,
    },
    Function {
        function_id: FunctionId::LoadPrivateSetter,
        name: "LoadPrivateSetter",
        nargs: 1,
        result_size: 1,
    },
];

impl FunctionId {
    pub fn to_u16(self) -> u16 {
        self as u16
    }

    pub fn from_u16(id: u16) -> Option<&'static Function> {
        K_INTRINSIC_FUNCTIONS.get(id as usize)
    }

    pub fn function(self) -> &'static Function {
        &K_INTRINSIC_FUNCTIONS[self as usize]
    }

    pub fn function_for_name(name: &str) -> Option<&'static Function> {
        K_INTRINSIC_FUNCTIONS
            .iter()
            .find(|function| function.name == name)
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
            FunctionId::DefineClass => self.runtime_define_class(args),
            FunctionId::ToFastProperties => self.runtime_to_fast_properties(args),
            FunctionId::ThrowStaticPrototypeError => self.runtime_throw_static_prototype_error(),
            FunctionId::CreatePrivateNameSymbol => self.runtime_create_private_name_symbol(args),
            FunctionId::CreatePrivateBrandSymbol => self.runtime_create_private_brand_symbol(args),
            FunctionId::CreatePrivateAccessors => self.runtime_create_private_accessors(args),
            FunctionId::LoadPrivateGetter => self.runtime_load_private_getter(args),
            FunctionId::LoadPrivateSetter => self.runtime_load_private_setter(args),
        }
    }
}
