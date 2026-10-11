//! Ported from `src/interpreter/constant-array-builder.h` /
//! `constant-array-builder.cc`.
//!
//! The constant pool of a bytecode array: the literals, property names and
//! nested function infos that bytecodes refer to by index.
//!
//! Three slices, not one
//! --------------------
//! The pool is split by *index width*:
//!
//! ```text
//!   slice 0   indices      0 ..    255    reachable with a 1-byte operand
//!   slice 1   indices    256 ..  65535    needs a 2-byte operand
//!   slice 2   indices  65536 ..           needs a 4-byte operand
//! ```
//!
//! That is not an optimisation for the common case -- it is what makes
//! forward jumps encodable. A forward jump has to be emitted before its
//! distance is known, so the width of its operand must be decided in
//! advance. [`ConstantArrayBuilder::create_reserved_entry`] reserves a slot
//! in the narrowest slice that has room and reports that slice's operand
//! size. The writer emits the jump that wide. If the real offset later fits,
//! the reservation is discarded and the offset is written in place
//! ([`Self::discard_reserved_entry`]); if it does not, the reservation is
//! committed ([`Self::commit_reserved_entry`]) and the index handed back is
//! *guaranteed* to fit in the width already emitted, because it comes from
//! that slice.
//!
//! Entries are deferred
//! --------------------
//! V8's `Entry` can hold a Smi, a raw AST string, a deferred handle and so
//! on, materialised into a `FixedArray` only at the end. `vx` does the same
//! with [`ConstantEntry`], and for the same reason: the builder runs during
//! bytecode generation, and allocating a heap object per literal while
//! generating would mean a GC safepoint in the middle of a half-built
//! function.

use crate::interpreter::bytecode_operands::OperandSize;

/// `ConstantArrayBuilder::k8BitCapacity`
pub const K_8BIT_CAPACITY: usize = 1 << 8;
/// `ConstantArrayBuilder::k16BitCapacity`
pub const K_16BIT_CAPACITY: usize = (1 << 16) - K_8BIT_CAPACITY;
/// `ConstantArrayBuilder::k32BitCapacity`
pub const K_32BIT_CAPACITY: usize = (1usize << 32) - K_16BIT_CAPACITY - K_8BIT_CAPACITY;

/// A constant pool entry, not yet materialised in the heap.
///
/// `ConstantArrayBuilder::Entry` in V8.
#[derive(Clone, PartialEq, Debug)]
pub enum ConstantEntry {
    /// A Smi, including a committed jump offset.
    Smi(i32),
    /// A number that does not fit in a Smi; becomes a `HeapNumber`.
    Number(f64),
    /// A string literal; becomes a `SeqString`.
    String(String),
    /// A name used as a property key or a global; becomes an *internalized*
    /// string, so that lookups compare it by address.
    Name(String),
    /// A nested function literal, by index into the isolate's code store.
    FunctionLiteral(usize),
    /// `the_hole`: a gap left by an unused reservation, or by a slice that
    /// started before the previous one filled up. Never read by a bytecode.
    TheHole,
}

/// `ConstantArrayBuilder::ConstantArraySlice`
#[derive(Debug)]
struct ConstantArraySlice {
    start_index: usize,
    capacity: usize,
    reserved: usize,
    operand_size: OperandSize,
    constants: Vec<ConstantEntry>,
}

impl ConstantArraySlice {
    fn new(start_index: usize, capacity: usize, operand_size: OperandSize) -> ConstantArraySlice {
        ConstantArraySlice {
            start_index,
            capacity,
            reserved: 0,
            operand_size,
            constants: Vec::new(),
        }
    }

    /// `ConstantArraySlice::available`: room for a new entry, counting the
    /// slots already promised to reservations.
    fn available(&self) -> usize {
        self.capacity - self.reserved - self.constants.len()
    }

    fn size(&self) -> usize {
        self.constants.len()
    }

    fn max_index(&self) -> usize {
        self.start_index + self.capacity - 1
    }

    /// `ConstantArraySlice::Reserve`
    fn reserve(&mut self) {
        debug_assert!(self.available() > 0);
        self.reserved += 1;
    }

    /// `ConstantArraySlice::Unreserve`
    fn unreserve(&mut self) {
        debug_assert!(self.reserved > 0);
        self.reserved -= 1;
    }

    /// `ConstantArraySlice::Allocate`
    fn allocate(&mut self, entry: ConstantEntry) -> usize {
        debug_assert!(self.available() > 0);
        let index = self.start_index + self.constants.len();
        self.constants.push(entry);
        index
    }

