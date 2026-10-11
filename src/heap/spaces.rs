use crate::common::globals::*;
use crate::objects::tagged::Address;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum AllocationSpace {
    ReadOnlySpace,
    NewSpace,
    FromSpace,
    OldSpace,
}

impl AllocationSpace {
    pub fn name(self) -> &'static str {
        match self {
            AllocationSpace::ReadOnlySpace => "read_only_space",
            AllocationSpace::NewSpace => "new_space",
            AllocationSpace::FromSpace => "from_space",
            AllocationSpace::OldSpace => "old_space",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Space {
    id: AllocationSpace,
    start: Address,
    top: Address,
    limit: Address,
    allocated_since_reset: usize,
}

impl Space {
    pub fn new(id: AllocationSpace, start: Address, capacity: usize) -> Space {
        debug_assert!(is_aligned(start, K_OBJECT_ALIGNMENT));
        debug_assert!(is_aligned(capacity as u64, K_OBJECT_ALIGNMENT));
        Space {
            id,
            start,
            top: start,
            limit: start + capacity as u64,
            allocated_since_reset: 0,
        }
    }

    #[inline]
    pub fn id(&self) -> AllocationSpace {
        self.id
    }

    #[inline]
    pub fn start(&self) -> Address {
        self.start
    }

    #[inline]
    pub fn top(&self) -> Address {
        self.top
    }

    #[inline]
    pub fn limit(&self) -> Address {
        self.limit
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        (self.limit - self.start) as usize
    }

    #[inline]
    pub fn size(&self) -> usize {
        (self.top - self.start) as usize
    }

    #[inline]
    pub fn available(&self) -> usize {
        (self.limit - self.top) as usize
    }

    pub fn allocated_since_reset(&self) -> usize {
        self.allocated_since_reset
    }

    #[inline]
    pub fn contains(&self, address: Address) -> bool {
        address >= self.start && address < self.limit
    }

    #[inline]
    pub fn contains_allocated(&self, address: Address) -> bool {
        address >= self.start && address < self.top
    }

    #[inline]
    pub fn allocate_raw(&mut self, size_in_bytes: usize) -> Option<Address> {
        let size = object_pointer_align(size_in_bytes as u64);
        if self.top + size > self.limit {
            return None;
        }
        let result = self.top;
        self.top += size;
        self.allocated_since_reset += size as usize;
        Some(result)
    }

    pub fn set_top(&mut self, top: Address) {
        debug_assert!(top >= self.start() && top <= self.limit());
        self.top = top;
    }

    pub fn reset(&mut self) {
        self.top = self.start;
        self.allocated_since_reset = 0;
    }

    pub(crate) fn set_id_for_swap(&mut self, id: AllocationSpace) {
        debug_assert!(matches!(
            (self.id, id),
            (AllocationSpace::NewSpace, AllocationSpace::FromSpace)
                | (AllocationSpace::FromSpace, AllocationSpace::NewSpace)
        ));
        self.id = id;
    }

    pub fn set_limit(&mut self, limit: Address) {
        debug_assert!(limit >= self.top);
        self.limit = limit;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn space() -> Space {
        Space::new(AllocationSpace::NewSpace, 0x1000, 128)
    }

    #[test]
    fn bump_allocation_is_aligned_and_sequential() {
        let mut s = space();
        let a = s.allocate_raw(8).unwrap();
        let b = s.allocate_raw(8).unwrap();
        assert_eq!(a, 0x1000);
        assert_eq!(b, 0x1008);
        assert_eq!(s.size(), 16);
    }

    #[test]
    fn odd_sizes_are_rounded_up_to_object_alignment() {
        let mut s = space();
        let a = s.allocate_raw(1).unwrap();
        let b = s.allocate_raw(9).unwrap();
        let c = s.allocate_raw(8).unwrap();
        assert_eq!(a, 0x1000);
        assert_eq!(b, 0x1008, "a 1-byte request still consumes a full word");
        assert_eq!(c, 0x1018, "a 9-byte request consumes two words");
        for addr in [a, b, c] {
            assert!(is_aligned(addr, K_OBJECT_ALIGNMENT));
        }
    }

    #[test]
    fn a_full_space_refuses_rather_than_overflowing() {
        let mut s = space();
        assert!(s.allocate_raw(128).is_some());
        assert_eq!(s.available(), 0);
        assert!(
            s.allocate_raw(8).is_none(),
            "must not hand out past the limit"
        );
        assert!(s.contains_allocated(0x1000));
        assert!(!s.contains(0x1080), "the limit is exclusive");
    }

    #[test]
    fn allocating_more_than_the_whole_space_fails_without_moving_top() {
        let mut s = space();
        assert!(s.allocate_raw(129).is_none());
        assert_eq!(s.size(), 0, "a failed allocation must not consume space");
    }

    #[test]
    fn reset_makes_the_space_reusable() {
        let mut s = space();
        s.allocate_raw(64).unwrap();
        s.reset();
        assert_eq!(s.size(), 0);
        assert_eq!(s.allocate_raw(8), Some(0x1000), "reset rewinds to start");
    }

    #[test]
    fn contains_distinguishes_reserved_from_occupied() {
        let mut s = space();
        s.allocate_raw(8).unwrap();
        assert!(s.contains(0x1040), "still inside the reservation");
        assert!(
            !s.contains_allocated(0x1040),
            "but nothing is allocated there"
        );
    }
}
