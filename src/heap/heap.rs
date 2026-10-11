use crate::common::globals::*;
use crate::heap::new_spaces::SemiSpaceNewSpace;
use crate::heap::paged_spaces::PagedSpace;
use crate::heap::read_only_spaces::ReadOnlySpace;
use crate::heap::spaces::AllocationSpace;
use crate::objects::map_word::MapWord;
use crate::objects::tagged::{Address, HeapObject, Tagged};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum AllocationType {
    Young,
    Old,
    Map,
    ReadOnly,
}

#[derive(Copy, Clone, Debug)]
pub struct HeapConfig {
    pub read_only_space_size: usize,
    pub semi_space_size: usize,
    pub initial_old_space_size: usize,
    pub max_old_space_size: usize,
}

impl Default for HeapConfig {
    fn default() -> HeapConfig {
        HeapConfig {
            read_only_space_size: 512 * K_KB,
            semi_space_size: 512 * K_KB,
            initial_old_space_size: 4 * K_MB,
            max_old_space_size: 256 * K_MB,
        }
    }
}

pub struct Heap {
    memory: Vec<u8>,
    config: HeapConfig,

    read_only_space: ReadOnlySpace,
    new_space: SemiSpaceNewSpace,
    /// `Heap::old_space_`
    old_space: PagedSpace,

    pub gc_count: usize,
    pub bytes_promoted: usize,
    pub bytes_evacuated: usize,

    /// `ReadOnlyRoots::the_hole_value`
    pub(crate) the_hole_value: Tagged,
    /// `ReadOnlyRoots::undefined_value`
    pub(crate) undefined_value: Tagged,
}

impl Heap {
    pub fn new() -> Heap {
        Heap::with_config(HeapConfig::default())
    }

    pub fn with_config(config: HeapConfig) -> Heap {
        let guard = K_OBJECT_ALIGNMENT as usize;

        let read_only_start = guard as Address;
        let to_space_start = read_only_start + config.read_only_space_size as u64;
        let from_space_start = to_space_start + config.semi_space_size as u64;
        let old_space_start = from_space_start + config.semi_space_size as u64;

        let total = old_space_start as usize + config.initial_old_space_size;

        Heap {
            memory: vec![0u8; total],
            config,
            read_only_space: ReadOnlySpace::new(read_only_start, config.read_only_space_size),
            new_space: SemiSpaceNewSpace::new(
                to_space_start,
                from_space_start,
                config.semi_space_size,
            ),
            old_space: PagedSpace::new(
                AllocationSpace::OldSpace,
                old_space_start,
                config.initial_old_space_size,
                config.max_old_space_size,
            ),
            gc_count: 0,
            bytes_promoted: 0,
            bytes_evacuated: 0,
            the_hole_value: Tagged::ZERO,
            undefined_value: Tagged::ZERO,
        }
    }

    pub fn config(&self) -> &HeapConfig {
        &self.config
    }

    // -- Spaces -------------------------------------------------------------

    pub fn read_only_space(&self) -> &ReadOnlySpace {
        &self.read_only_space
    }

    /// `Heap::new_space`
    pub fn new_space(&self) -> &SemiSpaceNewSpace {
        &self.new_space
    }

    /// `Heap::old_space`
    pub fn old_space(&self) -> &PagedSpace {
        &self.old_space
    }

    pub(crate) fn new_space_mut(&mut self) -> &mut SemiSpaceNewSpace {
        &mut self.new_space
    }

    pub fn space_of(&self, address: Address) -> Option<AllocationSpace> {
        if self.read_only_space.contains(address) {
            return Some(AllocationSpace::ReadOnlySpace);
        }
        if self.new_space.to_space().contains(address) {
            return Some(self.new_space.to_space().id());
        }
        if self.new_space.from_space().contains(address) {
            return Some(self.new_space.from_space().id());
        }
        if self.old_space.contains(address) {
            return Some(AllocationSpace::OldSpace);
        }
        None
    }

    #[inline]
    pub fn in_young_generation(&self, object: HeapObject) -> bool {
        self.new_space.contains(object.address())
    }

    #[inline]
    pub fn in_from_space(&self, object: HeapObject) -> bool {
        self.new_space.from_space().contains(object.address())
    }