    fn at(&self, index: usize) -> &ConstantEntry {
        &self.constants[index - self.start_index]
    }

    fn contains_index(&self, index: usize) -> bool {
        index >= self.start_index && index < self.start_index + self.constants.len()
    }
}

/// `ConstantArrayBuilder`
pub struct ConstantArrayBuilder {
    slices: [ConstantArraySlice; 3],
    /// Dedup: the same name or literal used twice should be one entry, which
    /// is what makes a property name in a loop cost one constant.
    dedup: std::collections::HashMap<DedupKey, usize>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum DedupKey {
    Smi(i32),
    /// Keyed by bits so that `NaN` and `-0.0` behave: `-0.0` and `0.0` are
    /// different constants, and two `NaN`s with the same payload are one.
    Number(u64),
    String(String),
    Name(String),
    FunctionLiteral(usize),
}

impl Default for ConstantArrayBuilder {
    fn default() -> ConstantArrayBuilder {
        ConstantArrayBuilder::new()
    }
}

impl ConstantArrayBuilder {
    pub fn new() -> ConstantArrayBuilder {
        ConstantArrayBuilder {
            slices: [
                ConstantArraySlice::new(0, K_8BIT_CAPACITY, OperandSize::Byte),
                ConstantArraySlice::new(K_8BIT_CAPACITY, K_16BIT_CAPACITY, OperandSize::Short),
                ConstantArraySlice::new(
                    K_8BIT_CAPACITY + K_16BIT_CAPACITY,
                    K_32BIT_CAPACITY,
                    OperandSize::Quad,
                ),
            ],
            dedup: std::collections::HashMap::new(),
        }
    }

    /// `ConstantArrayBuilder::size`: the length the materialised array will
    /// have, including the holes left between slices.
    pub fn size(&self) -> usize {
        for slice in self.slices.iter().rev() {
            if slice.size() > 0 {
                return slice.start_index + slice.size();
            }
        }
        0
    }

    /// `ConstantArrayBuilder::At`
    pub fn at(&self, index: usize) -> ConstantEntry {
        for slice in &self.slices {
            if slice.contains_index(index) {
                return slice.at(index).clone();
            }
        }
        // A gap between slices, or an uncommitted reservation.
        ConstantEntry::TheHole
    }

    fn slice_for(&mut self, operand_size: OperandSize) -> &mut ConstantArraySlice {
        self.slices
            .iter_mut()
            .find(|slice| slice.operand_size == operand_size)
            .expect("every operand size has a slice")
    }

    /// `ConstantArrayBuilder::AllocateIndex`: put `entry` in the narrowest
    /// slice that still has room.
    fn allocate_index(&mut self, entry: ConstantEntry) -> usize {
        for slice in self.slices.iter_mut() {
            if slice.available() > 0 {
                return slice.allocate(entry);
            }
        }
        panic!("constant pool is full");
    }

    fn insert_deduped(&mut self, key: DedupKey, entry: ConstantEntry) -> usize {
        if let Some(index) = self.dedup.get(&key) {
            return *index;
        }
        let index = self.allocate_index(entry);
        self.dedup.insert(key, index);
        index
    }

    /// `ConstantArrayBuilder::Insert(Smi)`
    pub fn insert_smi(&mut self, value: i32) -> usize {
        self.insert_deduped(DedupKey::Smi(value), ConstantEntry::Smi(value))
    }

    /// `ConstantArrayBuilder::Insert(double)`
    pub fn insert_number(&mut self, value: f64) -> usize {
        self.insert_deduped(
            DedupKey::Number(value.to_bits()),
            ConstantEntry::Number(value),
        )
    }

    /// `ConstantArrayBuilder::Insert(AstRawString)` for a string literal.
    pub fn insert_string(&mut self, value: &str) -> usize {
        self.insert_deduped(
            DedupKey::String(value.to_owned()),
            ConstantEntry::String(value.to_owned()),
        )
    }

    /// A property name or global name: internalized when materialised.
    pub fn insert_name(&mut self, value: &str) -> usize {
        self.insert_deduped(
            DedupKey::Name(value.to_owned()),
            ConstantEntry::Name(value.to_owned()),
        )
    }

    /// `ConstantArrayBuilder::Insert(FunctionLiteral)`
    pub fn insert_function_literal(&mut self, code_index: usize) -> usize {
        self.insert_deduped(
            DedupKey::FunctionLiteral(code_index),
            ConstantEntry::FunctionLiteral(code_index),
        )
    }

