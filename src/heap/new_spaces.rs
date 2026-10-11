use crate::heap::spaces::{AllocationSpace, Space};
use crate::objects::tagged::Address;

#[derive(Clone, Debug)]
pub struct SemiSpace {
    space: Space,
}

impl SemiSpace {
    pub fn new(id: AllocationSpace, start: Address, capacity: usize) -> SemiSpace {
        debug_assert!(matches!(
            id,
            AllocationSpace::NewSpace | AllocationSpace::FromSpace
        ));
        SemiSpace {
            space: Space::new(id, start, capacity),
        }
    }

    pub fn swap(from: &mut SemiSpace, to: &mut SemiSpace) {
        std::mem::swap(&mut from.space, &mut to.space);
        from.space.set_id_for_swap(AllocationSpace::FromSpace);
        to.space.set_id_for_swap(AllocationSpace::NewSpace);
    }

    #[inline]
    pub fn space(&self) -> &Space {
        &self.space
    }

    #[inline]
    pub fn space_mut(&mut self) -> &mut Space {
        &mut self.space
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
    /// `SemiSpace::Reset`
    #[inline]
    pub fn reset(&mut self) {
        self.space.reset();
    }
}

/// `SemiSpaceNewSpace`
pub struct SemiSpaceNewSpace {
    to_space: SemiSpace,
    from_space: SemiSpace,
    age_mark: Address,
}

impl SemiSpaceNewSpace {
    pub fn new(
        to_start: Address,
        from_start: Address,
        semi_space_size: usize,
    ) -> SemiSpaceNewSpace {
        SemiSpaceNewSpace {
            to_space: SemiSpace::new(AllocationSpace::NewSpace, to_start, semi_space_size),
            from_space: SemiSpace::new(AllocationSpace::FromSpace, from_start, semi_space_size),
            age_mark: to_start,
        }
    }

    /// `SemiSpaceNewSpace::SwapSemiSpaces`
    pub fn swap_semi_spaces(&mut self) {
        SemiSpace::swap(&mut self.from_space, &mut self.to_space);
    }

    /// `SemiSpaceNewSpace::age_mark`
    #[inline]
    pub fn age_mark(&self) -> Address {
        self.age_mark
    }

    /// `SemiSpaceNewSpace::set_age_mark`
    #[inline]
    pub fn set_age_mark(&mut self, address: Address) {
        self.age_mark = address;
    }

    #[inline]
    pub fn to_space(&self) -> &SemiSpace {
        &self.to_space
    }

    #[inline]
    pub fn to_space_mut(&mut self) -> &mut SemiSpace {
        &mut self.to_space
    }

    #[inline]
    pub fn from_space(&self) -> &SemiSpace {
        &self.from_space
    }

    #[inline]
    pub fn from_space_mut(&mut self) -> &mut SemiSpace {
        &mut self.from_space
    }

    #[inline]
    pub fn allocate_raw(&mut self, size_in_bytes: usize) -> Option<Address> {
        self.to_space.allocate_raw(size_in_bytes)
    }

    #[inline]
    pub fn start(&self) -> Address {
        self.to_space.start()
    }
    #[inline]
    pub fn top(&self) -> Address {
        self.to_space.top()
    }
    /// `NewSpace::Size`
    #[inline]
    pub fn size(&self) -> usize {
        self.to_space.size()
    }
    #[inline]
    pub fn capacity(&self) -> usize {
        self.to_space.capacity()
    }
    #[inline]
    pub fn available(&self) -> usize {
        self.to_space.available()
    }

    #[inline]
    pub fn contains(&self, address: Address) -> bool {
        self.to_space.contains(address) || self.from_space.contains(address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_space() -> SemiSpaceNewSpace {
        SemiSpaceNewSpace::new(0x1000, 0x2000, 256)
    }

    #[test]
    fn allocation_goes_into_to_space() {
        let mut space = new_space();
        let address = space.allocate_raw(16).unwrap();
        assert!(space.to_space().contains_allocated(address));
        assert!(!space.from_space().contains(address));
        assert_eq!(space.size(), 16);
        assert_eq!(space.from_space().size(), 0);
    }

    #[test]
    fn swapping_exchanges_roles_not_addresses() {
        let mut space = new_space();
        let to_start = space.to_space().start();
        let from_start = space.from_space().start();
        let address = space.allocate_raw(16).unwrap();

        space.swap_semi_spaces();

        assert_eq!(space.to_space().start(), from_start);
        assert_eq!(space.from_space().start(), to_start);
        assert_eq!(space.to_space().id(), AllocationSpace::NewSpace);
        assert_eq!(space.from_space().id(), AllocationSpace::FromSpace);
        assert!(space.from_space().contains(address));
        assert!(space.contains(address), "still young either way");
    }

    #[test]
    fn both_halves_are_the_nursery() {
        let space = new_space();
        assert!(space.contains(space.to_space().start()));
        assert!(space.contains(space.from_space().start()));
        assert!(!space.contains(0x9999));
    }

    #[test]
    fn the_age_mark_separates_survivors_from_fresh_allocations() {
        let mut space = new_space();
        let survivor = space.allocate_raw(32).unwrap();
        space.set_age_mark(space.top());
        let fresh = space.allocate_raw(16).unwrap();

        assert!(survivor < space.age_mark(), "below the mark: has survived");
        assert!(fresh >= space.age_mark(), "above it: allocated since");
    }
}
