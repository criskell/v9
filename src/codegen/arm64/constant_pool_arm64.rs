use crate::codegen::arm64::assembler_arm64::Assembler;
use crate::codegen::arm64::constants_arm64::*;
use crate::codegen::arm64::register_arm64::XZR;
use crate::codegen::label::Label;
use std::collections::BTreeMap;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct ConstantPoolKey {
    is_value32: bool,
    value: u64,
}

impl ConstantPoolKey {
    pub fn new_value64(value: u64) -> ConstantPoolKey {
        ConstantPoolKey {
            is_value32: false,
            value,
        }
    }

    pub fn new_value32(value: u32) -> ConstantPoolKey {
        ConstantPoolKey {
            is_value32: true,
            value: value as u64,
        }
    }

    pub fn value64(&self) -> u64 {
        assert!(!self.is_value32);
        self.value
    }

    pub fn value32(&self) -> u32 {
        assert!(self.is_value32);
        self.value as u32
    }

    pub fn is_value32(&self) -> bool {
        self.is_value32
    }

    pub fn allows_deduplication(&self) -> bool {
        true
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Jump {
    Omitted,
    Required,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Emission {
    IfNeeded,
    Forced,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Alignment {
    Omitted,
    Required,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RelocInfoStatus {
    MustRecord,
    MustOmitForDuplicate,
}

pub const K_MAX_DIST_TO_POOL_32: usize = 1024 * 1024;
pub const K_MAX_DIST_TO_POOL_64: usize = 1024 * 1024;
pub const K_CHECK_INTERVAL: usize = 128 * K_INSTR_SIZE as usize;
pub const K_APPROX_DIST_TO_POOL_32: usize = 64 * 1024;
pub const K_APPROX_DIST_TO_POOL_64: usize = K_APPROX_DIST_TO_POOL_32;
pub const K_OPPORTUNITY_DIST_TO_POOL_32: usize = 64 * 1024;
pub const K_OPPORTUNITY_DIST_TO_POOL_64: usize = 64 * 1024;
pub const K_APPROX_MAX_ENTRY_COUNT: usize = 512;

#[derive(Default)]
pub struct ConstantPool {
    first_use_32: i32,
    first_use_64: i32,
    entries: BTreeMap<ConstantPoolKey, Vec<i32>>,
    entry32_count: usize,
    entry64_count: usize,
    next_check: i32,
    old_next_check: i32,
    blocked_nesting: i32,
}

impl ConstantPool {
    pub fn new() -> ConstantPool {
        ConstantPool {
            first_use_32: -1,
            first_use_64: -1,
            entries: BTreeMap::new(),
            entry32_count: 0,
            entry64_count: 0,
            next_check: 0,
            old_next_check: 0,
            blocked_nesting: 0,
        }
    }

    pub fn record_entry_32(&mut self, data: u32, pc_offset: i32) -> RelocInfoStatus {
        let key = ConstantPoolKey::new_value32(data);
        assert!(key.is_value32());
        self.record_key(key, pc_offset)
    }

    pub fn record_entry_64(&mut self, data: u64, pc_offset: i32) -> RelocInfoStatus {
        let key = ConstantPoolKey::new_value64(data);
        assert!(!key.is_value32());
        self.record_key(key, pc_offset)
    }

    fn record_key(&mut self, key: ConstantPoolKey, offset: i32) -> RelocInfoStatus {
        let write_reloc_info = self.get_reloc_info_status_for(&key);
        if write_reloc_info == RelocInfoStatus::MustRecord {
            if key.is_value32() {
                if self.entry32_count == 0 {
                    self.first_use_32 = offset;
                }
                self.entry32_count += 1;
            } else {
                if self.entry64_count == 0 {
                    self.first_use_64 = offset;
                }
                self.entry64_count += 1;
            }
        }
        self.entries.entry(key).or_default().push(offset);

        if self.entry_32_count() + self.entry_64_count() > K_APPROX_MAX_ENTRY_COUNT {
            self.set_next_check_in(1, offset);
        }

        write_reloc_info
    }

    fn get_reloc_info_status_for(&self, key: &ConstantPoolKey) -> RelocInfoStatus {
        if key.allows_deduplication() && self.entries.contains_key(key) {
            return RelocInfoStatus::MustOmitForDuplicate;
        }
        RelocInfoStatus::MustRecord
    }

    pub fn entry_32_count(&self) -> usize {
        self.entry32_count
    }

    pub fn entry_64_count(&self) -> usize {
        self.entry64_count
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn is_blocked(&self) -> bool {
        self.blocked_nesting > 0
    }

    pub fn start_block(&mut self) {
        if self.blocked_nesting == 0 {
            self.old_next_check = self.next_check;
            self.next_check = K_MAX_INT;
        }
        self.blocked_nesting += 1;
    }

    pub fn end_block(&mut self) {
        self.blocked_nesting -= 1;
        if self.blocked_nesting == 0 {
            self.next_check = std::cmp::min(self.next_check, self.old_next_check);
        }
    }

    pub fn set_next_check_in(&mut self, instructions: usize, pc_offset: i32) {
        self.next_check = pc_offset + (instructions * K_INSTR_SIZE as usize) as i32;
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.first_use_32 = -1;
        self.first_use_64 = -1;
        self.entry32_count = 0;
        self.entry64_count = 0;
        self.next_check = 0;
        self.old_next_check = 0;
    }

    pub fn prologue_size(&self, require_jump: Jump) -> i32 {
        let mut prologue_size = if require_jump == Jump::Required {
            K_INSTR_SIZE as i32
        } else {
            0
        };
        prologue_size += 2 * K_INSTR_SIZE as i32;
        prologue_size
    }

    pub fn compute_size(&self, require_jump: Jump, require_alignment: Alignment) -> i32 {
        let size_up_to_marker = self.prologue_size(require_jump);
        let alignment = if require_alignment == Alignment::Required {
            K_INSTR_SIZE as i32
        } else {
            0
        };
        let size_after_marker = self.entry_32_count() as i32 * K_INT32_SIZE
            + alignment
            + self.entry_64_count() as i32 * K_INT64_SIZE;
        size_up_to_marker + size_after_marker
    }

    pub fn is_alignment_required_if_emitted_at(
        &self,
        require_jump: Jump,
        pc_offset: i32,
    ) -> Alignment {
        let size_up_to_marker = self.prologue_size(require_jump);
        if self.entry_64_count() != 0 && (pc_offset + size_up_to_marker) % K_INT64_SIZE != 0 {
            return Alignment::Required;
        }
        Alignment::Omitted
    }

    pub fn is_in_imm_range_if_emitted_at(&self, pc_offset: i32) -> bool {
        let require_alignment =
            self.is_alignment_required_if_emitted_at(Jump::Required, pc_offset);
        let pool_end_32 = pc_offset + self.compute_size(Jump::Required, require_alignment);
        let pool_end_64 = pool_end_32 - self.entry_32_count() as i32 * K_INT32_SIZE;
        let entries_in_range_32 = self.entry_32_count() == 0
            || (pool_end_32) < self.first_use_32 + K_MAX_DIST_TO_POOL_32 as i32;
        let entries_in_range_64 = self.entry_64_count() == 0
            || (pool_end_64) < self.first_use_64 + K_MAX_DIST_TO_POOL_64 as i32;
        entries_in_range_32 && entries_in_range_64
    }

    pub fn should_emit_now(&self, require_jump: Jump, pc_offset: i32, margin: usize) -> bool {
        if self.is_empty() {
            return false;
        }
        if self.entry_32_count() + self.entry_64_count() > K_APPROX_MAX_ENTRY_COUNT {
            return true;
        }
        let worst_case_size = self.compute_size(Jump::Required, Alignment::Required);
        let pool_end_32 = pc_offset as usize + margin + worst_case_size as usize;
        let pool_end_64 = pool_end_32 - self.entry_32_count() * K_INT32_SIZE as usize;
        if self.entry_64_count() != 0 {
            let dist64 = pool_end_64 - self.first_use_64 as usize;
            let next_check_too_late = dist64 + 2 * K_CHECK_INTERVAL >= K_MAX_DIST_TO_POOL_64;
            let opportune_emission_without_jump =
                require_jump == Jump::Omitted && (dist64 >= K_OPPORTUNITY_DIST_TO_POOL_64);
            let approximate_distance_exceeded = dist64 >= K_APPROX_DIST_TO_POOL_64;
            if next_check_too_late
                || opportune_emission_without_jump
                || approximate_distance_exceeded
            {
                return true;
            }
        }
        if self.entry_32_count() != 0 {
            let dist32 = pool_end_32 - self.first_use_32 as usize;
            let next_check_too_late = dist32 + 2 * K_CHECK_INTERVAL >= K_MAX_DIST_TO_POOL_32;
            let opportune_emission_without_jump =
                require_jump == Jump::Omitted && (dist32 >= K_OPPORTUNITY_DIST_TO_POOL_32);
            let approximate_distance_exceeded = dist32 >= K_APPROX_DIST_TO_POOL_32;
            if next_check_too_late
                || opportune_emission_without_jump
                || approximate_distance_exceeded
            {
                return true;
            }
        }
        false
    }

    pub fn check(
        &mut self,
        assm: &mut Assembler,
        force_emit: Emission,
        require_jump: Jump,
        margin: usize,
    ) {
        if self.is_blocked() {
            debug_assert_eq!(force_emit, Emission::IfNeeded);
            return;
        }

        if !self.is_empty()
            && (force_emit == Emission::Forced
                || self.should_emit_now(require_jump, assm.pc_offset(), margin))
        {
            self.emit_and_clear(assm, require_jump);
        }
        self.set_next_check_in(K_CHECK_INTERVAL / K_INSTR_SIZE as usize, assm.pc_offset());
    }

    pub fn maybe_check(&mut self, assm: &mut Assembler) {
        if assm.pc_offset() >= self.next_check {
            self.check(assm, Emission::IfNeeded, Jump::Required, 0);
        }
    }

    pub fn emit_and_clear(&mut self, assm: &mut Assembler, require_jump: Jump) {
        debug_assert!(!self.is_blocked());
        self.start_block();
        let require_alignment =
            self.is_alignment_required_if_emitted_at(require_jump, assm.pc_offset());
        let size = self.compute_size(require_jump, require_alignment);
        let size_check = assm.pc_offset();

        let mut after_pool = Label::new();
        if require_jump == Jump::Required {
            assm.b_label(&mut after_pool);
        }

        self.emit_prologue(assm, require_alignment);
        if require_alignment == Alignment::Required {
            assm.align(K_INT64_SIZE);
        }
        self.emit_entries(assm);

        if after_pool.is_linked() {
            assm.bind(&mut after_pool);
        }

        debug_assert_eq!(assm.pc_offset() - size_check, size);
        self.end_block();
        self.clear();
    }

    fn emit_prologue(&self, assm: &mut Assembler, require_alignment: Alignment) {
        let marker_size = 1;
        let word_count =
            self.compute_size(Jump::Omitted, require_alignment) / K_INT32_SIZE - marker_size;
        let instruction =
            LDR_X_LIT | Assembler::imm_l_literal(word_count) | Assembler::rt_of(XZR);
        assm.emit_instruction(instruction);
        assm.emit_pool_guard();
    }

    fn emit_entries(&mut self, assm: &mut Assembler) {
        let entries: Vec<(ConstantPoolKey, Vec<i32>)> = self
            .entries
            .iter()
            .map(|(key, offsets)| (*key, offsets.clone()))
            .collect();
        for (key, offsets) in entries {
            debug_assert!(key.is_value32() || assm.pc_offset() % 8 == 0);
            let shared = key.allows_deduplication();
            for load_offset in &offsets {
                let entry_offset = assm.pc_offset();
                ConstantPool::set_load_offset_to_const_pool_entry(
                    assm,
                    *load_offset,
                    entry_offset,
                );
                if !shared {
                    ConstantPool::emit(assm, &key);
                }
            }
            if shared {
                ConstantPool::emit(assm, &key);
            }
        }
    }

    fn emit(assm: &mut Assembler, key: &ConstantPoolKey) {
        if key.is_value32() {
            assm.dd(key.value32());
        } else {
            assm.dq(key.value64());
        }
    }

    fn set_load_offset_to_const_pool_entry(
        assm: &mut Assembler,
        load_offset: i32,
        entry_offset: i32,
    ) {
        let mut instr = assm.instruction_at(load_offset);
        debug_assert!(instr.is_ldr_literal() && instr.imm_l_literal() == 0);
        instr.set_imm_pc_offset_target((entry_offset - load_offset) as i64);
        assm.set_instruction_at(load_offset, instr);
    }
}
