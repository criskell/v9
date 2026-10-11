//! Ported from `src/interpreter/bytecode-register-allocator.h`.
//!
//! A stack of registers. `new_register` bumps a cursor, `release_registers`
//! pops back to a mark. There is no liveness analysis and no reuse beyond
//! that -- the generator allocates a temporary, uses it, and releases it,
//! and the high-water mark becomes the function's frame size.
//!
//! The reason a *stack* is enough: the bytecode generator evaluates
//! expressions in tree order, so the lifetimes of its temporaries nest. A
//! register allocated while evaluating a subexpression is dead before its
//! parent finishes.
//!
//! [`BytecodeRegisterAllocator::new_growable_register_list`] is the exception
//! that proves the rule: a call's argument list has to be contiguous, so the
//! arguments are appended one at a time, and nothing else may allocate in
//! between. V8 enforces that with a `CHECK`; so does this.

use crate::interpreter::bytecode_register::{Register, RegisterList};

/// `BytecodeRegisterAllocator`
#[derive(Debug)]
pub struct BytecodeRegisterAllocator {
    next_register_index: i32,
    /// The highest index ever handed out, which is the frame size the
    /// function needs.
    max_register_count: i32,
}

impl BytecodeRegisterAllocator {
    pub fn new(start_index: i32) -> BytecodeRegisterAllocator {
        BytecodeRegisterAllocator {
            next_register_index: start_index,
            max_register_count: start_index,
        }
    }

    /// `BytecodeRegisterAllocator::NewRegister`
    pub fn new_register(&mut self) -> Register {
        let register = Register::new(self.next_register_index);
        self.next_register_index += 1;
        self.max_register_count = self.max_register_count.max(self.next_register_index);
        register
    }

    /// `BytecodeRegisterAllocator::NewRegisterList`
    pub fn new_register_list(&mut self, count: i32) -> RegisterList {
        let list = RegisterList::new(Register::new(self.next_register_index), count);
        self.next_register_index += count;
        self.max_register_count = self.max_register_count.max(self.next_register_index);
        list
    }

    /// `BytecodeRegisterAllocator::NewGrowableRegisterList`: an empty list
    /// that [`Self::grow_register_list`] appends to.
    pub fn new_growable_register_list(&mut self) -> RegisterList {
        RegisterList::new(Register::new(self.next_register_index), 0)
    }

    /// `BytecodeRegisterAllocator::GrowRegisterList`.
    ///
    /// Panics if anything else allocated since the list was created: the
    /// list has to stay contiguous, because a call passes its arguments as a
    /// base register plus a count.
    pub fn grow_register_list(&mut self, list: &mut RegisterList) -> Register {
        let expected = list.first_register().index() + list.register_count();
        let register = self.new_register();
        assert_eq!(
            register.index(),
            expected,
            "a register was allocated between the creation of this list and \
             growing it, so the list is no longer contiguous"
        );
        *list = RegisterList::new(list.first_register(), list.register_count() + 1);
        register
    }

    /// `BytecodeRegisterAllocator::ReleaseRegisters`: pop back to a mark.
    pub fn release_registers(&mut self, register_index: i32) {
        debug_assert!(register_index <= self.next_register_index);
        self.next_register_index = register_index;
    }

    /// `BytecodeRegisterAllocator::ReleaseRegister`: pop exactly the register
    /// most recently allocated.
    pub fn release_register(&mut self, register: Register) {
        debug_assert_eq!(
            self.next_register_index - 1,
            register.index(),
            "registers are released in reverse order of allocation"
        );
        self.next_register_index -= 1;
    }

    /// `BytecodeRegisterAllocator::next_register_index`: the mark to release
    /// back to.
    #[inline]
    pub fn next_register_index(&self) -> i32 {
        self.next_register_index
    }

    /// `BytecodeRegisterAllocator::maximum_register_count`: the frame size.
    #[inline]
    pub fn maximum_register_count(&self) -> i32 {
        self.max_register_count
    }

