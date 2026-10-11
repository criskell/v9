use crate::objects::tagged::Address;

pub type CellType = u32;

// Bitmap is a sequence of cells each containing fixed number of bits.
pub struct MarkingBitmap {
    cells: Vec<CellType>,
    base: Address,
}

impl MarkingBitmap {
    pub const K_BITS_PER_CELL: u32 = 32;
    pub const K_BITS_PER_CELL_LOG2: u32 = 5;
    pub const K_BIT_INDEX_MASK: u32 = Self::K_BITS_PER_CELL - 1;

    pub fn new(base: Address, size_in_bytes: usize) -> MarkingBitmap {
        let length = size_in_bytes >> crate::common::globals::K_TAGGED_SIZE_LOG2;
        let cells_count =
            (length + Self::K_BITS_PER_CELL as usize - 1) >> Self::K_BITS_PER_CELL_LOG2;
        MarkingBitmap {
            cells: vec![0; cells_count],
            base,
        }
    }

    /// `MarkingBitmap::AddressToIndex`
    #[inline]
    fn address_to_index(&self, address: Address) -> usize {
        debug_assert!(address >= self.base);
        ((address - self.base) >> crate::common::globals::K_TAGGED_SIZE_LOG2) as usize
    }

    /// `MarkingBitmap::IndexToAddressOffset`
    #[inline]
    fn index_to_address(&self, index: usize) -> Address {
        self.base + ((index as Address) << crate::common::globals::K_TAGGED_SIZE_LOG2)
    }

    /// `MarkingBitmap::IndexToCell`
    #[inline]
    fn index_to_cell(index: usize) -> usize {
        index >> Self::K_BITS_PER_CELL_LOG2
    }

    /// `MarkingBitmap::IndexInCellMask`
    #[inline]
    fn index_in_cell_mask(index: usize) -> CellType {
        1 << (index as u32 & Self::K_BIT_INDEX_MASK)
    }

    #[inline]
    fn covers(&self, address: Address) -> bool {
        address >= self.base
            && Self::index_to_cell(self.address_to_index(address)) < self.cells.len()
    }

    #[inline]
    pub fn set(&mut self, address: Address) -> bool {
        if !self.covers(address) {
            return false;
        }
        let index = self.address_to_index(address);
        let mask = Self::index_in_cell_mask(index);
        let cell = &mut self.cells[Self::index_to_cell(index)];
        let old_value = *cell;
        if (old_value & mask) == mask {
            return false;
        }
        *cell = old_value | mask;
        true
    }

    /// `MarkBit::Get`
    #[inline]
    pub fn get(&self, address: Address) -> bool {
        if !self.covers(address) {
            return false;
        }
        let index = self.address_to_index(address);
        let mask = Self::index_in_cell_mask(index);
        (self.cells[Self::index_to_cell(index)] & mask) != 0
    }

    /// `MarkingBitmap::Clear`
    pub fn clear(&mut self) {
        self.cells.fill(0);
    }

    /// `MarkingBitmap::IsClean`
    pub fn is_clean(&self) -> bool {
        self.cells.iter().all(|cell| *cell == 0)
    }

    pub fn marked_addresses(&self) -> Vec<Address> {
        let mut marked = Vec::new();
        for (cell_index, cell) in self.cells.iter().enumerate() {
            let mut bits = *cell;
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                marked
                    .push(self.index_to_address((cell_index << Self::K_BITS_PER_CELL_LOG2) + bit));
            }
        }
        marked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bit_stands_for_one_tagged_word() {
        let mut bitmap = MarkingBitmap::new(0x1000, 1024);
        assert!(bitmap.set(0x1000));
        assert!(!bitmap.set(0x1000), "setting twice reports no change");
        assert!(bitmap.get(0x1000));
        assert!(!bitmap.get(0x1008));
        assert!(bitmap.set(0x1008));
        assert_eq!(bitmap.marked_addresses(), vec![0x1000, 0x1008]);
    }

    #[test]
    fn marks_survive_crossing_a_cell_boundary() {
        let mut bitmap = MarkingBitmap::new(0, 4096);
        let past_first_cell = 32 * 8;
        bitmap.set(past_first_cell);
        bitmap.set(past_first_cell + 8 * 40);
        assert!(bitmap.get(past_first_cell));
        assert!(bitmap.get(past_first_cell + 8 * 40));
        assert_eq!(
            bitmap.marked_addresses(),
            vec![past_first_cell, past_first_cell + 8 * 40]
        );
    }

    #[test]
    fn clearing_resets_everything() {
        let mut bitmap = MarkingBitmap::new(0, 1024);
        for word in 0..128 {
            bitmap.set(word * 8);
        }
        assert!(!bitmap.is_clean());
        bitmap.clear();
        assert!(bitmap.is_clean());
        assert!(bitmap.marked_addresses().is_empty());
    }

    #[test]
    fn an_address_outside_the_range_is_never_marked() {
        let mut bitmap = MarkingBitmap::new(0x1000, 64);
        assert!(!bitmap.set(0x2000), "out of range");
        assert!(!bitmap.get(0x2000));
    }
}
