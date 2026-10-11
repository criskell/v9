use crate::interpreter::bytecode_register::Register;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CatchPrediction {
    // The handler will (likely) rethrow the exception.
    Uncaught,
    // The exception will be caught by the handler.
    Caught,
}

pub const K_NO_HANDLER_FOUND: i32 = -1;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HandlerTable {
    entries: Vec<HandlerTableEntry>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct HandlerTableEntry {
    pub offset_start: i32,
    pub offset_end: i32,
    pub offset_target: i32,
    pub context: Register,
    pub prediction: CatchPrediction,
}

impl HandlerTable {
    pub const K_RANGE_START_INDEX: i32 = 0;
    pub const K_RANGE_END_INDEX: i32 = 1;
    pub const K_RANGE_HANDLER_INDEX: i32 = 2;
    pub const K_RANGE_DATA_INDEX: i32 = 3;
    pub const K_RANGE_ENTRY_SIZE: i32 = 4;

    pub const K_HANDLER_PREDICTION_SHIFT: u32 = 0;
    pub const K_HANDLER_PREDICTION_BITS: u32 = 3;
    pub const K_HANDLER_WAS_USED_SHIFT: u32 = 3;
    pub const K_HANDLER_OFFSET_SHIFT: u32 = 4;

    #[inline]
    pub fn encode_handler(offset: i32, prediction: CatchPrediction) -> i32 {
        (offset << Self::K_HANDLER_OFFSET_SHIFT)
            | ((prediction as i32) << Self::K_HANDLER_PREDICTION_SHIFT)
    }

    #[inline]
    pub fn decode_handler_offset(value: i32) -> i32 {
        value >> Self::K_HANDLER_OFFSET_SHIFT
    }

    #[inline]
    pub fn decode_handler_prediction(value: i32) -> CatchPrediction {
        match value & ((1 << Self::K_HANDLER_PREDICTION_BITS) - 1) {
            1 => CatchPrediction::Caught,
            _ => CatchPrediction::Uncaught,
        }
    }

    pub fn new(entries: Vec<HandlerTableEntry>) -> HandlerTable {
        HandlerTable { entries }
    }

    #[inline]
    pub fn number_of_range_entries(&self) -> usize {
        self.entries.len()
    }

    #[inline]
    pub fn entry(&self, index: i32) -> &HandlerTableEntry {
        &self.entries[index as usize]
    }

    #[inline]
    pub fn get_range_start(&self, index: usize) -> i32 {
        self.entries[index].offset_start
    }

    #[inline]
    pub fn get_range_end(&self, index: usize) -> i32 {
        self.entries[index].offset_end
    }

    #[inline]
    pub fn get_range_handler(&self, index: usize) -> i32 {
        self.entries[index].offset_target
    }

    pub fn lookup_handler_index_for_range(&self, pc_offset: i32) -> i32 {
        let mut innermost_handler = K_NO_HANDLER_FOUND;
        for index in 0..self.number_of_range_entries() {
            let start_offset = self.get_range_start(index);
            let end_offset = self.get_range_end(index);
            if end_offset <= pc_offset {
                continue;
            }
            if start_offset > pc_offset {
                break;
            }
            innermost_handler = index as i32;
        }
        innermost_handler
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(start: i32, end: i32, target: i32) -> HandlerTableEntry {
        HandlerTableEntry {
            offset_start: start,
            offset_end: end,
            offset_target: target,
            context: Register::new(0),
            prediction: CatchPrediction::Caught,
        }
    }

    #[test]
    fn a_pc_outside_every_range_finds_nothing() {
        let table = HandlerTable::new(vec![entry(10, 20, 30)]);
        assert_eq!(table.lookup_handler_index_for_range(0), K_NO_HANDLER_FOUND);
        assert_eq!(table.lookup_handler_index_for_range(20), K_NO_HANDLER_FOUND);
        assert_eq!(table.lookup_handler_index_for_range(25), K_NO_HANDLER_FOUND);
    }

    #[test]
    fn the_range_is_half_open() {
        let table = HandlerTable::new(vec![entry(10, 20, 30)]);
        assert_eq!(table.lookup_handler_index_for_range(10), 0);
        assert_eq!(table.lookup_handler_index_for_range(19), 0);
        assert_eq!(table.lookup_handler_index_for_range(20), K_NO_HANDLER_FOUND);
    }

    #[test]
    fn nested_ranges_resolve_to_the_innermost() {
        let table = HandlerTable::new(vec![entry(0, 100, 200), entry(10, 50, 60)]);
        assert_eq!(table.lookup_handler_index_for_range(5), 0);
        assert_eq!(table.lookup_handler_index_for_range(20), 1);
        assert_eq!(table.lookup_handler_index_for_range(60), 0);
    }
}
