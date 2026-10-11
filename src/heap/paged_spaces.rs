use crate::heap::spaces::{AllocationSpace, Space};
use crate::objects::tagged::Address;

#[derive(Clone, Debug)]
pub struct PagedSpace {
    space: Space,
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

    pub fn set_top(&mut self, top: Address) {
        self.space.set_top(top);
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

    #[inline]
    pub fn allocate_raw(&mut self, size_in_bytes: usize) -> Option<Address> {
        self.space.allocate_raw(size_in_bytes)
    }

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
