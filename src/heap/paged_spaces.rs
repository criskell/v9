//! Ported from `src/heap/paged-spaces.h` / `paged-spaces.cc`.
//!
//! Old space: where objects go once they have survived the nursery, and
//! where anything allocated tenured goes directly.
//!
//! Deviation: V8 grows a paged space by committing another `MemoryChunk` and
//! linking it in. `vx` has no pages, so growth raises the space's limit and
//! the heap extends its arena to match -- which works only because an
//! address here is an offset, so extending the arena cannot invalidate one.

use crate::heap::spaces::{AllocationSpace, Space};
use crate::objects::tagged::Address;

/// `PagedSpace`
#[derive(Clone, Debug)]
pub struct PagedSpace {
    space: Space,
    /// `PagedSpace::max_capacity_`: growth stops here rather than consuming
    /// the machine.
    max_capacity: usize,
}

impl PagedSpace {
    pub fn new(
        id: AllocationSpace,
        start: Address,
        initial_capacity: usize,
        max_capacity: usize,
    ) -> PagedSpace {
        PagedSpace {
            space: Space::new(id, start, initial_capacity),
            max_capacity,
        }
    }

    #[inline]
    pub fn id(&self) -> AllocationSpace {
        self.space.id()
    }
    #[inline]
    pub fn start(&self) -> Address {
        self.space.start()
    }
    #[inline]
    pub fn top(&self) -> Address {
        self.space.top()
    }
    #[inline]
    pub fn limit(&self) -> Address {
        self.space.limit()
    }
    #[inline]
    pub fn capacity(&self) -> usize {
        self.space.capacity()
    }
    #[inline]
    pub fn max_capacity(&self) -> usize {
        self.max_capacity
    }
    #[inline]
    pub fn size(&self) -> usize {
        self.space.size()
    }
    #[inline]
    pub fn available(&self) -> usize {
        self.space.available()
    }
    #[inline]
    pub fn contains(&self, address: Address) -> bool {
        self.space.contains(address)
    }
    #[inline]
    pub fn contains_allocated(&self, address: Address) -> bool {
        self.space.contains_allocated(address)
    }

    /// `PagedSpace::AllocateRaw`, without the growth step. `None` means the
    /// current capacity is exhausted; [`PagedSpace::capacity_needed_to_grow`]
    /// says how much more is wanted.
    #[inline]
    pub fn allocate_raw(&mut self, size_in_bytes: usize) -> Option<Address> {
        self.space.allocate_raw(size_in_bytes)
    }

    /// How large the space would have to become to satisfy `size_in_bytes`,
    /// or `None` if that is past the maximum.
    ///
    /// Doubling, rather than growing by exactly what was asked for, so that a
    /// loop of allocations does not extend the arena on every iteration.
    pub fn capacity_needed_to_grow(&self, size_in_bytes: usize) -> Option<usize> {
        let needed = crate::common::globals::object_pointer_align(size_in_bytes as u64) as usize;
        let current = self.capacity();
        let grow_by = std::cmp::max(current, needed).next_power_of_two();
        let new_capacity = current.saturating_add(grow_by);
        if new_capacity > self.max_capacity {
            None
        } else {
            Some(new_capacity)
        }
    }

    /// `PagedSpace::Grow`: raise the limit. The heap is responsible for
    /// having extended the arena first.
    pub fn grow_to(&mut self, new_capacity: usize) {
        debug_assert!(new_capacity >= self.capacity());
        debug_assert!(new_capacity <= self.max_capacity);
        self.space
            .set_limit(self.space.start() + new_capacity as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn old_space() -> PagedSpace {
        PagedSpace::new(AllocationSpace::OldSpace, 0x1000, 64, 1024)
    }

    #[test]
    fn allocation_is_a_bump_until_the_capacity_runs_out() {
        let mut space = old_space();
        assert!(space.allocate_raw(64).is_some());
        assert_eq!(space.available(), 0);
        assert!(space.allocate_raw(8).is_none());
    }

    #[test]
    fn growth_doubles_rather_than_fitting_exactly() {
        let space = old_space();
        assert_eq!(space.capacity(), 64);
        assert_eq!(space.capacity_needed_to_grow(8), Some(128));
    }

    #[test]
    fn growth_stops_at_the_maximum() {
        let mut space = old_space();
        let mut allocated = 0;
        loop {
            if space.allocate_raw(64).is_some() {
                allocated += 64;
                continue;
            }
            match space.capacity_needed_to_grow(64) {
                Some(capacity) => space.grow_to(capacity),
                None => break,
            }
        }
        assert!(allocated > 64, "it grew");
        assert!(space.capacity() <= space.max_capacity());
    }
}
