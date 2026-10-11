use crate::heap::heap::Heap;
use crate::objects::descriptor_array::DescriptorArray;
use crate::objects::fixed_array::{FixedArray, PropertyArray};
use crate::objects::string_table::StringTable;
use crate::objects::tagged::Tagged;

pub struct Isolate {
    pub heap: Heap,
    pub roots: RootsTable,
    pub(crate) string_table: StringTable,
    pub handles: Vec<Tagged>,

    pub(crate) promoted_scan: crate::objects::tagged::Address,

    pub stack: Vec<Tagged>,

    pub(crate) call_depth: usize,

    pub(crate) shared_function_infos: Vec<Tagged>,

    pub(crate) output: Vec<String>,

    pub(crate) ic_statistics: crate::ic::ic::ICStatistics,

    pub(crate) stub_cache: crate::ic::stub_cache::StubCache,

    /// `Heap::allocator_`
    pub(crate) heap_allocator: crate::heap::heap_allocator::HeapAllocator,

    pub trace_ic_enabled: bool,
    pub(crate) ic_trace: Vec<String>,

    pub(crate) script_function: Tagged,
}

pub use crate::roots::roots::RootsTable;

impl Isolate {
    pub fn new() -> Isolate {
        let mut isolate = Isolate {
            heap: Heap::new(),
            roots: RootsTable::default(),
            string_table: StringTable::new(),
            handles: Vec::new(),
            promoted_scan: 0,
            stack: Vec::new(),
            call_depth: 0,
            shared_function_infos: Vec::new(),
            output: Vec::new(),
            ic_statistics: Default::default(),
            stub_cache: crate::ic::stub_cache::StubCache::new(),
            heap_allocator: crate::heap::heap_allocator::HeapAllocator::new(),
            trace_ic_enabled: false,
            ic_trace: Vec::new(),
            script_function: Tagged::ZERO,
        };
        isolate.enter_always_allocate_scope();
        isolate.setup_read_only_roots();
        isolate.leave_always_allocate_scope();
        isolate
    }

    pub fn with_heap(heap: Heap) -> Isolate {
        let mut isolate = Isolate {
            heap,
            roots: RootsTable::default(),
            string_table: StringTable::new(),
            handles: Vec::new(),
            promoted_scan: 0,
            stack: Vec::new(),
            call_depth: 0,
            shared_function_infos: Vec::new(),
            output: Vec::new(),
            ic_statistics: Default::default(),
            stub_cache: crate::ic::stub_cache::StubCache::new(),
            heap_allocator: crate::heap::heap_allocator::HeapAllocator::new(),
            trace_ic_enabled: false,
            ic_trace: Vec::new(),
            script_function: Tagged::ZERO,
        };
        isolate.enter_always_allocate_scope();
        isolate.setup_read_only_roots();
        isolate.leave_always_allocate_scope();
        isolate
    }

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