    /// `ConstantArrayBuilder::CreateReservedEntry`.
    ///
    /// Claims a slot without deciding what goes in it, and reports how wide
    /// an index into that slot will be. The caller -- always a forward jump
    /// -- emits its operand that wide.
    pub fn create_reserved_entry(&mut self, minimum_operand_size: OperandSize) -> OperandSize {
        for slice in self.slices.iter_mut() {
            if slice.available() > 0 && slice.operand_size >= minimum_operand_size {
                slice.reserve();
                return slice.operand_size;
            }
        }
        panic!("no room left in the constant pool for a reservation");
    }

    /// `ConstantArrayBuilder::DiscardReservedEntry`: the jump's offset fitted
    /// after all, so the slot is given back.
    pub fn discard_reserved_entry(&mut self, operand_size: OperandSize) {
        self.slice_for(operand_size).unreserve();
    }

    /// `ConstantArrayBuilder::CommitReservedEntry`: the jump's offset did not
    /// fit, so it goes in the pool instead.
    ///
    /// The returned index is guaranteed to fit in `operand_size`, because the
    /// reservation was taken from the slice with that index width. That
    /// guarantee is the whole point of the reservation.
    pub fn commit_reserved_entry(&mut self, operand_size: OperandSize, value: i32) -> usize {
        self.discard_reserved_entry(operand_size);
        let index = self.insert_smi(value);
        debug_assert!(
            index <= self.slice_for(operand_size).max_index(),
            "a committed reservation must fit the operand it was reserved for"
        );
        index
    }

    /// Every entry in index order, holes included. This is what
    /// `ToFixedArray` materialises.
    pub fn entries(&self) -> Vec<ConstantEntry> {
        (0..self.size()).map(|index| self.at(index)).collect()
    }