    #[inline]
    pub fn in_read_only_space(&self, object: HeapObject) -> bool {
        self.read_only_space.contains(object.address())
    }

    // -- Allocation ---------------------------------------------------------

    pub fn allocate_raw(
        &mut self,
        size_in_bytes: usize,
        allocation: AllocationType,
    ) -> Option<HeapObject> {
        debug_assert!(size_in_bytes > 0);
        let address = match allocation {
            AllocationType::Young => self.new_space.allocate_raw(size_in_bytes),
            AllocationType::Old => self.allocate_in_old_space(size_in_bytes),
            AllocationType::Map | AllocationType::ReadOnly => {
                self.read_only_space.allocate_raw(size_in_bytes)
            }
        }?;
        Some(HeapObject::from_address(address))
    }

    fn allocate_in_old_space(&mut self, size_in_bytes: usize) -> Option<Address> {
        if let Some(address) = self.old_space.allocate_raw(size_in_bytes) {
            return Some(address);
        }
        let new_capacity = self.old_space.capacity_needed_to_grow(size_in_bytes)?;
        let new_limit = self.old_space.start() + new_capacity as u64;
        self.memory.resize(new_limit as usize, 0);
        self.old_space.grow_to(new_capacity);
        self.old_space.allocate_raw(size_in_bytes)
    }

    pub fn allocate_raw_or_fail(
        &mut self,
        size_in_bytes: usize,
        allocation: AllocationType,
    ) -> HeapObject {
        match self.allocate_raw(size_in_bytes, allocation) {
            Some(object) => object,
            None => panic!(
                "out of memory allocating {size_in_bytes} bytes as {allocation:?} \
                 (read_only {}/{}, new {}/{}, old {}/{})",
                self.read_only_space.size(),
                self.read_only_space.capacity(),
                self.new_space.size(),
                self.new_space.capacity(),
                self.old_space.size(),
                self.old_space.capacity(),
            ),
        }
    }

    #[inline]
    fn index(&self, address: Address, width: usize) -> usize {
        let index = address as usize;
        assert!(
            address != 0 && index + width <= self.memory.len(),
            "heap access out of bounds: address 0x{address:x} (+{width} bytes), \
             arena is {} bytes",
            self.memory.len()
        );
        index
    }

    #[inline]
    fn index_aligned(&self, address: Address, width: usize) -> usize {
        debug_assert!(
            is_aligned(address, width as u64),
            "misaligned heap access: address 0x{address:x} is not {width}-byte aligned"
        );
        self.index(address, width)
    }

    #[inline]
    /// `ReadOnlyRoots::undefined_value`
    pub fn is_undefined(&self, value: Tagged) -> bool {
        self.undefined_value != Tagged::ZERO && value == self.undefined_value
    }

    /// `ReadOnlyRoots::the_hole_value`
    pub fn is_the_hole(&self, value: Tagged) -> bool {
        self.the_hole_value != Tagged::ZERO && value == self.the_hole_value
    }

    pub fn read_u64(&self, address: Address) -> u64 {
        let i = self.index_aligned(address, 8);
        u64::from_ne_bytes(self.memory[i..i + 8].try_into().unwrap())
    }

    #[inline]
    pub fn write_u64(&mut self, address: Address, value: u64) {
        let i = self.index_aligned(address, 8);
        self.memory[i..i + 8].copy_from_slice(&value.to_ne_bytes());
    }

    #[inline]
    pub fn read_tagged(&self, address: Address) -> Tagged {
        Tagged(self.read_u64(address))
    }

    #[inline]
    pub fn write_tagged(&mut self, address: Address, value: Tagged) {
        self.write_u64(address, value.ptr());
    }

    #[inline]
    pub fn read_u8(&self, address: Address) -> u8 {
        self.memory[self.index(address, 1)]
    }

    #[inline]
    pub fn write_u8(&mut self, address: Address, value: u8) {
        let i = self.index(address, 1);
        self.memory[i] = value;
    }

    #[inline]
    pub fn read_u16(&self, address: Address) -> u16 {
        let i = self.index_aligned(address, 2);
        u16::from_ne_bytes(self.memory[i..i + 2].try_into().unwrap())
    }

