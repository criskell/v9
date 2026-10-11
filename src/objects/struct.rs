use crate::heap::heap::Heap;
use crate::objects::tagged::{Address, Tagged};

object_type!(
    /// `AccessorPair`
    AccessorPair
);

impl AccessorPair {
    pub const K_GETTER_OFFSET: i32 = crate::common::globals::K_TAGGED_SIZE;
    pub const K_SETTER_OFFSET: i32 = Self::K_GETTER_OFFSET + crate::common::globals::K_TAGGED_SIZE;
    pub const K_SIZE: i32 = Self::K_SETTER_OFFSET + crate::common::globals::K_TAGGED_SIZE;

    /// `AccessorPair::getter`
    #[inline]
    pub fn getter(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_GETTER_OFFSET))
    }

    /// `AccessorPair::set_getter`
    #[inline]
    pub fn set_getter(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_GETTER_OFFSET), value);
    }

    #[inline]
    pub fn getter_address(self) -> Address {
        self.field(Self::K_GETTER_OFFSET)
    }

    /// `AccessorPair::setter`
    #[inline]
    pub fn setter(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_SETTER_OFFSET))
    }

    /// `AccessorPair::set_setter`
    #[inline]
    pub fn set_setter(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_SETTER_OFFSET), value);
    }

    #[inline]
    pub fn setter_address(self) -> Address {
        self.field(Self::K_SETTER_OFFSET)
    }

    /// `AccessorPair::SetComponents`
    pub fn set_components(self, heap: &mut Heap, getter: Tagged, setter: Tagged) {
        if !heap.is_null(getter) {
            self.set_getter(heap, getter);
        }
        if !heap.is_null(setter) {
            self.set_setter(heap, setter);
        }
    }

    /// `AccessorPair::Equals`
    pub fn equals(self, heap: &Heap, getter_value: Tagged, setter_value: Tagged) -> bool {
        self.getter(heap) == getter_value && self.setter(heap) == setter_value
    }
}