    /// Reservations still outstanding. Should be zero once every jump is
    /// bound; the builder asserts it before handing over the array.
    pub fn outstanding_reservations(&self) -> usize {
        self.slices.iter().map(|slice| slice.reserved).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_constants_are_shared() {
        let mut builder = ConstantArrayBuilder::new();
        let a = builder.insert_name("length");
        let b = builder.insert_name("length");
        let c = builder.insert_name("size");
        assert_eq!(a, b, "a name used twice should cost one entry");
        assert_ne!(a, c);
        assert_eq!(builder.size(), 2);
    }

    #[test]
    fn names_and_string_literals_are_different_constants() {
        // A name is internalized when materialised and a literal is not, so
        // they cannot share an entry even with the same characters.
        let mut builder = ConstantArrayBuilder::new();
        let name = builder.insert_name("x");
        let literal = builder.insert_string("x");
        assert_ne!(name, literal);
        assert_eq!(builder.at(name), ConstantEntry::Name("x".to_string()));
        assert_eq!(builder.at(literal), ConstantEntry::String("x".to_string()));
    }

    #[test]
    fn numbers_are_deduped_by_bits_so_negative_zero_stays_distinct() {
        let mut builder = ConstantArrayBuilder::new();
        let zero = builder.insert_number(0.0);
        let negative_zero = builder.insert_number(-0.0);
        assert_ne!(zero, negative_zero, "0 and -0 are different values");

        let nan = builder.insert_number(f64::NAN);
        let nan_again = builder.insert_number(f64::NAN);
        assert_eq!(nan, nan_again, "the same NaN bits are one constant");
    }

    #[test]
    fn entries_land_in_the_narrowest_slice_first() {
        let mut builder = ConstantArrayBuilder::new();
        for i in 0..K_8BIT_CAPACITY {
            assert_eq!(builder.insert_smi(i as i32), i, "entry {i}");
        }
        // The byte slice is full, so the next entry starts the short slice.
        assert_eq!(
            builder.insert_smi(1_000_000),
            K_8BIT_CAPACITY,
            "the next index should begin the 16-bit slice"
        );
    }

    #[test]
    fn a_reservation_reports_the_width_of_the_index_it_guarantees() {
        let mut builder = ConstantArrayBuilder::new();
        // Plenty of room in the byte slice, so a byte it is.
        assert_eq!(
            builder.create_reserved_entry(OperandSize::Byte),
            OperandSize::Byte
        );
        // Asking for at least a short gets a short, even though a byte would
        // have been available.
        assert_eq!(
            builder.create_reserved_entry(OperandSize::Short),
            OperandSize::Short
        );
        assert_eq!(builder.outstanding_reservations(), 2);
    }

    #[test]
    fn discarding_a_reservation_gives_the_slot_back() {
        let mut builder = ConstantArrayBuilder::new();
        let size = builder.create_reserved_entry(OperandSize::Byte);
        assert_eq!(builder.outstanding_reservations(), 1);
        builder.discard_reserved_entry(size);
        assert_eq!(builder.outstanding_reservations(), 0);
        // The pool is still empty: a discarded reservation leaves no entry.
        assert_eq!(builder.size(), 0);
    }

    #[test]
    fn committing_a_reservation_yields_an_index_that_fits_the_reserved_width() {
        // The guarantee the whole scheme rests on.
        let mut builder = ConstantArrayBuilder::new();
        let size = builder.create_reserved_entry(OperandSize::Byte);
        let index = builder.commit_reserved_entry(size, 100_000);
        assert!(
            index < K_8BIT_CAPACITY,
            "an index reserved as a byte must fit in a byte, got {index}"
        );
        assert_eq!(builder.at(index), ConstantEntry::Smi(100_000));
        assert_eq!(builder.outstanding_reservations(), 0);
    }

    #[test]
    fn a_byte_reservation_survives_the_slice_filling_up_around_it() {
        // This is the case the reservation exists for: a forward jump
        // reserves a byte-wide slot, a lot of constants are added while the
        // jump is unbound, and the commit must still return a byte index.
        let mut builder = ConstantArrayBuilder::new();
        let size = builder.create_reserved_entry(OperandSize::Byte);
        assert_eq!(size, OperandSize::Byte);

        // Fill the byte slice right up to the reserved slot.
        for i in 0..(K_8BIT_CAPACITY - 1) {
            builder.insert_smi(i as i32);
        }
        // There is no room left for an ordinary entry in the byte slice...
        assert_eq!(
            builder.insert_smi(999_999),
            K_8BIT_CAPACITY,
            "an ordinary entry must spill to the next slice"
        );
        // ...but the reservation is still honoured with a byte index.
        let index = builder.commit_reserved_entry(size, 123);
        assert!(
            index < K_8BIT_CAPACITY,
            "the reserved slot must still be a byte index, got {index}"
        );
    }

    #[test]
    fn gaps_between_slices_materialise_as_holes() {
        let mut builder = ConstantArrayBuilder::new();
        builder.insert_name("first");
        // Force an entry into the 16-bit slice while the 8-bit slice has only
        // one real entry, by reserving the rest of the byte slice.
        let mut sizes = Vec::new();
        for _ in 0..(K_8BIT_CAPACITY - 1) {
            sizes.push(builder.create_reserved_entry(OperandSize::Byte));
        }
        let spilled = builder.insert_name("second");
        assert_eq!(spilled, K_8BIT_CAPACITY);

        // The array is as long as the highest used index, with holes in the
        // slots nothing committed to.
        assert_eq!(builder.size(), K_8BIT_CAPACITY + 1);
        assert_eq!(builder.at(0), ConstantEntry::Name("first".to_string()));
        assert_eq!(builder.at(1), ConstantEntry::TheHole);
        assert_eq!(builder.at(K_8BIT_CAPACITY - 1), ConstantEntry::TheHole);
        assert_eq!(
            builder.at(K_8BIT_CAPACITY),
            ConstantEntry::Name("second".to_string())
        );

        for size in sizes {
            builder.discard_reserved_entry(size);
        }
        assert_eq!(builder.outstanding_reservations(), 0);
    }

    #[test]
    fn the_slice_capacities_tile_the_index_space() {
        // Each slice covers exactly the indices its operand width can name.
        assert_eq!(K_8BIT_CAPACITY, 256);
        assert_eq!(K_8BIT_CAPACITY + K_16BIT_CAPACITY, 65536);
        assert_eq!(
            K_8BIT_CAPACITY + K_16BIT_CAPACITY + K_32BIT_CAPACITY,
            1usize << 32
        );
    }

    #[test]
    fn entries_come_back_in_index_order() {
        let mut builder = ConstantArrayBuilder::new();
        builder.insert_name("a");
        builder.insert_smi(7);
        builder.insert_string("hello");
        assert_eq!(
            builder.entries(),
            vec![
                ConstantEntry::Name("a".to_string()),
                ConstantEntry::Smi(7),
                ConstantEntry::String("hello".to_string()),
            ]
        );
    }
}