    #[inline]
    pub fn write_u16(&mut self, address: Address, value: u16) {
        let i = self.index_aligned(address, 2);
        self.memory[i..i + 2].copy_from_slice(&value.to_ne_bytes());
    }

    #[inline]
    pub fn read_u32(&self, address: Address) -> u32 {
        let i = self.index_aligned(address, 4);
        u32::from_ne_bytes(self.memory[i..i + 4].try_into().unwrap())
    }

    #[inline]
    pub fn write_u32(&mut self, address: Address, value: u32) {
        let i = self.index_aligned(address, 4);
        self.memory[i..i + 4].copy_from_slice(&value.to_ne_bytes());
    }

    #[inline]
    pub fn read_f64(&self, address: Address) -> f64 {
        f64::from_bits(self.read_u64(address))
    }

    #[inline]
    pub fn write_f64(&mut self, address: Address, value: f64) {
        self.write_u64(address, value.to_bits());
    }

    #[inline]
    pub fn read_bytes(&self, address: Address, length: usize) -> &[u8] {
        let i = self.index(address, length);
        &self.memory[i..i + length]
    }

    #[inline]
    pub fn write_bytes(&mut self, address: Address, bytes: &[u8]) {
        let i = self.index(address, bytes.len());
        self.memory[i..i + bytes.len()].copy_from_slice(bytes);
    }

    pub fn set_old_space_top(&mut self, top: Address) {
        self.old_space.set_top(top);
    }

    pub fn copy_within(&mut self, from: Address, to: Address, length: usize) {
        let src = self.index(from, length);
        let dst = self.index(to, length);
        self.memory.copy_within(src..src + length, dst);
    }

    pub fn zero_range(&mut self, address: Address, length: usize) {
        let i = self.index(address, length);
        self.memory[i..i + length].fill(0);
    }

    // -- The map word -------------------------------------------------------

    /// `HeapObject::map_word`
    #[inline]
    pub fn map_word(&self, object: HeapObject) -> MapWord {
        MapWord::from_raw(self.read_u64(object.address()))
    }

    /// `HeapObject::set_map_word`
    #[inline]
    pub fn set_map_word(&mut self, object: HeapObject, map_word: MapWord) {
        self.write_u64(object.address(), map_word.ptr());
    }

    #[inline]
    pub fn map(&self, object: HeapObject) -> HeapObject {
        let word = self.map_word(object);
        assert!(
            !word.is_forwarding_address(),
            "read the map of an evacuated object at 0x{:x}; follow the \
             forwarding address first",
            object.address()
        );
        word.to_map()
    }

    /// `HeapObject::set_map`
    #[inline]
    pub fn set_map(&mut self, object: HeapObject, map: HeapObject) {
        self.set_map_word(object, MapWord::from_map(map));
    }

    pub fn stats(&self) -> HeapStats {
        HeapStats {
            read_only_size: self.read_only_space.size(),
            new_space_size: self.new_space.size(),
            new_space_capacity: self.new_space.capacity(),
            old_space_size: self.old_space.size(),
            old_space_capacity: self.old_space.capacity(),
            arena_size: self.memory.len(),
            gc_count: self.gc_count,
            bytes_promoted: self.bytes_promoted,
            bytes_evacuated: self.bytes_evacuated,
        }
    }
}

impl Default for Heap {
    fn default() -> Heap {
        Heap::new()
    }
}

#[derive(Copy, Clone, Debug)]
pub struct HeapStats {
    pub read_only_size: usize,
    pub new_space_size: usize,
    pub new_space_capacity: usize,
    pub old_space_size: usize,
    pub old_space_capacity: usize,
    pub arena_size: usize,
    pub gc_count: usize,
    pub bytes_promoted: usize,
    pub bytes_evacuated: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::smi::Smi;

    fn small_heap() -> Heap {
        Heap::with_config(HeapConfig {
            read_only_space_size: 1024,
            semi_space_size: 1024,
            initial_old_space_size: 1024,
            max_old_space_size: 64 * K_KB,
        })
    }

    #[test]
    fn address_zero_is_never_allocated() {
        let mut heap = small_heap();
        let object = heap.allocate_raw_or_fail(8, AllocationType::ReadOnly);
        assert_ne!(object.address(), 0, "kNullAddress must stay invalid");
        assert!(heap.space_of(0).is_none());
    }

