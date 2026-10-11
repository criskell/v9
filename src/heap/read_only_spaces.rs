//! Ported from `src/heap/read-only-spaces.h` / `read-only-spaces.cc`.
//!
//! Immortal, immovable objects: the maps, the oddballs, the shared empty
//! arrays, and (in `vx`) the internalized strings.
//!
//! Immovability is not a convenience here, it is what several other
//! simplifications rest on. A map that never moves means an inline cache can
//! hold one without the collector having to find and update it, and means a
//! `DescriptorArray`'s keys never need rewriting. See `PORTING.md`.
//!
//! Deviation: V8 can serialise this space into a snapshot and map it
//! read-only at startup, which is most of why it exists as a separate space.
//! `vx` builds it at startup every time.

use crate::heap::spaces::{AllocationSpace, Space};
use crate::objects::tagged::Address;

/// `ReadOnlySpace`
#[derive(Clone, Debug)]
pub struct ReadOnlySpace {
    space: Space,
}

impl ReadOnlySpace {
    pub fn new(start: Address, capacity: usize) -> ReadOnlySpace {
        ReadOnlySpace {
            space: Space::new(AllocationSpace::ReadOnlySpace, start, capacity),
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
}
