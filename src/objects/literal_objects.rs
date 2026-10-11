use crate::heap::heap::Heap;
use crate::objects::fixed_array::FixedArray;
use crate::objects::smi::Smi;
use crate::objects::tagged::Tagged;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ValueKind {
    ConstantKind,
    GetterKind,
    SetterKind,
}

impl ValueKind {
    fn to_int(self) -> i32 {
        match self {
            ValueKind::ConstantKind => 0,
            ValueKind::GetterKind => 1,
            ValueKind::SetterKind => 2,
        }
    }

    fn from_int(value: i32) -> ValueKind {
        match value {
            0 => ValueKind::ConstantKind,
            1 => ValueKind::GetterKind,
            _ => ValueKind::SetterKind,
        }
    }
}

object_type!(
    ClassBoilerplate
);

impl ClassBoilerplate {
    pub const K_ARGUMENTS_COUNT_INDEX: i32 = 0;
    pub const K_FIRST_MEMBER_INDEX: i32 = 1;
    pub const K_MEMBER_SIZE: i32 = 2;
    pub const K_MEMBER_FLAGS_OFFSET: i32 = 0;
    pub const K_MEMBER_KEY_OFFSET: i32 = 1;

    pub const K_CONSTRUCTOR_ARGUMENT_INDEX: i32 = 1;
    pub const K_PROTOTYPE_ARGUMENT_INDEX: i32 = 2;
    pub const K_FIRST_DYNAMIC_ARGUMENT_INDEX: i32 = 3;

    fn as_fixed_array(self) -> FixedArray {
        FixedArray::unchecked_cast(self.object())
    }

    pub fn arguments_count(self, heap: &Heap) -> i32 {
        Smi::to_int(
            self.as_fixed_array()
                .get(heap, Self::K_ARGUMENTS_COUNT_INDEX),
        )
    }

    pub fn number_of_members(self, heap: &Heap) -> i32 {
        (self.as_fixed_array().length(heap) - Self::K_FIRST_MEMBER_INDEX) / Self::K_MEMBER_SIZE
    }

    pub fn key_at(self, heap: &Heap, member: i32) -> Tagged {
        self.as_fixed_array().get(
            heap,
            Self::K_FIRST_MEMBER_INDEX + member * Self::K_MEMBER_SIZE + Self::K_MEMBER_KEY_OFFSET,
        )
    }

    pub fn value_kind_at(self, heap: &Heap, member: i32) -> ValueKind {
        ValueKind::from_int(self.flags_at(heap, member) >> 1)
    }

    pub fn is_static_at(self, heap: &Heap, member: i32) -> bool {
        self.flags_at(heap, member) & 1 != 0
    }

    fn flags_at(self, heap: &Heap, member: i32) -> i32 {
        Smi::to_int(self.as_fixed_array().get(
            heap,
            Self::K_FIRST_MEMBER_INDEX + member * Self::K_MEMBER_SIZE + Self::K_MEMBER_FLAGS_OFFSET,
        ))
    }

    pub fn encode_flags(kind: ValueKind, is_static: bool) -> i32 {
        (kind.to_int() << 1) | is_static as i32
    }
}