    #[test]
    fn spaces_do_not_overlap() {
        let heap = small_heap();
        let ranges = [
            (
                "read_only",
                heap.read_only_space().start(),
                heap.read_only_space().limit(),
            ),
            (
                "to_space",
                heap.new_space().to_space().start(),
                heap.new_space().to_space().limit(),
            ),
            (
                "from_space",
                heap.new_space().from_space().start(),
                heap.new_space().from_space().limit(),
            ),
            ("old", heap.old_space().start(), heap.old_space().limit()),
        ];
        for (i, (a_name, a_start, a_limit)) in ranges.iter().enumerate() {
            for (b_name, b_start, b_limit) in ranges.iter().skip(i + 1) {
                assert!(
                    a_limit <= b_start || b_limit <= a_start,
                    "{a_name} and {b_name} overlap"
                );
            }
        }
    }

    #[test]
    fn allocation_type_picks_the_right_space() {
        let mut heap = small_heap();
        let young = heap.allocate_raw_or_fail(16, AllocationType::Young);
        let old = heap.allocate_raw_or_fail(16, AllocationType::Old);
        let map = heap.allocate_raw_or_fail(16, AllocationType::Map);

        assert_eq!(
            heap.space_of(young.address()),
            Some(AllocationSpace::NewSpace)
        );
        assert_eq!(
            heap.space_of(old.address()),
            Some(AllocationSpace::OldSpace)
        );
        assert_eq!(
            heap.space_of(map.address()),
            Some(AllocationSpace::ReadOnlySpace),
            "maps must be immovable"
        );

        assert!(heap.in_young_generation(young));
        assert!(!heap.in_young_generation(old));
        assert!(!heap.in_young_generation(map));
        assert!(heap.in_read_only_space(map));
    }

    #[test]
    fn a_full_new_space_reports_failure_instead_of_spilling_elsewhere() {
        let mut heap = small_heap();
        let mut count = 0;
        while heap.allocate_raw(16, AllocationType::Young).is_some() {
            count += 1;
            assert!(count < 1000, "new space should have filled up by now");
        }
        assert_eq!(count, 1024 / 16);
        assert!(heap.allocate_raw(16, AllocationType::Old).is_some());
    }

    #[test]
    fn old_space_grows_and_offsets_stay_valid_across_the_reallocation() {
        let mut heap = small_heap();
        let first = heap.allocate_raw_or_fail(16, AllocationType::Old);
        heap.write_tagged(first.address(), Smi::from_int(0x1234));

        let mut last = first;
        for _ in 0..512 {
            last = heap.allocate_raw_or_fail(16, AllocationType::Old);
        }
        assert!(
            heap.old_space().capacity() > 1024,
            "old space should have grown"
        );

        assert_eq!(Smi::to_int(heap.read_tagged(first.address())), 0x1234);
        assert_ne!(first, last);
        assert_eq!(
            heap.space_of(last.address()),
            Some(AllocationSpace::OldSpace)
        );
    }

    #[test]
    fn old_space_refuses_to_grow_past_its_maximum() {
        let mut heap = Heap::with_config(HeapConfig {
            read_only_space_size: 1024,
            semi_space_size: 1024,
            initial_old_space_size: 1024,
            max_old_space_size: 2048,
        });
        let mut allocated = 0;
        while heap.allocate_raw(64, AllocationType::Old).is_some() {
            allocated += 64;
            assert!(allocated <= 2048, "grew past max_old_space_size");
        }
        assert!(allocated > 0);
    }

    #[test]
    fn fields_round_trip_at_every_width() {
        let mut heap = small_heap();
        let object = heap.allocate_raw_or_fail(32, AllocationType::Old);
        let a = object.address();

        heap.write_u64(a, 0xdead_beef_cafe_f00d);
        assert_eq!(heap.read_u64(a), 0xdead_beef_cafe_f00d);

        heap.write_u32(a + 8, 0xfeed_face);
        assert_eq!(heap.read_u32(a + 8), 0xfeed_face);

        heap.write_u16(a + 12, 0xbeef);
        assert_eq!(heap.read_u16(a + 12), 0xbeef);

        heap.write_u8(a + 14, 0x42);
        assert_eq!(heap.read_u8(a + 14), 0x42);

        heap.write_tagged(a + 16, Smi::from_int(-7));
        assert_eq!(Smi::to_int(heap.read_tagged(a + 16)), -7);

        heap.write_f64(a + 24, 1.5);
        assert_eq!(heap.read_f64(a + 24), 1.5);
    }

