//! Ported from `src/execution/isolate.h` (the parts `vx` has reached).
//!
//! An `Isolate` is one independent JavaScript world: its own heap, its own
//! roots, its own string table. V8's is enormous because it also owns the
//! compiler tiers, the debugger, the stack and the embedder plumbing; `vx`'s
//! grows as those arrive.
//!
//! It exists mostly to solve a borrow problem honestly. Allocation can move
//! objects, so anything live across an allocation has to be reachable from
//! something the collector traces. Putting the heap, the roots and the string
//! table in one struct means the collector can be a method on it and see all
//! of them at once -- no juggling of `&mut` borrows, and no way to forget a
//! root set.

use crate::heap::heap::Heap;
use crate::objects::descriptor_array::DescriptorArray;
use crate::objects::fixed_array::{FixedArray, PropertyArray};
use crate::objects::string_table::StringTable;
use crate::objects::tagged::Tagged;

pub struct Isolate {
    pub heap: Heap,
    pub roots: RootsTable,
    /// `Isolate::string_table_`: one canonical copy of each distinct name.
    /// See [`crate::objects::string_table`].
    pub(crate) string_table: StringTable,
    /// `HandleScope`, flattened. Values parked here are traced as roots, so
    /// they survive an allocation that triggers a collection. Code that holds
    /// more than one object across an allocation puts them here.
    pub handles: Vec<Tagged>,

    /// Scavenger bookkeeping: how far the collector has scanned the objects
    /// it promoted into old space during the collection in progress.
    ///
    /// Promotion appends to old space, and a promoted object's own references
    /// still have to be traced, so the collector needs a scan pointer there
    /// as well as in to-space. Parked on the isolate rather than passed
    /// around because the two drain loops feed each other.
    pub(crate) promoted_scan: crate::objects::tagged::Address,
}

pub use crate::roots::roots::RootsTable;

impl Isolate {
    /// `Isolate::New` followed by `Isolate::Init`: builds the heap and
    /// bootstraps the root set.
    pub fn new() -> Isolate {
        let mut isolate = Isolate {
            heap: Heap::new(),
            roots: RootsTable::default(),
            string_table: StringTable::new(),
            handles: Vec::new(),
            promoted_scan: 0,
        };
        isolate.setup_read_only_roots();
        isolate
    }

    pub fn with_heap(heap: Heap) -> Isolate {
        let mut isolate = Isolate {
            heap,
            roots: RootsTable::default(),
            string_table: StringTable::new(),
            handles: Vec::new(),
            promoted_scan: 0,
        };
        isolate.setup_read_only_roots();
        isolate
    }

    // The bootstrap lives in `heap::setup_heap_internal`, the string table in
    // `objects::string_table`, and object creation in `heap::factory` -- the
    // files V8 keeps them in.

    // -- roots, as typed accessors -----------------------------------------

    #[inline]
    pub fn undefined_value(&self) -> Tagged {
        self.roots.undefined_value
    }

    #[inline]
    pub fn null_value(&self) -> Tagged {
        self.roots.null_value
    }

    #[inline]
    pub fn true_value(&self) -> Tagged {
        self.roots.true_value
    }

    #[inline]
    pub fn false_value(&self) -> Tagged {
        self.roots.false_value
    }

    #[inline]
    pub fn the_hole_value(&self) -> Tagged {
        self.roots.the_hole_value
    }

    #[inline]
    pub fn boolean_value(&self, value: bool) -> Tagged {
        if value {
            self.roots.true_value
        } else {
            self.roots.false_value
        }
    }

    #[inline]
    pub fn empty_fixed_array(&self) -> FixedArray {
        FixedArray::unchecked_cast(
            self.roots
                .empty_fixed_array
                .to_strong_heap_object()
                .unwrap(),
        )
    }

    #[inline]
    pub fn empty_property_array(&self) -> PropertyArray {
        PropertyArray::unchecked_cast(
            self.roots
                .empty_property_array
                .to_strong_heap_object()
                .unwrap(),
        )
    }

    #[inline]
    pub fn empty_descriptor_array(&self) -> DescriptorArray {
        DescriptorArray::unchecked_cast(
            self.roots
                .empty_descriptor_array
                .to_strong_heap_object()
                .unwrap(),
        )
    }

    // -- the string table ---------------------------------------------------
}

impl Default for Isolate {
    fn default() -> Isolate {
        Isolate::new()
    }
}
