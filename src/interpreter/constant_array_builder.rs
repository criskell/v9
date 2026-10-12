use crate::interpreter::bytecode_operands::OperandSize;

pub const K_8BIT_CAPACITY: usize = 1 << 8;
pub const K_16BIT_CAPACITY: usize = (1 << 16) - K_8BIT_CAPACITY;
pub const K_32BIT_CAPACITY: usize = (1usize << 32) - K_16BIT_CAPACITY - K_8BIT_CAPACITY;

#[derive(Clone, PartialEq, Debug)]
pub enum ConstantEntry {
    Smi(i32),
    Number(f64),
    String(String),
    Name(String),
    FunctionLiteral(usize),
    ScopeInfo(Vec<String>),
    ClassBoilerplate(Vec<(i32, Option<String>)>),
    ClassFieldsSymbol,
    TheHole,
}

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

    fn available(&self) -> usize {
        self.capacity - self.reserved - self.constants.len()
    }

    fn size(&self) -> usize {
        self.constants.len()
    }

    fn max_index(&self) -> usize {
        self.start_index + self.capacity - 1
    }

    fn reserve(&mut self) {
        debug_assert!(self.available() > 0);
        self.reserved += 1;
    }

    fn unreserve(&mut self) {
        debug_assert!(self.reserved > 0);
        self.reserved -= 1;
    }

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

pub struct ConstantArrayBuilder {
    slices: [ConstantArraySlice; 3],
    dedup: std::collections::HashMap<DedupKey, usize>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum DedupKey {
    ClassFieldsSymbol,
    Smi(i32),
    Number(u64),
    String(String),
    Name(String),
    FunctionLiteral(usize),
    ScopeInfo(Vec<String>),
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

    pub fn size(&self) -> usize {
        for slice in self.slices.iter().rev() {
            if slice.size() > 0 {
                return slice.start_index + slice.size();
            }
        }
        0
    }

    pub fn at(&self, index: usize) -> ConstantEntry {
        for slice in &self.slices {
            if slice.contains_index(index) {
                return slice.at(index).clone();
            }
        }
        ConstantEntry::TheHole
    }

    fn slice_for(&mut self, operand_size: OperandSize) -> &mut ConstantArraySlice {
        self.slices
            .iter_mut()
            .find(|slice| slice.operand_size == operand_size)
            .expect("every operand size has a slice")
    }

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

    pub fn insert_smi(&mut self, value: i32) -> usize {
        self.insert_deduped(DedupKey::Smi(value), ConstantEntry::Smi(value))
    }

    pub fn insert_number(&mut self, value: f64) -> usize {
        self.insert_deduped(
            DedupKey::Number(value.to_bits()),
            ConstantEntry::Number(value),
        )
    }

    pub fn insert_string(&mut self, value: &str) -> usize {
        self.insert_deduped(
            DedupKey::String(value.to_owned()),
            ConstantEntry::String(value.to_owned()),
        )
    }

    pub fn insert_name(&mut self, value: &str) -> usize {
        self.insert_deduped(
            DedupKey::Name(value.to_owned()),
            ConstantEntry::Name(value.to_owned()),
        )
    }

    pub fn insert_class_fields_symbol(&mut self) -> usize {
        self.insert_deduped(DedupKey::ClassFieldsSymbol, ConstantEntry::ClassFieldsSymbol)
    }

    pub fn insert_class_boilerplate(&mut self, members: &[(i32, Option<String>)]) -> usize {
        self.allocate_index(ConstantEntry::ClassBoilerplate(members.to_vec()))
    }

    pub fn insert_scope_info(&mut self, names: &[String]) -> usize {
        self.insert_deduped(
            DedupKey::ScopeInfo(names.to_vec()),
            ConstantEntry::ScopeInfo(names.to_vec()),
        )
    }

    pub fn insert_function_literal(&mut self, code_index: usize) -> usize {
        self.insert_deduped(
            DedupKey::FunctionLiteral(code_index),
            ConstantEntry::FunctionLiteral(code_index),
        )
    }

    pub fn create_reserved_entry(&mut self, minimum_operand_size: OperandSize) -> OperandSize {
        for slice in self.slices.iter_mut() {
            if slice.available() > 0 && slice.operand_size >= minimum_operand_size {
                slice.reserve();
                return slice.operand_size;
            }
        }
        panic!("no room left in the constant pool for a reservation");
    }

    pub fn discard_reserved_entry(&mut self, operand_size: OperandSize) {
        self.slice_for(operand_size).unreserve();
    }

    pub fn commit_reserved_entry(&mut self, operand_size: OperandSize, value: i32) -> usize {
        self.discard_reserved_entry(operand_size);
        let index = self.insert_smi(value);
        debug_assert!(index <= self.slice_for(operand_size).max_index());
        index
    }

    pub fn entries(&self) -> Vec<ConstantEntry> {
        (0..self.size()).map(|index| self.at(index)).collect()
    }

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
        assert_eq!(
            builder.insert_smi(1_000_000),
            K_8BIT_CAPACITY,
            "the next index should begin the 16-bit slice"
        );
    }

    #[test]
    fn a_reservation_reports_the_width_of_the_index_it_guarantees() {
        let mut builder = ConstantArrayBuilder::new();
        assert_eq!(
            builder.create_reserved_entry(OperandSize::Byte),
            OperandSize::Byte
        );
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
        assert_eq!(builder.size(), 0);
    }

    #[test]
    fn committing_a_reservation_yields_an_index_that_fits_the_reserved_width() {
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
        let mut builder = ConstantArrayBuilder::new();
        let size = builder.create_reserved_entry(OperandSize::Byte);
        assert_eq!(size, OperandSize::Byte);

        for i in 0..(K_8BIT_CAPACITY - 1) {
            builder.insert_smi(i as i32);
        }
        assert_eq!(
            builder.insert_smi(999_999),
            K_8BIT_CAPACITY,
            "an ordinary entry must spill to the next slice"
        );
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
        let mut sizes = Vec::new();
        for _ in 0..(K_8BIT_CAPACITY - 1) {
            sizes.push(builder.create_reserved_entry(OperandSize::Byte));
        }
        let spilled = builder.insert_name("second");
        assert_eq!(spilled, K_8BIT_CAPACITY);

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