    /// `BytecodeRegisterAllocator::RegisterIsLive`
    #[inline]
    pub fn register_is_live(&self, register: Register) -> bool {
        register.index() < self.next_register_index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_come_out_in_order() {
        let mut allocator = BytecodeRegisterAllocator::new(0);
        assert_eq!(allocator.new_register(), Register::new(0));
        assert_eq!(allocator.new_register(), Register::new(1));
        assert_eq!(allocator.new_register(), Register::new(2));
        assert_eq!(allocator.maximum_register_count(), 3);
    }

    #[test]
    fn releasing_rewinds_and_the_registers_are_reused() {
        let mut allocator = BytecodeRegisterAllocator::new(0);
        let first = allocator.new_register();
        let mark = allocator.next_register_index();
        let temporary = allocator.new_register();
        assert!(allocator.register_is_live(temporary));

        allocator.release_registers(mark);
        assert!(!allocator.register_is_live(temporary));
        assert!(allocator.register_is_live(first));
        // The next allocation reuses the slot.
        assert_eq!(allocator.new_register(), temporary);
    }

    #[test]
    fn the_high_water_mark_is_the_frame_size() {
        let mut allocator = BytecodeRegisterAllocator::new(0);
        let mark = allocator.next_register_index();
        allocator.new_register_list(5);
        assert_eq!(allocator.maximum_register_count(), 5);
        allocator.release_registers(mark);
        // Releasing does not shrink the frame: the function still needed
        // room for five at its peak.
        assert_eq!(allocator.maximum_register_count(), 5);
        allocator.new_register();
        assert_eq!(allocator.maximum_register_count(), 5);
    }

    #[test]
    fn a_register_list_is_contiguous() {
        let mut allocator = BytecodeRegisterAllocator::new(0);
        allocator.new_register();
        let list = allocator.new_register_list(3);
        assert_eq!(list.first_register(), Register::new(1));
        assert_eq!(
            list.iter().collect::<Vec<_>>(),
            vec![Register::new(1), Register::new(2), Register::new(3)]
        );
        assert_eq!(allocator.next_register_index(), 4);
    }

    #[test]
    fn a_growable_list_collects_consecutive_registers() {
        // How a call's argument list is built: one register per argument,
        // appended as each argument is evaluated.
        let mut allocator = BytecodeRegisterAllocator::new(0);
        let mut arguments = allocator.new_growable_register_list();
        assert!(arguments.is_empty());

        let receiver = allocator.grow_register_list(&mut arguments);
        let first = allocator.grow_register_list(&mut arguments);
        let second = allocator.grow_register_list(&mut arguments);

        assert_eq!(arguments.register_count(), 3);
        assert_eq!(arguments.get(0), receiver);
        assert_eq!(arguments.get(1), first);
        assert_eq!(arguments.get(2), second);
        assert_eq!(second.index(), receiver.index() + 2);
    }

    #[test]
    #[should_panic(expected = "no longer contiguous")]
    fn growing_a_list_after_an_unrelated_allocation_is_refused() {
        let mut allocator = BytecodeRegisterAllocator::new(0);
        let mut arguments = allocator.new_growable_register_list();
        allocator.grow_register_list(&mut arguments);
        // An unrelated temporary lands in the middle of the list.
        allocator.new_register();
        allocator.grow_register_list(&mut arguments);
    }

    #[test]
    fn releasing_a_single_register_must_be_the_last_one() {
        let mut allocator = BytecodeRegisterAllocator::new(0);
        allocator.new_register();
        let last = allocator.new_register();
        allocator.release_register(last);
        assert_eq!(allocator.next_register_index(), 1);
    }

    #[test]
    fn allocation_can_start_above_zero() {
        // The generator reserves the first registers for the function's
        // declared locals before allocating temporaries.
        let mut allocator = BytecodeRegisterAllocator::new(3);
        assert_eq!(allocator.new_register(), Register::new(3));
        assert_eq!(allocator.maximum_register_count(), 4);
    }
}