    #[test]
    fn nan_payloads_survive_the_heap() {
        let mut heap = small_heap();
        let object = heap.allocate_raw_or_fail(8, AllocationType::Old);
        let signalling = f64::from_bits(0x7ff0_0000_0000_0001);
        heap.write_f64(object.address(), signalling);
        assert_eq!(
            heap.read_f64(object.address()).to_bits(),
            signalling.to_bits()
        );
    }

    #[test]
    fn fresh_memory_reads_back_as_smi_zero() {
        let mut heap = small_heap();
        let object = heap.allocate_raw_or_fail(32, AllocationType::Old);
        for offset in (0..32).step_by(8) {
            let value = heap.read_tagged(object.address() + offset);
            assert!(value.is_smi(), "fresh memory must be GC-safe");
            assert_eq!(Smi::to_int(value), 0);
        }
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn reading_outside_the_arena_panics_instead_of_corrupting() {
        let heap = small_heap();
        heap.read_u64(0xffff_fff8);
    }

    #[test]
    #[should_panic(expected = "misaligned")]
    fn a_misaligned_tagged_access_is_caught() {
        let mut heap = small_heap();
        let object = heap.allocate_raw_or_fail(16, AllocationType::Old);
        heap.read_u64(object.address() + 1);
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn reading_address_zero_panics() {
        let heap = small_heap();
        heap.read_u64(0);
    }

    #[test]
    fn the_map_word_holds_a_map_and_can_be_forwarded() {
        let mut heap = small_heap();
        let map = heap.allocate_raw_or_fail(16, AllocationType::Map);
        let object = heap.allocate_raw_or_fail(16, AllocationType::Young);
        heap.set_map(object, map);
        assert_eq!(heap.map(object), map);
        assert!(!heap.map_word(object).is_forwarding_address());

        let copy = heap.allocate_raw_or_fail(16, AllocationType::Old);
        heap.set_map_word(object, MapWord::from_forwarding_address(copy));
        let word = heap.map_word(object);
        assert!(word.is_forwarding_address());
        assert_eq!(word.to_forwarding_address(), copy);
    }

    #[test]
    #[should_panic(expected = "evacuated")]
    fn reading_the_map_of_an_evacuated_object_panics() {
        let mut heap = small_heap();
        let object = heap.allocate_raw_or_fail(16, AllocationType::Young);
        let copy = heap.allocate_raw_or_fail(16, AllocationType::Old);
        heap.set_map_word(object, MapWord::from_forwarding_address(copy));
        heap.map(object);
    }

    #[test]
    fn swapping_semi_spaces_moves_an_object_into_the_space_being_evacuated() {
        let mut heap = small_heap();
        let young = heap.allocate_raw_or_fail(16, AllocationType::Young);
        assert!(heap.in_young_generation(young));
        assert!(!heap.in_from_space(young));

        heap.new_space_mut().swap_semi_spaces();

        assert!(heap.in_from_space(young));
        assert!(
            heap.in_young_generation(young),
            "either half is the nursery"
        );
        assert_eq!(
            heap.space_of(young.address()),
            Some(AllocationSpace::FromSpace)
        );
    }

    #[test]
    fn copy_within_moves_an_object_byte_for_byte() {
        let mut heap = small_heap();
        let src = heap.allocate_raw_or_fail(24, AllocationType::Young);
        let dst = heap.allocate_raw_or_fail(24, AllocationType::Old);
        for i in 0..3u64 {
            heap.write_tagged(src.address() + i * 8, Smi::from_int(i as i32 + 1));
        }
        heap.copy_within(src.address(), dst.address(), 24);
        for i in 0..3u64 {
            assert_eq!(
                Smi::to_int(heap.read_tagged(dst.address() + i * 8)),
                i as i32 + 1
            );
        }
    }
}
