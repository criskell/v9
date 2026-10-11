use crate::common::globals::K_TAGGED_SIZE;
use crate::heap::heap::Heap;
use crate::objects::smi::Smi;
use crate::objects::tagged::{Address, Tagged};

object_type!(
    /// `Context`
    Context
);

impl Context {
    pub const K_LENGTH_OFFSET: i32 = K_TAGGED_SIZE;
    pub const K_HEADER_SIZE: i32 = Self::K_LENGTH_OFFSET + K_TAGGED_SIZE;

    /// `Context::SCOPE_INFO_INDEX`
    pub const SCOPE_INFO_INDEX: i32 = 0;
    pub const PREVIOUS_INDEX: i32 = 1;
    pub const MIN_CONTEXT_SLOTS: i32 = 2;

    pub fn size_for(length: i32) -> i32 {
        Self::K_HEADER_SIZE + length * K_TAGGED_SIZE
    }

    #[inline]
    pub fn length(self, heap: &Heap) -> i32 {
        Smi::to_int(heap.read_tagged(self.field(Self::K_LENGTH_OFFSET)))
    }

    #[inline]
    pub fn set_length(self, heap: &mut Heap, length: i32) {
        heap.write_tagged(self.field(Self::K_LENGTH_OFFSET), Smi::from_int(length));
    }

    #[inline]
    pub fn slot_address(self, index: i32) -> Address {
        self.field(Self::K_HEADER_SIZE + index * K_TAGGED_SIZE)
    }

    /// `Context::get`
    #[inline]
    pub fn get(self, heap: &Heap, index: i32) -> Tagged {
        debug_assert!(
            index >= 0 && index < self.length(heap),
            "context slot {index} out of range"
        );
        heap.read_tagged(self.slot_address(index))
    }

    /// `Context::set`
    #[inline]
    pub fn set(self, heap: &mut Heap, index: i32, value: Tagged) {
        debug_assert!(index >= 0 && index < self.length(heap));
        heap.write_tagged(self.slot_address(index), value);
    }

    #[inline]
    pub fn scope_info(self, heap: &Heap) -> Tagged {
        self.get(heap, Self::SCOPE_INFO_INDEX)
    }

    #[inline]
    pub fn set_scope_info(self, heap: &mut Heap, value: Tagged) {
        self.set(heap, Self::SCOPE_INFO_INDEX, value);
    }

    /// `Context::previous`
    #[inline]
    pub fn previous(self, heap: &Heap) -> Tagged {
        self.get(heap, Self::PREVIOUS_INDEX)
    }

    #[inline]
    pub fn set_previous(self, heap: &mut Heap, value: Tagged) {
        self.set(heap, Self::PREVIOUS_INDEX, value);
    }

    pub fn at_depth(self, heap: &Heap, depth: i32) -> Context {
        let mut context = self;
        for _ in 0..depth {
            let previous = context.previous(heap);
            context = Context::unchecked_cast(
                previous
                    .to_strong_heap_object()
                    .expect("walked past the outermost context"),
            );
        }
        context
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::isolate::Isolate;

    #[test]
    fn layout_and_slot_indices_match_v8() {
        assert_eq!(Context::SCOPE_INFO_INDEX, 0);
        assert_eq!(Context::PREVIOUS_INDEX, 1);
        assert_eq!(Context::MIN_CONTEXT_SLOTS, 2);
        assert_eq!(Context::K_HEADER_SIZE, 16);
        assert_eq!(Context::size_for(2), 32);
        assert_eq!(Context::size_for(3), 40);
    }

    #[test]
    fn a_context_holds_its_variables_after_the_header() {
        let mut isolate = Isolate::new();
        let context = isolate.new_function_context(1, Tagged::ZERO);
        assert_eq!(
            context.length(&isolate.heap),
            Context::MIN_CONTEXT_SLOTS + 1
        );

        let slot = Context::MIN_CONTEXT_SLOTS;
        context.set(&mut isolate.heap, slot, Smi::from_int(7));
        assert_eq!(Smi::to_int(context.get(&isolate.heap, slot)), 7);
    }

    #[test]
    fn previous_makes_the_chain_walkable() {
        let mut isolate = Isolate::new();
        let outer = isolate.new_function_context(1, Tagged::ZERO);
        let middle = isolate.new_function_context(1, outer.tagged());
        let inner = isolate.new_function_context(1, middle.tagged());

        assert_eq!(inner.at_depth(&isolate.heap, 0), inner);
        assert_eq!(inner.at_depth(&isolate.heap, 1), middle);
        assert_eq!(inner.at_depth(&isolate.heap, 2), outer);
    }

    #[test]
    fn a_fresh_context_slot_holds_undefined_not_garbage() {
        let mut isolate = Isolate::new();
        let context = isolate.new_function_context(2, Tagged::ZERO);
        let undefined = isolate.undefined_value();
        for slot in Context::MIN_CONTEXT_SLOTS..context.length(&isolate.heap) {
            assert_eq!(
                context.get(&isolate.heap, slot),
                undefined,
                "slot {slot} must be traceable before it is assigned"
            );
        }
    }
}
