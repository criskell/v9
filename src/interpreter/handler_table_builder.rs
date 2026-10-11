use crate::codegen::handler_table::{CatchPrediction, HandlerTable, HandlerTableEntry};
use crate::interpreter::bytecode_register::Register;

#[derive(Default)]
pub struct HandlerTableBuilder {
    entries: Vec<Entry>,
}

#[derive(Copy, Clone)]
struct Entry {
    offset_start: i32,
    offset_end: i32,
    offset_target: i32,
    context: Register,
    catch_prediction: CatchPrediction,
    dropped: bool,
}

impl HandlerTableBuilder {
    pub fn new() -> HandlerTableBuilder {
        HandlerTableBuilder::default()
    }

    pub fn new_handler_entry(&mut self) -> i32 {
        let handler_id = self.entries.len() as i32;
        self.entries.push(Entry {
            offset_start: -1,
            offset_end: -1,
            offset_target: -1,
            context: Register::new(0),
            catch_prediction: CatchPrediction::Uncaught,
            dropped: false,
        });
        handler_id
    }

    pub fn set_try_region_start(&mut self, handler_id: i32, offset: usize) {
        self.entries[handler_id as usize].offset_start = offset as i32;
    }

    pub fn set_try_region_end(&mut self, handler_id: i32, offset: usize) {
        self.entries[handler_id as usize].offset_end = offset as i32;
    }

    pub fn set_handler_target(&mut self, handler_id: i32, offset: usize) {
        self.entries[handler_id as usize].offset_target = offset as i32;
    }

    pub fn set_prediction(&mut self, handler_id: i32, prediction: CatchPrediction) {
        self.entries[handler_id as usize].catch_prediction = prediction;
    }

    pub fn set_context_register(&mut self, handler_id: i32, register: Register) {
        self.entries[handler_id as usize].context = register;
    }

    pub fn drop_handler_entry(&mut self, handler_id: i32) {
        self.entries[handler_id as usize].dropped = true;
    }

    pub fn to_handler_table(&self) -> HandlerTable {
        let mut entries: Vec<HandlerTableEntry> = self
            .entries
            .iter()
            .filter(|entry| !entry.dropped)
            .map(|entry| HandlerTableEntry {
                offset_start: entry.offset_start,
                offset_end: entry.offset_end,
                offset_target: entry.offset_target,
                context: entry.context,
                prediction: entry.catch_prediction,
            })
            .collect();
        entries.sort_by_key(|entry| (entry.offset_start, -entry.offset_end));
        HandlerTable::new(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dropped_entry_does_not_reach_the_table() {
        let mut builder = HandlerTableBuilder::new();
        let kept = builder.new_handler_entry();
        let dropped = builder.new_handler_entry();
        builder.set_try_region_start(kept, 0);
        builder.set_try_region_end(kept, 10);
        builder.set_handler_target(kept, 12);
        builder.set_try_region_start(dropped, 2);
        builder.set_try_region_end(dropped, 4);
        builder.drop_handler_entry(dropped);

        let table = builder.to_handler_table();
        assert_eq!(table.number_of_range_entries(), 1);
        assert_eq!(table.get_range_handler(0), 12);
    }

    #[test]
    fn entries_come_out_in_the_order_the_lookup_expects() {
        let mut builder = HandlerTableBuilder::new();
        let inner = builder.new_handler_entry();
        let outer = builder.new_handler_entry();
        builder.set_try_region_start(inner, 10);
        builder.set_try_region_end(inner, 20);
        builder.set_handler_target(inner, 21);
        builder.set_try_region_start(outer, 0);
        builder.set_try_region_end(outer, 40);
        builder.set_handler_target(outer, 41);

        let table = builder.to_handler_table();
        assert_eq!(table.lookup_handler_index_for_range(15), 1);
        assert_eq!(table.get_range_handler(1), 21);
        assert_eq!(table.lookup_handler_index_for_range(30), 0);
        assert_eq!(table.get_range_handler(0), 41);
    }
}
