//! Ported from `src/heap/spaces.h`.
//!
//! V8 reserves virtual memory per space and hands out pages from it. `vx`
//! reserves one contiguous arena up front and gives each space a disjoint
//! sub-range of it, so an address still identifies its space by value alone.
//! Within a space, allocation is the same bump pointer V8 uses: `top` walks
//! up towards `limit`, and running out is what triggers a collection.
//!
//! `Space` is the base class every space derives from; the derived spaces
//! live in the files V8 puts them in -- [`SemiSpace`] and
//! [`SemiSpaceNewSpace`](crate::heap::new_spaces::SemiSpaceNewSpace) in
//! `new_spaces`, [`PagedSpace`](crate::heap::paged_spaces::PagedSpace) in
//! `paged_spaces`, [`ReadOnlySpace`](crate::heap::read_only_spaces::ReadOnlySpace)
//! in `read_only_spaces`.
//!
//! Deviation: there are no pages. A space is one run of bytes, not a list of
//! `MemoryChunk`s. Pages buy V8 incremental sweeping, per-page remembered
//! sets and growable spaces without reservation games; none of that is needed
//! until `vx` has a mark-compact collector, so it is deferred rather than
//! faked. See `PORTING.md`.

use crate::common::globals::*;
use crate::objects::tagged::Address;

/// `AllocationSpace` from `src/common/globals.h`, restricted to the spaces
/// `vx` actually has.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum AllocationSpace {
    /// `RO_SPACE`: immortal, immovable objects -- maps, oddballs, the empty
    /// arrays. Nothing here ever moves, which is why the GC can skip it as a
    /// relocation target and why a map pointer stays valid across a scavenge.
    ReadOnlySpace,
    /// `NEW_SPACE`, to-space half: where every young object is born.
    NewSpace,
    /// `NEW_SPACE`, from-space half: idle between scavenges.
    FromSpace,
    /// `OLD_SPACE`: survivors, and anything allocated tenured.
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

/// One space's bump-allocation region inside the arena.
///
/// This is V8's `Space` reduced to what a bump allocator needs: the bounds of
/// the reservation and how far into it allocation has got.
#[derive(Clone, Debug)]
pub struct Space {
    id: AllocationSpace,
    /// First address of the reservation.
    start: Address,
    /// Next address to allocate at. `start <= top <= limit`.
    top: Address,
    /// One past the last usable address.
    limit: Address,
    /// Bytes handed out since the space was last reset. Tracked separately
    /// from `top - start` so that a reset can keep the running total for
    /// `--trace-gc`-style reporting.
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

    /// `Space::Capacity`
    #[inline]
    pub fn capacity(&self) -> usize {
        (self.limit - self.start) as usize
    }

    /// `Space::Size`: bytes currently occupied by objects.
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

    /// `Space::Contains`
    #[inline]
    pub fn contains(&self, address: Address) -> bool {
        address >= self.start && address < self.limit
    }

    /// Whether `address` points inside the *occupied* part of the space, i.e.
    /// at something that has actually been allocated.
    #[inline]
    pub fn contains_allocated(&self, address: Address) -> bool {
        address >= self.start && address < self.top
    }

    /// Bump allocation. `None` means the space is full, which is the caller's
    /// cue to collect (`Heap::AllocateRaw` retries after a GC).
    ///
    /// The returned address is `kObjectAlignment`-aligned because `start` is
    /// aligned and every allocation is rounded up.
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

    /// `SemiSpace::Reset`: drop everything in the space at once.
    ///
    /// Only sound for a space whose live contents have already been evacuated
    /// -- which, for a semi-space after a scavenge, they have.
    pub fn reset(&mut self) {
        self.top = self.start;
        self.allocated_since_reset = 0;
    }

    /// Exchange the role this space plays, used when the semi-spaces swap.
    /// The address range is unaffected -- only the label moves.
    pub(crate) fn set_id_for_swap(&mut self, id: AllocationSpace) {
        debug_assert!(matches!(
            (self.id, id),
            (AllocationSpace::NewSpace, AllocationSpace::FromSpace)
                | (AllocationSpace::FromSpace, AllocationSpace::NewSpace)
        ));
        self.id = id;
    }

    /// Raise the limit, the way V8 grows a paged space by adding pages. The
    /// caller owns making sure the arena actually reaches that far.
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
