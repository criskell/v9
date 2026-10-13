#![allow(non_upper_case_globals)]

use crate::codegen::arm64::constants_arm64::*;
use crate::codegen::arm64::instructions_arm64::{
    kDebugCodeOffset, kDebugMessageOffset, kDebugParamsOffset, kImmExceptionIsDebug,
    kImmExceptionIsUnreachable, DebugParameters, Instruction,
};
use crate::codegen::arm64::register_arm64::*;
use crate::codegen::assembler::AssemblerBase;
use crate::codegen::cpu_features::CpuFeature;
use crate::codegen::label::{Distance, Label};
use crate::codegen::arm64::constant_pool_arm64::RelocInfoStatus;
use crate::codegen::reloc_info::{Mode, RelocInfo, RelocInfoWriter};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ConstantPoolMode {
    NeedsPoolEntry,
    NoPoolEntry,
}

use std::collections::{BTreeMap, HashMap};
use crate::codegen::arm64::utils_arm64::{count_leading_zeros, largest_power_of_2_divisor};
use crate::utils::utils::{
    checked_truncate_to_int14, checked_truncate_to_int19, checked_truncate_to_int26,
    checked_truncate_to_int7, checked_truncate_to_int9, is_int7,
    is_int9, is_uint1, is_uint12, is_uint16, is_uint2, is_uint32, is_uint5, is_uint6,
    is_uint7,
};

#[derive(Copy, Clone)]
pub struct Operand {
    immediate: i64,
    reg: Register,
    shift: Shift,
    extend: Extend,
    shift_amount: u32,
}

impl Operand {
    pub fn with_shift(reg: Register, shift: Shift, shift_amount: u32) -> Operand {
        debug_assert!(reg.is_64_bits() || (shift_amount < kWRegSizeInBits as u32));
        debug_assert!(reg.is_32_bits() || (shift_amount < kXRegSizeInBits as u32));
        debug_assert!(!reg.is_sp() || shift_amount == 0);
        Operand {
            immediate: 0,
            reg,
            shift,
            extend: Extend::NoExtend,
            shift_amount,
        }
    }

    pub fn with_extend(reg: Register, extend: Extend, shift_amount: u32) -> Operand {
        debug_assert!(reg.is_valid());
        debug_assert!(shift_amount <= 4);
        debug_assert!(!reg.is_sp());
        debug_assert!(
            (extend != Extend::Sxtx) || (extend != Extend::Uxtx) || !reg.is_32_bits()
        );
        Operand {
            immediate: 0,
            reg,
            shift: Shift::NoShift,
            extend,
            shift_amount,
        }
    }

    #[inline]
    pub fn is_immediate(&self) -> bool {
        self.reg == NoReg
    }

    #[inline]
    pub fn is_plain_register(&self) -> bool {
        self.reg.is_valid()
            && (((self.shift == Shift::NoShift) && (self.extend == Extend::NoExtend))
                || ((self.shift != Shift::NoShift) && (self.shift_amount == 0))
                || (((self.extend == Extend::Uxtx) || (self.extend == Extend::Sxtx))
                    && (self.shift_amount == 0)))
    }

    #[inline]
    pub fn is_shifted_register(&self) -> bool {
        self.reg.is_valid() && (self.shift != Shift::NoShift)
    }

    #[inline]
    pub fn is_extended_register(&self) -> bool {
        self.reg.is_valid() && (self.extend != Extend::NoExtend)
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        if self.is_immediate() {
            self.immediate_value() == 0
        } else {
            self.reg().is_zero()
        }
    }

    pub fn to_extended_register(&self) -> Operand {
        debug_assert!(self.is_shifted_register());
        debug_assert!((self.shift == Shift::Lsl) && (self.shift_amount <= 4));
        Operand::with_extend(
            self.reg,
            if self.reg.is_64_bits() {
                Extend::Uxtx
            } else {
                Extend::Uxtw
            },
            self.shift_amount,
        )
    }

    #[inline]
    pub fn immediate_value(&self) -> i64 {
        debug_assert!(self.is_immediate());
        self.immediate
    }

    #[inline]
    pub fn reg(&self) -> Register {
        debug_assert!(self.is_shifted_register() || self.is_extended_register());
        self.reg
    }

    #[inline]
    pub fn shift(&self) -> Shift {
        debug_assert!(self.is_shifted_register());
        self.shift
    }

    #[inline]
    pub fn extend(&self) -> Extend {
        debug_assert!(self.is_extended_register());
        self.extend
    }

    #[inline]
    pub fn shift_amount(&self) -> u32 {
        debug_assert!(self.is_shifted_register() || self.is_extended_register());
        self.shift_amount
    }
}

impl From<Register> for Operand {
    fn from(reg: Register) -> Operand {
        Operand::with_shift(reg, Shift::Lsl, 0)
    }
}

impl From<i64> for Operand {
    fn from(immediate: i64) -> Operand {
        Operand {
            immediate,
            reg: NoReg,
            shift: Shift::NoShift,
            extend: Extend::NoExtend,
            shift_amount: 0,
        }
    }
}

impl From<i32> for Operand {
    fn from(immediate: i32) -> Operand {
        Operand::from(immediate as i64)
    }
}

#[derive(Copy, Clone)]
pub struct MemOperand {
    base: Register,
    regoffset: Register,
    offset: i64,
    addrmode: AddrMode,
    shift: Shift,
    extend: Extend,
    shift_amount: u32,
}

impl MemOperand {
    pub fn new(base: Register, offset: i64, addrmode: AddrMode) -> MemOperand {
        debug_assert!(base.is_64_bits() && !base.is_zero());
        MemOperand {
            base,
            regoffset: NoReg,
            offset,
            addrmode,
            shift: Shift::NoShift,
            extend: Extend::NoExtend,
            shift_amount: 0,
        }
    }

    pub fn with_shift(
        base: Register,
        regoffset: Register,
        shift: Shift,
        shift_amount: u32,
    ) -> MemOperand {
        debug_assert!(base.is_64_bits() && !base.is_zero());
        debug_assert!(!regoffset.is_sp());
        debug_assert!(shift == Shift::Lsl);
        MemOperand {
            base,
            regoffset,
            offset: 0,
            addrmode: AddrMode::Offset,
            shift,
            extend: Extend::NoExtend,
            shift_amount,
        }
    }

    #[inline]
    pub fn base(&self) -> Register {
        self.base
    }

    #[inline]
    pub fn regoffset(&self) -> Register {
        self.regoffset
    }

    #[inline]
    pub fn offset(&self) -> i64 {
        self.offset
    }

    #[inline]
    pub fn addrmode(&self) -> AddrMode {
        self.addrmode
    }

    #[inline]
    pub fn shift(&self) -> Shift {
        self.shift
    }

    #[inline]
    pub fn extend(&self) -> Extend {
        self.extend
    }

    #[inline]
    pub fn shift_amount(&self) -> u32 {
        self.shift_amount
    }

    #[inline]
    pub fn is_immediate_offset(&self) -> bool {
        (self.addrmode == AddrMode::Offset) && self.regoffset == NoReg
    }

    #[inline]
    pub fn is_register_offset(&self) -> bool {
        (self.addrmode == AddrMode::Offset) && self.regoffset != NoReg
    }

    #[inline]
    pub fn is_pre_index(&self) -> bool {
        self.addrmode == AddrMode::PreIndex
    }

    #[inline]
    pub fn is_post_index(&self) -> bool {
        self.addrmode == AddrMode::PostIndex
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct LabelId(usize);

pub trait BranchTarget {
    fn branch_instruction_offset(self, assm: &mut Assembler) -> i32;
    fn byte_offset(self, assm: &mut Assembler) -> i32;
}

impl BranchTarget for i32 {
    fn branch_instruction_offset(self, _assm: &mut Assembler) -> i32 {
        self
    }

    fn byte_offset(self, _assm: &mut Assembler) -> i32 {
        self
    }
}

impl BranchTarget for LabelId {
    fn branch_instruction_offset(self, assm: &mut Assembler) -> i32 {
        assm.link_and_get_branch_instruction_offset_to(self)
    }

    fn byte_offset(self, assm: &mut Assembler) -> i32 {
        assm.link_and_get_byte_offset_to(self)
    }
}

pub struct Assembler {
    base: AssemblerBase,
    reloc_info_writer: RelocInfoWriter,
    internal_reference_positions: Vec<i32>,
    constpool: crate::codegen::arm64::constant_pool_arm64::ConstantPool,
    labels: Vec<Label>,
    unresolved_branches: BTreeMap<i32, LabelId>,
    branch_link_chain_back_edge: HashMap<i32, i32>,
    veneer_pool_blocked_nesting: i32,
    next_veneer_pool_check: i32,
}

impl Default for Assembler {
    fn default() -> Assembler {
        Assembler::new()
    }
}

impl Assembler {
    pub const kStartOfLabelLinkChain: i32 = 0;
    pub const kVeneerCodeSize: i32 = 1 * kInstrSize as i32;
    pub const kVeneerDistanceMargin: usize = 1 * crate::common::globals::K_KB;
    pub const kVeneerNoProtectionFactor: usize = 2;
    const kVeneerDistanceCheckMargin: i32 =
        (Assembler::kVeneerNoProtectionFactor * Assembler::kVeneerDistanceMargin) as i32;

    pub fn new_label(&mut self) -> LabelId {
        self.labels.push(Label::new());
        LabelId(self.labels.len() - 1)
    }

    pub fn label(&self, id: LabelId) -> &Label {
        &self.labels[id.0]
    }

    fn label_mut(&mut self, id: LabelId) -> &mut Label {
        &mut self.labels[id.0]
    }

    pub fn new() -> Assembler {
        Assembler {
            base: AssemblerBase::new(),
            reloc_info_writer: RelocInfoWriter::new(),
            internal_reference_positions: Vec::new(),
            constpool: crate::codegen::arm64::constant_pool_arm64::ConstantPool::new(),
            labels: Vec::new(),
            unresolved_branches: BTreeMap::new(),
            branch_link_chain_back_edge: HashMap::new(),
            veneer_pool_blocked_nesting: 0,
            next_veneer_pool_check: kMaxInt,
        }
    }

    #[inline]
    pub fn pc_offset(&self) -> i32 {
        self.base.pc_offset()
    }

    #[inline]
    pub fn buffer_start(&self) -> &[u8] {
        self.base.buffer_start()
    }

    pub fn instruction_at(&self, offset: i32) -> Instruction {
        let offset = offset as usize;
        let bytes = &self.base.buffer_start()[offset..offset + kInstrSize as usize];
        Instruction::new(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn set_instruction_at(&mut self, offset: i32, instr: Instruction) {
        self.base
            .write_bytes_at(offset, &instr.instruction_bits().to_le_bytes());
    }

    fn emit(&mut self, instruction: Instr) {
        self.base.emit_bytes(&instruction.to_le_bytes());
        self.check_buffer();
    }

    pub fn bind(&mut self, label: LabelId) {
        debug_assert!(!self.label(label).is_near_linked());
        debug_assert!(!self.label(label).is_bound());

        self.delete_unresolved_branch_info_for_label(label);

        while self.label(label).is_linked() {
            let linkoffset = self.label(label).pos();
            let mut link = self.instruction_at(linkoffset);
            let prevlinkoffset = if self.is_unresolved_internal_reference(linkoffset) {
                linkoffset + self.imm_unresolved_internal_reference(linkoffset)
            } else {
                linkoffset + link.imm_pc_offset() as i32
            };

            debug_assert!(linkoffset >= 0);
            debug_assert!(linkoffset < self.pc_offset());
            debug_assert!(
                (linkoffset > prevlinkoffset)
                    || (linkoffset - prevlinkoffset == Assembler::kStartOfLabelLinkChain)
            );
            debug_assert!(prevlinkoffset >= 0);

            if self.is_unresolved_internal_reference(linkoffset) {
                self.internal_reference_positions.push(linkoffset);
                let target = self.pc_offset() as u64;
                self.base.write_bytes_at(linkoffset, &target.to_le_bytes());
            } else {
                link.set_imm_pc_offset_target((self.pc_offset() - linkoffset) as i64);
                self.set_instruction_at(linkoffset, link);

                self.branch_link_chain_back_edge.remove(&linkoffset);
            }

            if linkoffset - prevlinkoffset == Assembler::kStartOfLabelLinkChain {
                self.label_mut(label).unuse();
            } else {
                self.label_mut(label).link_to(prevlinkoffset, Distance::Far);
            }
        }
        let pc_offset = self.pc_offset();
        self.label_mut(label).bind_to(pc_offset);

        debug_assert!(self.label(label).is_bound());
        debug_assert!(!self.label(label).is_linked());
    }

    fn link_and_get_byte_offset_to(&mut self, label: LabelId) -> i32 {
        let offset;
        if self.label(label).is_bound() {
            offset = self.label(label).pos() - self.pc_offset();
            debug_assert!(offset <= 0);
        } else {
            if self.label(label).is_linked() {
                offset = self.label(label).pos() - self.pc_offset();
                debug_assert_ne!(offset, Assembler::kStartOfLabelLinkChain);
            } else {
                offset = Assembler::kStartOfLabelLinkChain;
            }
            let pc_offset = self.pc_offset();
            self.label_mut(label).link_to(pc_offset, Distance::Far);
        }
        offset
    }

    fn link_and_get_branch_instruction_offset_to(&mut self, label: LabelId) -> i32 {
        debug_assert_eq!(Assembler::kStartOfLabelLinkChain, 0);
        let offset = self.link_and_get_byte_offset_to(label);
        debug_assert_eq!(offset % kInstrSize as i32, 0);
        if self.label(label).is_linked() && (offset != Assembler::kStartOfLabelLinkChain) {
            let pc_offset = self.pc_offset();
            self.branch_link_chain_back_edge
                .insert(pc_offset + offset, pc_offset);
        }
        offset >> kInstrSizeLog2
    }

    pub fn start_block_veneer_pool(&mut self) {
        self.veneer_pool_blocked_nesting += 1;
    }

    pub fn end_block_veneer_pool(&mut self) {
        self.veneer_pool_blocked_nesting -= 1;
        debug_assert!(self.veneer_pool_blocked_nesting >= 0);
    }

    pub fn is_veneer_pool_blocked(&self) -> bool {
        self.veneer_pool_blocked_nesting > 0
    }

    fn unresolved_branches_first_limit(&self) -> i32 {
        debug_assert!(!self.unresolved_branches.is_empty());
        *self.unresolved_branches.keys().next().unwrap() & !1
    }

    fn max_pc_offset_after_veneer_pool_if_emitted_now(&self, margin: usize) -> isize {
        const kBranchSizeInBytes: i32 = kInstrSize as i32;
        const kGuardSizeInBytes: i32 = kInstrSize as i32;
        let max_veneer_size_in_bytes =
            self.unresolved_branches.len() * Assembler::kVeneerCodeSize as usize;
        (self.pc_offset() + kBranchSizeInBytes + kGuardSizeInBytes) as isize
            + max_veneer_size_in_bytes as isize
            + margin as isize
    }

    fn should_emit_veneer(&self, max_reachable_pc: i32, margin: usize) -> bool {
        (max_reachable_pc as isize) < self.max_pc_offset_after_veneer_pool_if_emitted_now(margin)
    }

    pub fn should_emit_veneers(&self, margin: usize) -> bool {
        self.should_emit_veneer(self.unresolved_branches_first_limit(), margin)
    }

    pub fn register_pending_unresolved_branch(
        &mut self,
        branch_type: ImmBranchType,
        label: LabelId,
    ) {
        let max_reachable_pc = self.pc_offset() + Instruction::imm_branch_range(branch_type);
        let branch_type_tag = if branch_type == ImmBranchType::TestBranchType {
            1
        } else {
            0
        };
        self.unresolved_branches
            .insert(max_reachable_pc + branch_type_tag, label);
        self.next_veneer_pool_check = std::cmp::min(
            self.next_veneer_pool_check,
            max_reachable_pc - Assembler::kVeneerDistanceCheckMargin,
        );
    }

    fn imm_pc_offset_target(&self, offset: i32) -> i32 {
        offset + self.instruction_at(offset).imm_pc_offset() as i32
    }

    fn set_imm_pc_offset_target(&mut self, offset: i32, target: i32) {
        if self.is_unresolved_internal_reference(offset) {
            self.set_unresolved_internal_reference_imm_target(offset, target - offset);
            return;
        }
        let mut instr = self.instruction_at(offset);
        instr.set_imm_pc_offset_target((target - offset) as i64);
        self.set_instruction_at(offset, instr);
    }

    fn erase_unresolved_branch(&mut self, link_offset: i32) {
        let link = self.instruction_at(link_offset);
        if link.is_cond_branch_imm() || link.is_compare_branch() {
            debug_assert_eq!(
                Instruction::imm_branch_range(ImmBranchType::CondBranchType),
                Instruction::imm_branch_range(ImmBranchType::CompareBranchType)
            );
            let max_reachable_pc =
                link_offset + Instruction::imm_branch_range(ImmBranchType::CondBranchType);
            self.unresolved_branches.remove(&max_reachable_pc);
        } else if link.is_test_branch() {
            let max_reachable_pc =
                link_offset + Instruction::imm_branch_range(ImmBranchType::TestBranchType) + 1;
            self.unresolved_branches.remove(&max_reachable_pc);
        }
    }

    fn remove_branch_from_label_link_chain(
        &mut self,
        branch: i32,
        label: LabelId,
        label_veneer: Option<i32>,
    ) {
        debug_assert!(self.label(label).is_linked());

        let mut link = self.label(label).pos();
        let mut prev_link = link;

        if link != branch {
            debug_assert!(self.branch_link_chain_back_edge.contains_key(&branch));
            prev_link = self.branch_link_chain_back_edge[&branch];
            link = branch;
        }

        debug_assert_eq!(branch, link);
        let mut next_link = self.imm_pc_offset_target(branch);

        if branch == prev_link {
            if branch == next_link {
                self.label_mut(label).unuse();
            } else {
                self.label_mut(label).link_to(next_link, Distance::Far);
                self.branch_link_chain_back_edge.remove(&next_link);
            }
        } else if branch == next_link {
            self.set_imm_pc_offset_target(prev_link, prev_link);
            self.branch_link_chain_back_edge.remove(&branch);
        } else {
            if self.branch_link_chain_back_edge.contains_key(&next_link) {
                self.branch_link_chain_back_edge
                    .insert(next_link, prev_link);
                self.branch_link_chain_back_edge.remove(&branch);
            }

            let distance = (next_link - prev_link) as i64;
            if self.is_unresolved_internal_reference(prev_link)
                || self
                    .instruction_at(prev_link)
                    .is_target_in_imm_pc_offset_range(distance)
            {
                self.set_imm_pc_offset_target(prev_link, next_link);
            } else if let Some(veneer) = label_veneer {
                self.set_imm_pc_offset_target(prev_link, prev_link);

                let mut end_of_chain = false;
                link = next_link;
                while !end_of_chain {
                    next_link = self.imm_pc_offset_target(link);
                    end_of_chain = link == next_link;
                    self.set_imm_pc_offset_target(link, veneer);
                    self.erase_unresolved_branch(link);
                    link = next_link;
                }
            } else {
                assert!(self
                    .instruction_at(prev_link)
                    .is_target_in_imm_pc_offset_range(distance));
                unreachable!();
            }
        }
    }

    fn delete_unresolved_branch_info_for_label_traverse(&mut self, label: LabelId) {
        debug_assert!(self.label(label).is_linked());

        let mut link_offset = self.label(label).pos();
        let mut link_pcoffset;
        let mut end_of_chain = false;

        while !end_of_chain {
            let link = self.instruction_at(link_offset);
            let mut max_reachable_pc = link_offset;

            if link.is_cond_branch_imm() || link.is_compare_branch() {
                debug_assert_eq!(
                    Instruction::imm_branch_range(ImmBranchType::CondBranchType),
                    Instruction::imm_branch_range(ImmBranchType::CompareBranchType)
                );
                max_reachable_pc += Instruction::imm_branch_range(ImmBranchType::CondBranchType);
                self.unresolved_branches.remove(&max_reachable_pc);
                link_pcoffset = link.imm_cond_branch() * kInstrSize as i32;
            } else if link.is_test_branch() {
                max_reachable_pc +=
                    Instruction::imm_branch_range(ImmBranchType::TestBranchType) + 1;
                self.unresolved_branches.remove(&max_reachable_pc);
                link_pcoffset = link.imm_test_branch() * kInstrSize as i32;
            } else if link.is_uncond_branch_imm() {
                link_pcoffset = link.imm_uncond_branch() * kInstrSize as i32;
            } else {
                link_pcoffset = link.imm_pc_offset() as i32;
            }

            end_of_chain = link_pcoffset == 0;
            link_offset += link_pcoffset;
        }
    }

    fn delete_unresolved_branch_info_for_label(&mut self, label: LabelId) {
        if self.unresolved_branches.is_empty() {
            debug_assert_eq!(self.next_veneer_pool_check, kMaxInt);
            return;
        }

        if self.label(label).is_linked() {
            self.delete_unresolved_branch_info_for_label_traverse(label);
        }
        if self.unresolved_branches.is_empty() {
            self.next_veneer_pool_check = kMaxInt;
        } else {
            self.next_veneer_pool_check =
                self.unresolved_branches_first_limit() - Assembler::kVeneerDistanceCheckMargin;
        }
    }

    pub fn emit_veneers_guard(&mut self) {
        self.emit_pool_guard();
    }

    pub fn emit_veneers(&mut self, force_emit: bool, need_protection: bool, margin: usize) {
        self.constpool.start_block();
        self.start_block_veneer_pool();

        let end = self.new_label();
        if need_protection {
            self.b(end);
        }

        self.emit_veneers_guard();

        let veneer_pool_relocinfo_loc = self.pc_offset();
        let max_pc_after_veneers = self.max_pc_offset_after_veneer_pool_if_emitted_now(margin);

        let mut tasks: Vec<(i32, LabelId)> = Vec::new();
        for (&key, &label) in self.unresolved_branches.iter() {
            if !force_emit && ((key & !1) as isize) > max_pc_after_veneers {
                break;
            }
            tasks.push((key, label));
        }

        for (key, label) in tasks {
            if !self.unresolved_branches.contains_key(&key) {
                continue;
            }
            let mut pc_offset = key;
            if (pc_offset & 1) != 0 {
                pc_offset -= Instruction::imm_branch_range(ImmBranchType::TestBranchType) + 1;
            } else {
                debug_assert_eq!(
                    Instruction::imm_branch_range(ImmBranchType::CondBranchType),
                    Instruction::imm_branch_range(ImmBranchType::CompareBranchType)
                );
                pc_offset -= Instruction::imm_branch_range(ImmBranchType::CondBranchType);
            }
            let veneer = self.pc_offset();
            self.remove_branch_from_label_link_chain(pc_offset, label, Some(veneer));
            self.set_imm_pc_offset_target(pc_offset, veneer);
            self.b(label);
            self.unresolved_branches.remove(&key);
        }

        if self.unresolved_branches.is_empty() {
            self.next_veneer_pool_check = kMaxInt;
        } else {
            self.next_veneer_pool_check =
                self.unresolved_branches_first_limit() - Assembler::kVeneerDistanceCheckMargin;
        }

        let pool_size = self.pc_offset() - veneer_pool_relocinfo_loc;
        self.record_veneer_pool(veneer_pool_relocinfo_loc, pool_size);

        self.bind(end);

        self.end_block_veneer_pool();
        self.constpool.end_block();
    }

    pub fn check_veneer_pool(&mut self, force_emit: bool, require_jump: bool, margin: usize) {
        if self.unresolved_branches.is_empty() {
            debug_assert_eq!(self.next_veneer_pool_check, kMaxInt);
            return;
        }

        debug_assert!(self.pc_offset() < self.unresolved_branches_first_limit());

        if self.is_veneer_pool_blocked() {
            debug_assert!(!force_emit);
            return;
        }

        let mut margin = margin;
        if !require_jump {
            margin *= Assembler::kVeneerNoProtectionFactor;
        }
        if force_emit || self.should_emit_veneers(margin) {
            self.emit_veneers(force_emit, require_jump, margin);
        } else {
            self.next_veneer_pool_check =
                self.unresolved_branches_first_limit() - Assembler::kVeneerDistanceCheckMargin;
        }
    }

    fn rd(rd: impl Into<CPURegister>) -> Instr {
        let rd = rd.into();
        debug_assert_ne!(rd.code(), kSPRegInternalCode);
        (rd.code() as Instr) << Rd_offset
    }

    fn rn(rn: impl Into<CPURegister>) -> Instr {
        let rn = rn.into();
        debug_assert_ne!(rn.code(), kSPRegInternalCode);
        (rn.code() as Instr) << Rn_offset
    }

    fn rm(rm: impl Into<CPURegister>) -> Instr {
        let rm = rm.into();
        debug_assert_ne!(rm.code(), kSPRegInternalCode);
        (rm.code() as Instr) << Rm_offset
    }

    fn rt(rt: impl Into<CPURegister>) -> Instr {
        let rt = rt.into();
        debug_assert_ne!(rt.code(), kSPRegInternalCode);
        (rt.code() as Instr) << Rt_offset
    }

    fn rd_sp(rd: Register) -> Instr {
        debug_assert!(!rd.is_zero());
        ((rd.code() as Instr) & kRegCodeMask) << Rd_offset
    }

    fn rn_sp(rn: Register) -> Instr {
        debug_assert!(!rn.is_zero());
        ((rn.code() as Instr) & kRegCodeMask) << Rn_offset
    }

    fn flags(s: FlagsUpdate) -> Instr {
        match s {
            FlagsUpdate::SetFlags => 1 << FlagsUpdate_offset,
            FlagsUpdate::LeaveFlags => 0 << FlagsUpdate_offset,
        }
    }

    fn imm_uncond_branch(imm26: i32) -> Instr {
        (checked_truncate_to_int26(imm26 as i64) as Instr) << ImmUncondBranch_offset
    }

    fn imm_cond_branch(imm19: i32) -> Instr {
        (checked_truncate_to_int19(imm19 as i64) as Instr) << ImmCondBranch_offset
    }

    fn imm_cmp_branch(imm19: i32) -> Instr {
        (checked_truncate_to_int19(imm19 as i64) as Instr) << ImmCmpBranch_offset
    }

    fn sf(rd: Register) -> Instr {
        if rd.is_64_bits() {
            SixtyFourBits
        } else {
            ThirtyTwoBits
        }
    }

    fn imm_add_sub(imm: i32) -> Instr {
        debug_assert!(Assembler::is_imm_add_sub(imm as i64));
        let mut imm = imm;
        if is_uint12(imm as i64) {
            imm <<= ImmAddSub_offset;
        } else {
            imm = ((imm >> 12) << ImmAddSub_offset) | (1 << ShiftAddSub_offset);
        }
        imm as Instr
    }

    fn shift_dp(shift: Shift) -> Instr {
        debug_assert!(
            shift == Shift::Lsl
                || shift == Shift::Lsr
                || shift == Shift::Asr
                || shift == Shift::Ror
        );
        (shift as Instr) << ShiftDP_offset
    }

    fn imm_dp_shift(amount: u32) -> Instr {
        debug_assert!(is_uint6(amount as i64));
        amount << ImmDPShift_offset
    }

    fn imm_ls_unsigned(imm12: i32) -> Instr {
        debug_assert!(is_uint12(imm12 as i64));
        (imm12 as Instr) << ImmLSUnsigned_offset
    }

    fn imm_ls(imm9: i32) -> Instr {
        (checked_truncate_to_int9(imm9 as i64) as Instr) << ImmLS_offset
    }

    fn imm_exception(imm16: i32) -> Instr {
        debug_assert!(is_uint16(imm16 as i64));
        (imm16 as Instr) << ImmException_offset
    }

    fn imm_hint(imm7: i32) -> Instr {
        debug_assert!(is_uint7(imm7 as i64));
        (imm7 as Instr) << ImmHint_offset
    }

    fn imm_move_wide(imm: i32) -> Instr {
        debug_assert!(is_uint16(imm as i64));
        (imm as Instr) << ImmMoveWide_offset
    }

    fn shift_move_wide(shift: i32) -> Instr {
        debug_assert!(is_uint2(shift as i64));
        (shift as Instr) << ShiftMoveWide_offset
    }

    pub fn is_imm_add_sub(immediate: i64) -> bool {
        is_uint12(immediate)
            || (is_uint12(immediate >> 12) && ((immediate & 0xFFF) == 0))
    }

    pub fn is_imm_ls_unscaled(offset: i64) -> bool {
        is_int9(offset)
    }

    pub fn is_imm_ls_scaled(offset: i64, size_log2: u32) -> bool {
        let offset_is_size_multiple = ((offset >> size_log2) << size_log2) == offset;
        offset_is_size_multiple && is_uint12(offset >> size_log2)
    }

    pub fn appropriate_zero_reg_for(reg: impl Into<CPURegister>) -> Register {
        let reg = reg.into();
        if reg.is_64_bits() {
            xzr
        } else {
            wzr
        }
    }

    fn load_op_for(rt: impl Into<CPURegister>) -> Instr {
        let rt = rt.into();
        debug_assert!(rt.is_valid());
        if rt.is_register() {
            return if rt.is_64_bits() { LDR_x } else { LDR_w };
        }
        debug_assert!(rt.is_v_register());
        match rt.size_in_bits() as i32 {
            kBRegSizeInBits => LDR_b,
            kHRegSizeInBits => LDR_h,
            kSRegSizeInBits => LDR_s,
            kDRegSizeInBits => LDR_d,
            _ => {
                debug_assert!(rt.is_q());
                LDR_q
            }
        }
    }

    fn store_op_for(rt: impl Into<CPURegister>) -> Instr {
        let rt = rt.into();
        debug_assert!(rt.is_valid());
        if rt.is_register() {
            return if rt.is_64_bits() { STR_x } else { STR_w };
        }
        debug_assert!(rt.is_v_register());
        match rt.size_in_bits() as i32 {
            kBRegSizeInBits => STR_b,
            kHRegSizeInBits => STR_h,
            kSRegSizeInBits => STR_s,
            kDRegSizeInBits => STR_d,
            _ => {
                debug_assert!(rt.is_q());
                STR_q
            }
        }
    }

    fn calc_ls_data_size_log2(op: Instr) -> u32 {
        let mut size_log2 = op >> LSSize_offset;
        if (op & LSVector_mask) != 0 && size_log2 == 0 && ((op & LSOpc_mask) >> LSOpc_offset) >= 2
        {
            size_log2 = kQRegSizeLog2 as u32;
        }
        size_log2
    }

    pub fn b(&mut self, imm26: impl BranchTarget) {
        let imm26 = imm26.branch_instruction_offset(self);
        let instruction = B | Assembler::imm_uncond_branch(imm26);
        self.emit(instruction);
    }

    pub fn b_cond(&mut self, imm19: impl BranchTarget, cond: Condition) {
        let imm19 = imm19.branch_instruction_offset(self);
        let instruction = B_cond | Assembler::imm_cond_branch(imm19) | (cond as Instr);
        self.emit(instruction);
    }

    pub fn bl(&mut self, imm26: impl BranchTarget) {
        let imm26 = imm26.branch_instruction_offset(self);
        let instruction = BL | Assembler::imm_uncond_branch(imm26);
        self.emit(instruction);
    }

    pub fn cbz(&mut self, rt: Register, imm19: impl BranchTarget) {
        let imm19 = imm19.branch_instruction_offset(self);
        let instruction =
            Assembler::sf(rt) | CBZ | Assembler::imm_cmp_branch(imm19) | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn cbnz(&mut self, rt: Register, imm19: impl BranchTarget) {
        let imm19 = imm19.branch_instruction_offset(self);
        let instruction =
            Assembler::sf(rt) | CBNZ | Assembler::imm_cmp_branch(imm19) | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn br(&mut self, xn: Register) {
        debug_assert!(xn.is_64_bits());
        let instruction = BR | Assembler::rn(xn);
        self.emit(instruction);
    }

    pub fn blr(&mut self, xn: Register) {
        debug_assert!(xn.is_64_bits());
        debug_assert_ne!(xn, xzr);
        let instruction = BLR | Assembler::rn(xn);
        self.emit(instruction);
    }

    pub fn ret(&mut self, xn: Register) {
        debug_assert!(xn.is_64_bits());
        let instruction = RET | Assembler::rn(xn);
        self.emit(instruction);
    }

    pub fn adds<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub(rd, rn, operand.into(), FlagsUpdate::SetFlags, ADD);
    }

    pub fn cmn<T: Into<Operand>>(&mut self, rn: Register, operand: T) {
        let zr = Assembler::appropriate_zero_reg_for(rn);
        self.adds(zr, rn, operand);
    }

    pub fn subs<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub(rd, rn, operand.into(), FlagsUpdate::SetFlags, SUB);
    }

    pub fn cmp<T: Into<Operand>>(&mut self, rn: Register, operand: T) {
        let zr = Assembler::appropriate_zero_reg_for(rn);
        self.subs(zr, rn, operand);
    }

    pub fn neg<T: Into<Operand>>(&mut self, rd: Register, operand: T) {
        let zr = Assembler::appropriate_zero_reg_for(rd);
        self.sub(rd, zr, operand);
    }

    pub fn mov(&mut self, rd: Register, rm: Register) {
        if rd.is_sp() || rm.is_sp() {
            self.add(rd, rm, 0);
        } else {
            let zr = Assembler::appropriate_zero_reg_for(rd);
            self.orr(rd, zr, rm);
        }
    }

    pub fn movz(&mut self, rd: Register, imm: u64, shift: i32) {
        self.move_wide(rd, imm, shift, MOVZ);
    }

    pub fn movn(&mut self, rd: Register, imm: u64, shift: i32) {
        self.move_wide(rd, imm, shift, MOVN);
    }

    pub fn movk(&mut self, rd: Register, imm: u64, shift: i32) {
        self.move_wide(rd, imm, shift, MOVK);
    }

    pub fn ldr(&mut self, rt: impl Into<CPURegister> + Copy, src: MemOperand) {
        self.load_store(rt, src, Assembler::load_op_for(rt));
    }

    pub fn str(&mut self, rt: impl Into<CPURegister> + Copy, src: MemOperand) {
        self.load_store(rt, src, Assembler::store_op_for(rt));
    }

    pub fn ldrb(&mut self, rt: Register, src: MemOperand) {
        self.load_store(rt, src, LDRB_w);
    }

    pub fn strb(&mut self, rt: Register, src: MemOperand) {
        self.load_store(rt, src, STRB_w);
    }

    pub fn ldrh(&mut self, rt: Register, src: MemOperand) {
        self.load_store(rt, src, LDRH_w);
    }

    pub fn strh(&mut self, rt: Register, src: MemOperand) {
        self.load_store(rt, src, STRH_w);
    }

    pub fn ldrsb(&mut self, rt: Register, src: MemOperand) {
        let op = if rt.is_64_bits() { LDRSB_x } else { LDRSB_w };
        self.load_store(rt, src, op);
    }

    pub fn ldrsh(&mut self, rt: Register, src: MemOperand) {
        let op = if rt.is_64_bits() { LDRSH_x } else { LDRSH_w };
        self.load_store(rt, src, op);
    }

    pub fn ldrsw(&mut self, rt: Register, src: MemOperand) {
        debug_assert!(rt.is_64_bits());
        self.load_store(rt, src, LDRSW_x);
    }

    pub fn brk(&mut self, code: i32) {
        debug_assert!(is_uint16(code as i64));
        let instruction = BRK | Assembler::imm_exception(code);
        self.emit(instruction);
    }

    pub fn hlt(&mut self, code: i32) {
        debug_assert!(is_uint16(code as i64));
        let instruction = HLT | Assembler::imm_exception(code);
        self.emit(instruction);
    }

    pub fn hint(&mut self, code: SystemHint) {
        let instruction = HINT | Assembler::imm_hint(code as i32) | Assembler::rt(xzr);
        self.emit(instruction);
    }

    pub fn nop(&mut self) {
        self.hint(SystemHint::Nop);
    }

    fn ra(ra: impl Into<CPURegister>) -> Instr {
        let ra = ra.into();
        debug_assert_ne!(ra.code(), kSPRegInternalCode);
        (ra.code() as Instr) << Ra_offset
    }

    fn rt2(rt2: impl Into<CPURegister>) -> Instr {
        let rt2 = rt2.into();
        debug_assert_ne!(rt2.code(), kSPRegInternalCode);
        (rt2.code() as Instr) << Rt2_offset
    }

    fn cond(cond: Condition) -> Instr {
        (cond as Instr) << Condition_offset
    }

    fn nzcv(nzcv: u32) -> Instr {
        ((nzcv >> Flags_offset) & 0xf) << Nzcv_offset
    }

    fn imm_cond_cmp(imm: u32) -> Instr {
        debug_assert!(is_uint5(imm as i64));
        imm << ImmCondCmp_offset
    }

    fn bit_n(bitn: u32, reg_size: u32) -> Instr {
        debug_assert!(
            (reg_size == kWRegSizeInBits as u32)
                || (reg_size == kXRegSizeInBits as u32)
        );
        debug_assert!((reg_size == kXRegSizeInBits as u32) || (bitn == 0));
        bitn << BitN_offset
    }

    fn imm_rotate(immr: u32, reg_size: u32) -> Instr {
        debug_assert!(
            ((reg_size == kXRegSizeInBits as u32) && is_uint6(immr as i64))
                || ((reg_size == kWRegSizeInBits as u32) && is_uint5(immr as i64))
        );
        immr << ImmRotate_offset
    }

    fn imm_set_bits(imms: u32, reg_size: u32) -> Instr {
        debug_assert!(
            (reg_size == kWRegSizeInBits as u32)
                || (reg_size == kXRegSizeInBits as u32)
        );
        debug_assert!(is_uint6(imms as i64));
        debug_assert!(
            (reg_size == kXRegSizeInBits as u32) || is_uint6(imms as i64 + 3)
        );
        imms << ImmSetBits_offset
    }

    fn imm_r(immr: u32, reg_size: u32) -> Instr {
        debug_assert!(
            ((reg_size == kXRegSizeInBits as u32) && is_uint6(immr as i64))
                || ((reg_size == kWRegSizeInBits as u32) && is_uint5(immr as i64))
        );
        debug_assert!(is_uint6(immr as i64));
        immr << ImmR_offset
    }

    fn imm_s(imms: u32, reg_size: u32) -> Instr {
        debug_assert!(
            ((reg_size == kXRegSizeInBits as u32) && is_uint6(imms as i64))
                || ((reg_size == kWRegSizeInBits as u32) && is_uint5(imms as i64))
        );
        imms << ImmS_offset
    }

    fn imm_test_branch(imm14: i32) -> Instr {
        (checked_truncate_to_int14(imm14 as i64) as Instr) << ImmTestBranch_offset
    }

    fn imm_test_branch_bit(bit_pos: u32) -> Instr {
        debug_assert!(is_uint6(bit_pos as i64));
        let bit5 = bit_pos << (ImmTestBranchBit5_offset - 5);
        let bit40 = bit_pos << ImmTestBranchBit40_offset;
        let bit5 = bit5 & ImmTestBranchBit5_mask;
        let bit40 = bit40 & ImmTestBranchBit40_mask;
        bit5 | bit40
    }

    fn imm_ls_pair(imm7: i32, size: u32) -> Instr {
        debug_assert_eq!(imm7, (imm7 >> size) << size);
        let scaled_imm7 = imm7 >> size;
        (checked_truncate_to_int7(scaled_imm7 as i64) as Instr) << ImmLSPair_offset
    }

    pub fn is_imm_ls_pair(offset: i64, size: u32) -> bool {
        let offset_is_size_multiple = ((offset >> size) << size) == offset;
        offset_is_size_multiple && is_int7(offset >> size)
    }

    pub fn is_imm_conditional_compare(immediate: i64) -> bool {
        is_uint5(immediate)
    }

    fn calc_ls_pair_data_size(op: Instr) -> u32 {
        match op {
            STP_q | LDP_q => kQRegSizeLog2 as u32,
            STP_x | LDP_x | STP_d | LDP_d => kXRegSizeLog2 as u32,
            _ => kWRegSizeLog2 as u32,
        }
    }

    fn load_pair_op_for(rt: impl Into<CPURegister>, rt2: impl Into<CPURegister>) -> Instr {
        debug_assert_eq!(STP_w | LoadStorePairLBit, LDP_w);
        Assembler::store_pair_op_for(rt, rt2) | LoadStorePairLBit
    }

    fn store_pair_op_for(rt: impl Into<CPURegister>, _rt2: impl Into<CPURegister>) -> Instr {
        let rt = rt.into();
        if rt.is_register() {
            return if rt.is_64_bits() { STP_x } else { STP_w };
        }
        debug_assert!(rt.is_v_register());
        match rt.size_in_bits() as i32 {
            kSRegSizeInBits => STP_s,
            kDRegSizeInBits => STP_d,
            _ => {
                debug_assert!(rt.is_q());
                STP_q
            }
        }
    }

    pub fn is_imm_logical(
        value: u64,
        width: u32,
        n: &mut u32,
        imm_s: &mut u32,
        imm_r: &mut u32,
    ) -> bool {
        debug_assert!(
            (width == kWRegSizeInBits as u32) || (width == kXRegSizeInBits as u32)
        );

        let mut negate = false;
        let mut value = value;

        if value & 1 != 0 {
            negate = true;
            value = !value;
        }

        if width == kWRegSizeInBits as u32 {
            value <<= kWRegSizeInBits;
            value |= value >> kWRegSizeInBits;
        }

        let a = largest_power_of_2_divisor(value);
        let value_plus_a = value.wrapping_add(a);
        let b = largest_power_of_2_divisor(value_plus_a);
        let value_plus_a_minus_b = value_plus_a.wrapping_sub(b);
        let c = largest_power_of_2_divisor(value_plus_a_minus_b);

        let d;
        let clz_a;
        let out_n;
        let mask;

        if c != 0 {
            clz_a = count_leading_zeros(a, kXRegSizeInBits);
            let clz_c = count_leading_zeros(c, kXRegSizeInBits);
            d = clz_a - clz_c;
            mask = (1u64 << d) - 1;
            out_n = 0;
        } else {
            if a == 0 {
                return false;
            }
            clz_a = count_leading_zeros(a, kXRegSizeInBits);
            d = 64;
            mask = !0u64;
            out_n = 1;
        }

        if !crate::base::bits::is_power_of_two(d as u32) {
            return false;
        }

        if (b.wrapping_sub(a) & !mask) != 0 {
            return false;
        }

        const MULTIPLIERS: [u64; 6] = [
            0x0000000000000001,
            0x0000000100000001,
            0x0001000100010001,
            0x0101010101010101,
            0x1111111111111111,
            0x5555555555555555,
        ];
        let multiplier_idx = count_leading_zeros(d as u64, kXRegSizeInBits) - 57;
        debug_assert!(
            (multiplier_idx >= 0) && ((multiplier_idx as usize) < MULTIPLIERS.len())
        );
        let multiplier = MULTIPLIERS[multiplier_idx as usize];
        let candidate = b.wrapping_sub(a).wrapping_mul(multiplier);

        if value != candidate {
            return false;
        }

        let clz_b = if b == 0 {
            -1
        } else {
            count_leading_zeros(b, kXRegSizeInBits)
        };
        let mut s = clz_a - clz_b;

        let r;
        if negate {
            s = d - s;
            r = (clz_b + 1) & (d - 1);
        } else {
            r = (clz_a + 1) & (d - 1);
        }

        *n = out_n;
        *imm_s = (((-d * 2) | (s - 1)) & 0x3F) as u32;
        *imm_r = r as u32;

        true
    }

    fn logical_immediate(
        &mut self,
        rd: Register,
        rn: Register,
        n: u32,
        imm_s: u32,
        imm_r: u32,
        op: Instr,
    ) {
        let reg_size = rd.size_in_bits();
        let dest_reg = if op == ANDS {
            Assembler::rd(rd)
        } else {
            Assembler::rd_sp(rd)
        };
        let instruction = Assembler::sf(rd)
            | LogicalImmediateFixed
            | op
            | Assembler::bit_n(n, reg_size)
            | Assembler::imm_set_bits(imm_s, reg_size)
            | Assembler::imm_rotate(imm_r, reg_size)
            | dest_reg
            | Assembler::rn(rn);
        self.emit(instruction);
    }

    pub fn ands<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.logical(rd, rn, operand.into(), ANDS);
    }

    pub fn bics<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.logical(rd, rn, operand.into(), BICS);
    }

    pub fn eon<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.logical(rd, rn, operand.into(), EON);
    }

    pub fn tst<T: Into<Operand>>(&mut self, rn: Register, operand: T) {
        let zr = Assembler::appropriate_zero_reg_for(rn);
        self.ands(zr, rn, operand);
    }

    pub fn mvn<T: Into<Operand>>(&mut self, rd: Register, operand: T) {
        let zr = Assembler::appropriate_zero_reg_for(rd);
        self.orn(rd, zr, operand);
    }

    pub fn negs<T: Into<Operand>>(&mut self, rd: Register, operand: T) {
        let zr = Assembler::appropriate_zero_reg_for(rd);
        self.subs(rd, zr, operand);
    }

    pub fn adc<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub_with_carry(rd, rn, operand.into(), FlagsUpdate::LeaveFlags, ADC);
    }

    pub fn adcs<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub_with_carry(rd, rn, operand.into(), FlagsUpdate::SetFlags, ADC);
    }

    pub fn sbc<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub_with_carry(rd, rn, operand.into(), FlagsUpdate::LeaveFlags, SBC);
    }

    pub fn sbcs<T: Into<Operand>>(&mut self, rd: Register, rn: Register, operand: T) {
        self.add_sub_with_carry(rd, rn, operand.into(), FlagsUpdate::SetFlags, SBC);
    }

    pub fn ngc<T: Into<Operand>>(&mut self, rd: Register, operand: T) {
        let zr = Assembler::appropriate_zero_reg_for(rd);
        self.sbc(rd, zr, operand);
    }

    pub fn ngcs<T: Into<Operand>>(&mut self, rd: Register, operand: T) {
        let zr = Assembler::appropriate_zero_reg_for(rd);
        self.sbcs(rd, zr, operand);
    }

    fn add_sub_with_carry(
        &mut self,
        rd: Register,
        rn: Register,
        operand: Operand,
        s: FlagsUpdate,
        op: Instr,
    ) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), operand.reg().size_in_bits());
        debug_assert!(operand.is_shifted_register() && (operand.shift_amount() == 0));
        let instruction = Assembler::sf(rd)
            | op
            | Assembler::flags(s)
            | Assembler::rm(operand.reg())
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn bfm(&mut self, rd: Register, rn: Register, immr: u32, imms: u32) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        let n = Assembler::sf(rd) >> (kSFOffset - kBitfieldNOffset);
        let instruction = Assembler::sf(rd)
            | BFM
            | n
            | Assembler::imm_r(immr, rd.size_in_bits())
            | Assembler::imm_s(imms, rn.size_in_bits())
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn sbfm(&mut self, rd: Register, rn: Register, immr: u32, imms: u32) {
        debug_assert!(rd.is_64_bits() || rn.is_32_bits());
        let n = Assembler::sf(rd) >> (kSFOffset - kBitfieldNOffset);
        let instruction = Assembler::sf(rd)
            | SBFM
            | n
            | Assembler::imm_r(immr, rd.size_in_bits())
            | Assembler::imm_s(imms, rn.size_in_bits())
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn ubfm(&mut self, rd: Register, rn: Register, immr: u32, imms: u32) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        let n = Assembler::sf(rd) >> (kSFOffset - kBitfieldNOffset);
        let instruction = Assembler::sf(rd)
            | UBFM
            | n
            | Assembler::imm_r(immr, rd.size_in_bits())
            | Assembler::imm_s(imms, rn.size_in_bits())
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn extr(&mut self, rd: Register, rn: Register, rm: Register, lsb: u32) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), rm.size_in_bits());
        let n = Assembler::sf(rd) >> (kSFOffset - kBitfieldNOffset);
        let instruction = Assembler::sf(rd)
            | EXTR
            | n
            | Assembler::rm(rm)
            | Assembler::imm_s(lsb, rn.size_in_bits())
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn lsl(&mut self, rd: Register, rn: Register, shift: u32) {
        let reg_size = rd.size_in_bits();
        debug_assert!(shift < reg_size);
        self.ubfm(rd, rn, (reg_size - shift) % reg_size, reg_size - shift - 1);
    }

    pub fn lsr(&mut self, rd: Register, rn: Register, shift: u32) {
        debug_assert!(shift < rd.size_in_bits());
        self.ubfm(rd, rn, shift, rd.size_in_bits() - 1);
    }

    pub fn asr(&mut self, rd: Register, rn: Register, shift: u32) {
        debug_assert!(shift < rd.size_in_bits());
        self.sbfm(rd, rn, shift, rd.size_in_bits() - 1);
    }

    pub fn ror(&mut self, rd: Register, rs: Register, shift: u32) {
        self.extr(rd, rs, rs, shift);
    }

    pub fn sbfx(&mut self, rd: Register, rn: Register, lsb: u32, width: u32) {
        debug_assert!(width >= 1);
        debug_assert!(lsb + width <= rn.size_in_bits());
        self.sbfm(rd, rn, lsb, lsb + width - 1);
    }

    pub fn ubfx(&mut self, rd: Register, rn: Register, lsb: u32, width: u32) {
        debug_assert!(width >= 1);
        debug_assert!(lsb + width <= rn.size_in_bits());
        self.ubfm(rd, rn, lsb, lsb + width - 1);
    }

    pub fn sbfiz(&mut self, rd: Register, rn: Register, lsb: u32, width: u32) {
        debug_assert!(width >= 1);
        debug_assert!(lsb + width <= rn.size_in_bits());
        self.sbfm(
            rd,
            rn,
            (rd.size_in_bits() - lsb) & (rd.size_in_bits() - 1),
            width - 1,
        );
    }

    pub fn ubfiz(&mut self, rd: Register, rn: Register, lsb: u32, width: u32) {
        debug_assert!(width >= 1);
        debug_assert!(lsb + width <= rn.size_in_bits());
        self.ubfm(
            rd,
            rn,
            (rd.size_in_bits() - lsb) & (rd.size_in_bits() - 1),
            width - 1,
        );
    }

    pub fn bfi(&mut self, rd: Register, rn: Register, lsb: u32, width: u32) {
        debug_assert!(width >= 1);
        debug_assert!(lsb + width <= rn.size_in_bits());
        self.bfm(
            rd,
            rn,
            (rd.size_in_bits() - lsb) & (rd.size_in_bits() - 1),
            width - 1,
        );
    }

    pub fn bfxil(&mut self, rd: Register, rn: Register, lsb: u32, width: u32) {
        debug_assert!(width >= 1);
        debug_assert!(lsb + width <= rn.size_in_bits());
        self.bfm(rd, rn, lsb, lsb + width - 1);
    }

    pub fn sxtb(&mut self, rd: Register, rn: Register) {
        self.sbfm(rd, rn, 0, 7);
    }

    pub fn sxth(&mut self, rd: Register, rn: Register) {
        self.sbfm(rd, rn, 0, 15);
    }

    pub fn sxtw(&mut self, rd: Register, rn: Register) {
        self.sbfm(rd, rn, 0, 31);
    }

    pub fn uxtb(&mut self, rd: Register, rn: Register) {
        self.ubfm(rd, rn, 0, 7);
    }

    pub fn uxth(&mut self, rd: Register, rn: Register) {
        self.ubfm(rd, rn, 0, 15);
    }

    pub fn uxtw(&mut self, rd: Register, rn: Register) {
        self.ubfm(rd, rn, 0, 31);
    }

    fn conditional_select(
        &mut self,
        rd: Register,
        rn: Register,
        rm: Register,
        cond: Condition,
        op: Instr,
    ) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), rm.size_in_bits());
        let instruction = Assembler::sf(rd)
            | op
            | Assembler::rm(rm)
            | Assembler::cond(cond)
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn csel(&mut self, rd: Register, rn: Register, rm: Register, cond: Condition) {
        self.conditional_select(rd, rn, rm, cond, CSEL);
    }

    pub fn csinc(&mut self, rd: Register, rn: Register, rm: Register, cond: Condition) {
        self.conditional_select(rd, rn, rm, cond, CSINC);
    }

    pub fn csinv(&mut self, rd: Register, rn: Register, rm: Register, cond: Condition) {
        self.conditional_select(rd, rn, rm, cond, CSINV);
    }

    pub fn csneg(&mut self, rd: Register, rn: Register, rm: Register, cond: Condition) {
        self.conditional_select(rd, rn, rm, cond, CSNEG);
    }

    pub fn cset(&mut self, rd: Register, cond: Condition) {
        debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
        let zr = Assembler::appropriate_zero_reg_for(rd);
        self.csinc(rd, zr, zr, negate_condition(cond));
    }

    pub fn csetm(&mut self, rd: Register, cond: Condition) {
        debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
        let zr = Assembler::appropriate_zero_reg_for(rd);
        self.csinv(rd, zr, zr, negate_condition(cond));
    }

    pub fn cinc(&mut self, rd: Register, rn: Register, cond: Condition) {
        debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
        self.csinc(rd, rn, rn, negate_condition(cond));
    }

    pub fn cinv(&mut self, rd: Register, rn: Register, cond: Condition) {
        debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
        self.csinv(rd, rn, rn, negate_condition(cond));
    }

    pub fn cneg(&mut self, rd: Register, rn: Register, cond: Condition) {
        debug_assert!((cond != Condition::Al) && (cond != Condition::Nv));
        self.csneg(rd, rn, rn, negate_condition(cond));
    }

    fn conditional_compare(
        &mut self,
        rn: Register,
        operand: Operand,
        nzcv: u32,
        cond: Condition,
        op: Instr,
    ) {
        let ccmpop;
        if operand.is_immediate() {
            let immediate = operand.immediate_value();
            debug_assert!(Assembler::is_imm_conditional_compare(immediate));
            ccmpop = ConditionalCompareImmediateFixed
                | op
                | Assembler::imm_cond_cmp(immediate as u32);
        } else {
            debug_assert!(operand.is_shifted_register() && (operand.shift_amount() == 0));
            ccmpop = ConditionalCompareRegisterFixed | op | Assembler::rm(operand.reg());
        }
        let instruction = Assembler::sf(rn)
            | ccmpop
            | Assembler::cond(cond)
            | Assembler::rn(rn)
            | Assembler::nzcv(nzcv);
        self.emit(instruction);
    }

    pub fn ccmn<T: Into<Operand>>(
        &mut self,
        rn: Register,
        operand: T,
        nzcv: u32,
        cond: Condition,
    ) {
        self.conditional_compare(rn, operand.into(), nzcv, cond, CCMN);
    }

    pub fn ccmp<T: Into<Operand>>(
        &mut self,
        rn: Register,
        operand: T,
        nzcv: u32,
        cond: Condition,
    ) {
        self.conditional_compare(rn, operand.into(), nzcv, cond, CCMP);
    }

    fn data_processing_1_source(&mut self, rd: Register, rn: Register, op: Instr) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        let instruction = Assembler::sf(rn) | op | Assembler::rn(rn) | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn rbit(&mut self, rd: Register, rn: Register) {
        self.data_processing_1_source(rd, rn, RBIT);
    }

    pub fn rev16(&mut self, rd: Register, rn: Register) {
        self.data_processing_1_source(rd, rn, REV16);
    }

    pub fn rev32(&mut self, rd: Register, rn: Register) {
        debug_assert!(rd.is_64_bits());
        self.data_processing_1_source(rd, rn, REV);
    }

    pub fn rev(&mut self, rd: Register, rn: Register) {
        let op = if rd.is_64_bits() { REV_x } else { REV_w };
        self.data_processing_1_source(rd, rn, op);
    }

    pub fn clz(&mut self, rd: Register, rn: Register) {
        self.data_processing_1_source(rd, rn, CLZ);
    }

    pub fn cls(&mut self, rd: Register, rn: Register) {
        self.data_processing_1_source(rd, rn, CLS);
    }

    pub fn lslv(&mut self, rd: Register, rn: Register, rm: Register) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), rm.size_in_bits());
        let instruction = Assembler::sf(rd)
            | LSLV
            | Assembler::rm(rm)
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn lsrv(&mut self, rd: Register, rn: Register, rm: Register) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), rm.size_in_bits());
        let instruction = Assembler::sf(rd)
            | LSRV
            | Assembler::rm(rm)
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn asrv(&mut self, rd: Register, rn: Register, rm: Register) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), rm.size_in_bits());
        let instruction = Assembler::sf(rd)
            | ASRV
            | Assembler::rm(rm)
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn rorv(&mut self, rd: Register, rn: Register, rm: Register) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), rm.size_in_bits());
        let instruction = Assembler::sf(rd)
            | RORV
            | Assembler::rm(rm)
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn sdiv(&mut self, rd: Register, rn: Register, rm: Register) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), rm.size_in_bits());
        let instruction = Assembler::sf(rd)
            | SDIV
            | Assembler::rm(rm)
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn udiv(&mut self, rd: Register, rn: Register, rm: Register) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        debug_assert_eq!(rd.size_in_bits(), rm.size_in_bits());
        let instruction = Assembler::sf(rd)
            | UDIV
            | Assembler::rm(rm)
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    fn data_processing_3_source(
        &mut self,
        rd: Register,
        rn: Register,
        rm: Register,
        ra: Register,
        op: Instr,
    ) {
        let instruction = Assembler::sf(rd)
            | op
            | Assembler::rm(rm)
            | Assembler::ra(ra)
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn madd(&mut self, rd: Register, rn: Register, rm: Register, ra: Register) {
        self.data_processing_3_source(rd, rn, rm, ra, MADD);
    }

    pub fn msub(&mut self, rd: Register, rn: Register, rm: Register, ra: Register) {
        self.data_processing_3_source(rd, rn, rm, ra, MSUB);
    }

    pub fn mneg(&mut self, rd: Register, rn: Register, rm: Register) {
        let zr = Assembler::appropriate_zero_reg_for(rn);
        self.data_processing_3_source(rd, rn, rm, zr, MSUB);
    }

    pub fn smaddl(&mut self, rd: Register, rn: Register, rm: Register, ra: Register) {
        debug_assert!(rd.is_64_bits() && ra.is_64_bits());
        debug_assert!(rn.is_32_bits() && rm.is_32_bits());
        self.data_processing_3_source(rd, rn, rm, ra, SMADDL_x);
    }

    pub fn smsubl(&mut self, rd: Register, rn: Register, rm: Register, ra: Register) {
        debug_assert!(rd.is_64_bits() && ra.is_64_bits());
        debug_assert!(rn.is_32_bits() && rm.is_32_bits());
        self.data_processing_3_source(rd, rn, rm, ra, SMSUBL_x);
    }

    pub fn umaddl(&mut self, rd: Register, rn: Register, rm: Register, ra: Register) {
        debug_assert!(rd.is_64_bits() && ra.is_64_bits());
        debug_assert!(rn.is_32_bits() && rm.is_32_bits());
        self.data_processing_3_source(rd, rn, rm, ra, UMADDL_x);
    }

    pub fn umsubl(&mut self, rd: Register, rn: Register, rm: Register, ra: Register) {
        debug_assert!(rd.is_64_bits() && ra.is_64_bits());
        debug_assert!(rn.is_32_bits() && rm.is_32_bits());
        self.data_processing_3_source(rd, rn, rm, ra, UMSUBL_x);
    }

    pub fn smulh(&mut self, rd: Register, rn: Register, rm: Register) {
        debug_assert!(rd.is_64_bits());
        debug_assert!(rn.is_64_bits());
        debug_assert!(rm.is_64_bits());
        self.data_processing_3_source(rd, rn, rm, xzr, SMULH_x);
    }

    pub fn umulh(&mut self, rd: Register, rn: Register, rm: Register) {
        debug_assert!(rd.is_64_bits());
        debug_assert!(rn.is_64_bits());
        debug_assert!(rm.is_64_bits());
        self.data_processing_3_source(rd, rn, rm, xzr, UMULH_x);
    }

    pub fn tbz(&mut self, rt: Register, bit_pos: u32, imm14: impl BranchTarget) {
        let imm14 = imm14.branch_instruction_offset(self);
        debug_assert!(rt.is_64_bits() || (rt.is_32_bits() && (bit_pos < kWRegSizeInBits as u32)));
        let instruction = TBZ
            | Assembler::imm_test_branch_bit(bit_pos)
            | Assembler::imm_test_branch(imm14)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn tbnz(&mut self, rt: Register, bit_pos: u32, imm14: impl BranchTarget) {
        let imm14 = imm14.branch_instruction_offset(self);
        debug_assert!(rt.is_64_bits() || (rt.is_32_bits() && (bit_pos < kWRegSizeInBits as u32)));
        let instruction = TBNZ
            | Assembler::imm_test_branch_bit(bit_pos)
            | Assembler::imm_test_branch(imm14)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn adr(&mut self, rd: Register, imm21: impl BranchTarget) {
        let imm21 = imm21.byte_offset(self);
        debug_assert!(rd.is_64_bits());
        let instruction = ADR | crate::codegen::arm64::instructions_arm64::imm_pc_rel_address(imm21) | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn ldp(&mut self, rt: impl Into<CPURegister> + Copy, rt2: impl Into<CPURegister> + Copy, src: MemOperand) {
        let op = Assembler::load_pair_op_for(rt, rt2);
        self.load_store_pair(rt, rt2, src, op);
    }

    pub fn stp(&mut self, rt: impl Into<CPURegister> + Copy, rt2: impl Into<CPURegister> + Copy, dst: MemOperand) {
        let op = Assembler::store_pair_op_for(rt, rt2);
        self.load_store_pair(rt, rt2, dst, op);
    }

    pub fn ldpsw(&mut self, rt: Register, rt2: Register, src: MemOperand) {
        debug_assert!(rt.is_64_bits());
        self.load_store_pair(rt, rt2, src, LDPSW_x);
    }

    fn load_store_pair(
        &mut self,
        rt: impl Into<CPURegister>,
        rt2: impl Into<CPURegister>,
        addr: MemOperand,
        op: Instr,
    ) {
        let rt = rt.into();
        let rt2 = rt2.into();
        debug_assert!(((op & LoadStorePairLBit) == 0) || rt != rt2);
        debug_assert_eq!(rt.size_in_bits(), rt2.size_in_bits());
        debug_assert!(Assembler::is_imm_ls_pair(
            addr.offset(),
            Assembler::calc_ls_pair_data_size(op)
        ));
        let offset = addr.offset() as i32;

        let memop = op
            | Assembler::rt(rt)
            | Assembler::rt2(rt2)
            | Assembler::rn_sp(addr.base())
            | Assembler::imm_ls_pair(offset, Assembler::calc_ls_pair_data_size(op));

        let addrmodeop;
        if addr.is_immediate_offset() {
            addrmodeop = LoadStorePairOffsetFixed;
        } else {
            debug_assert_ne!(rt, addr.base().into());
            debug_assert_ne!(rt2, addr.base().into());
            debug_assert_ne!(addr.offset(), 0);
            if addr.is_pre_index() {
                addrmodeop = LoadStorePairPreIndexFixed;
            } else {
                debug_assert!(addr.is_post_index());
                addrmodeop = LoadStorePairPostIndexFixed;
            }
        }
        self.emit(addrmodeop | memop);
    }

    fn fp_type(fd: VRegister) -> Instr {
        if fd.is_64_bits() {
            FP64
        } else {
            FP32
        }
    }

    fn fp_scale(scale: u32) -> Instr {
        debug_assert!(is_uint6(scale as i64));
        scale << FPScale_offset
    }

    fn imm_fp(imm: f64) -> Instr {
        Assembler::fp_to_imm8(imm) << ImmFP_offset
    }

    pub fn fp_to_imm8(imm: f64) -> u32 {
        let bits = imm.to_bits();
        debug_assert!(Assembler::is_imm_fp64(bits));
        let bit7 = ((bits >> 63) & 0x1) << 7;
        let bit6 = ((bits >> 61) & 0x1) << 6;
        let bit5_to_0 = (bits >> 48) & 0x3F;
        (bit7 | bit6 | bit5_to_0) as u32
    }

    pub fn is_imm_fp64(bits: u64) -> bool {
        if (bits & 0xFFFFFFFFFFFF) != 0 {
            return false;
        }

        let b_pattern = ((bits >> 48) & 0x3FC0) as u32;
        if b_pattern != 0 && b_pattern != 0x3FC0 {
            return false;
        }

        if ((bits ^ (bits << 1)) & 0x4000000000000000) == 0 {
            return false;
        }

        true
    }

    fn fp_data_processing_1_source(&mut self, vd: VRegister, vn: VRegister, op: Instr) {
        let instruction =
            Assembler::fp_type(vn) | op | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    #[allow(dead_code)]
    fn fp_data_processing_2_source(
        &mut self,
        fd: VRegister,
        fn_: VRegister,
        fm: VRegister,
        op: Instr,
    ) {
        debug_assert_eq!(fd.size_in_bits(), fn_.size_in_bits());
        debug_assert_eq!(fd.size_in_bits(), fm.size_in_bits());
        let instruction = Assembler::fp_type(fd)
            | op
            | Assembler::rm(fm)
            | Assembler::rn(fn_)
            | Assembler::rd(fd);
        self.emit(instruction);
    }

    fn fp_data_processing_3_source(
        &mut self,
        fd: VRegister,
        fn_: VRegister,
        fm: VRegister,
        fa: VRegister,
        op: Instr,
    ) {
        let instruction = Assembler::fp_type(fd)
            | op
            | Assembler::rm(fm)
            | Assembler::rn(fn_)
            | Assembler::ra(fa)
            | Assembler::rd(fd);
        self.emit(instruction);
    }

    pub fn fmov(&mut self, vd: VRegister, vn: VRegister) {
        debug_assert_eq!(vd.size_in_bits(), vn.size_in_bits());
        let instruction =
            Assembler::fp_type(vd) | FMOV | Assembler::rd(vd) | Assembler::rn(vn);
        self.emit(instruction);
    }

    pub fn fmov_from_register(&mut self, vd: VRegister, rn: Register) {
        debug_assert_eq!(vd.size_in_bits(), rn.size_in_bits());
        let op = if vd.is_32_bits() { FMOV_sw } else { FMOV_dx };
        let instruction = op | Assembler::rd(vd) | Assembler::rn(rn);
        self.emit(instruction);
    }

    pub fn fmov_to_register(&mut self, rd: Register, fn_: VRegister) {
        debug_assert_eq!(rd.size_in_bits(), fn_.size_in_bits());
        let op = if rd.is_32_bits() { FMOV_ws } else { FMOV_xd };
        let instruction = op | Assembler::rd(rd) | Assembler::rn(fn_);
        self.emit(instruction);
    }

    pub fn fmov_immediate(&mut self, vd: VRegister, imm: f64) {
        debug_assert!(vd.is_scalar());
        let op = if vd.is_1d() { FMOV_d_imm } else { FMOV_s_imm };
        debug_assert!(vd.is_1d() || vd.is_1s());
        let instruction = op | Assembler::rd(vd) | Assembler::imm_fp(imm);
        self.emit(instruction);
    }

    pub fn fcmp(&mut self, fn_: VRegister, fm: VRegister) {
        debug_assert_eq!(fn_.size_in_bits(), fm.size_in_bits());
        let instruction =
            Assembler::fp_type(fn_) | FCMP | Assembler::rm(fm) | Assembler::rn(fn_);
        self.emit(instruction);
    }

    pub fn fcmp_zero(&mut self, fn_: VRegister, value: f64) {
        debug_assert_eq!(value, 0.0);
        let instruction = Assembler::fp_type(fn_) | FCMP_zero | Assembler::rn(fn_);
        self.emit(instruction);
    }

    pub fn fcvt(&mut self, vd: VRegister, vn: VRegister) {
        let op;
        if vd.is_1d() {
            debug_assert!(vn.is_1s() || vn.is_1h());
            op = if vn.is_1s() { FCVT_ds } else { FCVT_dh };
        } else if vd.is_1s() {
            debug_assert!(vn.is_1d() || vn.is_1h());
            op = if vn.is_1d() { FCVT_sd } else { FCVT_sh };
        } else {
            debug_assert!(vd.is_1h());
            debug_assert!(vn.is_1d() || vn.is_1s());
            op = if vn.is_1d() { FCVT_hd } else { FCVT_hs };
        }
        self.fp_data_processing_1_source(vd, vn, op);
    }

    pub fn scvtf(&mut self, vd: VRegister, rn: Register, fbits: u32) {
        let instruction = if fbits == 0 {
            Assembler::sf(rn)
                | Assembler::fp_type(vd)
                | SCVTF
                | Assembler::rn(rn)
                | Assembler::rd(vd)
        } else {
            Assembler::sf(rn)
                | Assembler::fp_type(vd)
                | SCVTF_fixed
                | Assembler::fp_scale(64 - fbits)
                | Assembler::rn(rn)
                | Assembler::rd(vd)
        };
        self.emit(instruction);
    }

    pub fn ucvtf(&mut self, fd: VRegister, rn: Register, fbits: u32) {
        let instruction = if fbits == 0 {
            Assembler::sf(rn)
                | Assembler::fp_type(fd)
                | UCVTF
                | Assembler::rn(rn)
                | Assembler::rd(fd)
        } else {
            Assembler::sf(rn)
                | Assembler::fp_type(fd)
                | UCVTF_fixed
                | Assembler::fp_scale(64 - fbits)
                | Assembler::rn(rn)
                | Assembler::rd(fd)
        };
        self.emit(instruction);
    }

    pub fn fcvtzs(&mut self, rd: Register, vn: VRegister, fbits: u32) {
        debug_assert!(vn.is_1s() || vn.is_1d());
        debug_assert!(fbits <= rd.size_in_bits());
        let instruction = if fbits == 0 {
            Assembler::sf(rd)
                | Assembler::fp_type(vn)
                | FCVTZS
                | Assembler::rn(vn)
                | Assembler::rd(rd)
        } else {
            Assembler::sf(rd)
                | Assembler::fp_type(vn)
                | FCVTZS_fixed
                | Assembler::fp_scale(64 - fbits)
                | Assembler::rn(vn)
                | Assembler::rd(rd)
        };
        self.emit(instruction);
    }

    pub fn fcvtzu(&mut self, rd: Register, vn: VRegister, fbits: u32) {
        debug_assert!(vn.is_1s() || vn.is_1d());
        debug_assert!(fbits <= rd.size_in_bits());
        let instruction = if fbits == 0 {
            Assembler::sf(rd)
                | Assembler::fp_type(vn)
                | FCVTZU
                | Assembler::rn(vn)
                | Assembler::rd(rd)
        } else {
            Assembler::sf(rd)
                | Assembler::fp_type(vn)
                | FCVTZU_fixed
                | Assembler::fp_scale(64 - fbits)
                | Assembler::rn(vn)
                | Assembler::rd(rd)
        };
        self.emit(instruction);
    }

    pub fn fjcvtzs(&mut self, rd: Register, vn: VRegister) {
        debug_assert!(rd.is_w());
        debug_assert!(vn.is_1d());
        let instruction = FJCVTZS | Assembler::rn(vn) | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn fnmul(&mut self, vd: VRegister, vn: VRegister, vm: VRegister) {
        let op = if vd.is_1s() { FNMUL_s } else { FNMUL_d };
        let instruction = Assembler::fp_type(vd)
            | op
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn fmadd(&mut self, fd: VRegister, fn_: VRegister, fm: VRegister, fa: VRegister) {
        let op = if fd.is_32_bits() { FMADD_s } else { FMADD_d };
        self.fp_data_processing_3_source(fd, fn_, fm, fa, op);
    }

    pub fn fmsub(&mut self, fd: VRegister, fn_: VRegister, fm: VRegister, fa: VRegister) {
        let op = if fd.is_32_bits() { FMSUB_s } else { FMSUB_d };
        self.fp_data_processing_3_source(fd, fn_, fm, fa, op);
    }

    pub fn fnmadd(&mut self, fd: VRegister, fn_: VRegister, fm: VRegister, fa: VRegister) {
        let op = if fd.is_32_bits() { FNMADD_s } else { FNMADD_d };
        self.fp_data_processing_3_source(fd, fn_, fm, fa, op);
    }

    pub fn fnmsub(&mut self, fd: VRegister, fn_: VRegister, fm: VRegister, fa: VRegister) {
        let op = if fd.is_32_bits() { FNMSUB_s } else { FNMSUB_d };
        self.fp_data_processing_3_source(fd, fn_, fm, fa, op);
    }

    pub fn emit_instruction(&mut self, instruction: Instr) {
        self.emit(instruction);
    }

    pub fn rt_of(rt: impl Into<CPURegister>) -> Instr {
        Assembler::rt(rt)
    }

    pub fn imm_l_literal(imm19: i32) -> Instr {
        (checked_truncate_to_int19(imm19 as i64) as Instr) << ImmLLiteral_offset
    }

    pub fn is_imm_l_literal(offset: i64) -> bool {
        crate::utils::utils::is_int19(offset >> kLoadLiteralScaleLog2)
    }

    fn load_literal_op_for(rt: impl Into<CPURegister>) -> Instr {
        let rt = rt.into();
        if rt.is_register() {
            return if rt.is_64_bits() { LDR_x_lit } else { LDR_w_lit };
        }
        debug_assert!(rt.is_v_register());
        if rt.is_64_bits() {
            LDR_d_lit
        } else {
            LDR_s_lit
        }
    }

    pub fn ldr_pcrel(&mut self, rt: impl Into<CPURegister> + Copy, imm19: i32) {
        debug_assert!(!rt.into().is_zero());
        let instruction = Assembler::load_literal_op_for(rt)
            | Assembler::imm_l_literal(imm19)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn ldr_literal(&mut self, rt: impl Into<CPURegister> + Copy, imm: u64) {
        self.block_pools();
        if rt.into().is_32_bits() {
            let pc_offset = self.pc_offset();
            self.constpool
                .record_entry(imm as u32, Mode::NoInfo, pc_offset);
        } else {
            let pc_offset = self.pc_offset();
            self.constpool.record_entry(imm, Mode::NoInfo, pc_offset);
        }
        self.ldr_pcrel(rt, 0);
        self.release_pools();
    }

    pub fn emit_pool_guard(&mut self) {
        let instruction = BLR | Assembler::rn(xzr);
        self.emit(instruction);
    }

    pub fn emit_data(&mut self, data: &[u8]) {
        self.base.emit_bytes(data);
    }

    pub fn emit_string_data(&mut self, string: &str) {
        let mut bytes = string.as_bytes().to_vec();
        bytes.push(0);
        self.emit_data(&bytes);
        let pad = [0u8; kInstrSize as usize];
        let aligned = (self.pc_offset() + kInstrSize as i32 - 1) & !(kInstrSize as i32 - 1);
        let padding = (aligned - self.pc_offset()) as usize;
        self.emit_data(&pad[..padding]);
    }

    pub fn debug(&mut self, message: &str, code: u32, params: Instr) {
        if self.base.options().enable_simulator_code {
            let size_of_debug_sequence = 4 * kInstrSize as i32
                + ((message.len() as i32 + 1 + kInstrSize as i32 - 1) & !(kInstrSize as i32 - 1));

            self.block_pools();

            let start = self.pc_offset();

            self.hlt(kImmExceptionIsDebug as i32);
            debug_assert_eq!(self.pc_offset() - start, kDebugCodeOffset as i32);
            self.dc32(code);
            debug_assert_eq!(self.pc_offset() - start, kDebugParamsOffset as i32);
            self.dc32(params);
            debug_assert_eq!(self.pc_offset() - start, kDebugMessageOffset as i32);
            self.emit_string_data(message);
            self.hlt(kImmExceptionIsUnreachable as i32);
            debug_assert_eq!(self.pc_offset() - start, size_of_debug_sequence);

            self.release_pools();
            return;
        }

        if (params & DebugParameters::BREAK as u32) != 0 {
            self.brk(0);
        }
    }

    pub fn bc(&mut self, imm19: impl BranchTarget, cond: Condition) {
        let imm19 = imm19.branch_instruction_offset(self);
        let instruction = BC_cond | Assembler::imm_cond_branch(imm19) | (cond as Instr);
        self.emit(instruction);
    }

    pub fn ctz(&mut self, rd: Register, rn: Register) {
        debug_assert!(self.base.is_enabled(CpuFeature::CSSC));
        debug_assert!(rd.is_same_size_and_type(*rn));

        let instruction =
            0x5ac01800 | Assembler::sf(rd) | Assembler::rd(rd) | Assembler::rn(rn);
        self.emit(instruction);
    }

    pub fn align(&mut self, m: i32) {
        debug_assert_eq!(self.pc_offset() % kInstrSize as i32, 0);
        debug_assert!(m >= kInstrSize as i32 && crate::base::bits::is_power_of_two(m as u32));
        while (self.pc_offset() & (m - 1)) != 0 {
            self.nop();
        }
    }

    pub fn dc32(&mut self, data: u32) {
        self.base.emit_bytes(&data.to_le_bytes());
    }

    pub fn dc64(&mut self, data: u64) {
        self.base.emit_bytes(&data.to_le_bytes());
    }

    pub fn dd(&mut self, data: u32) {
        self.block_pools();
        self.dc32(data);
        self.release_pools();
    }

    pub fn dq(&mut self, data: u64) {
        self.block_pools();
        self.dc64(data);
        self.release_pools();
    }

    pub fn set_options(&mut self, options: crate::codegen::assembler::AssemblerOptions) {
        self.base.set_options(options);
    }

    pub fn enable_cpu_feature(&mut self, feature: CpuFeature) {
        self.base.enable_cpu_feature(feature);
    }

    pub fn reloc_info(&self) -> &[u8] {
        self.reloc_info_writer.buffer()
    }

    pub fn internal_reference_positions(&self) -> &[i32] {
        &self.internal_reference_positions
    }

    fn should_record_reloc_info(&self, rmode: Mode) -> bool {
        debug_assert!(rmode != Mode::NoInfo);
        true
    }

    fn is_unresolved_internal_reference(&self, offset: i32) -> bool {
        if offset + 2 * kInstrSize as i32 > self.pc_offset() {
            return false;
        }
        self.instruction_at(offset).is_brk()
            && self.instruction_at(offset + kInstrSize as i32).is_brk()
    }

    fn imm_unresolved_internal_reference(&self, offset: i32) -> i32 {
        debug_assert!(self.is_unresolved_internal_reference(offset));
        let high16 = self.instruction_at(offset).imm_exception() as i32;
        let low16 = self
            .instruction_at(offset + kInstrSize as i32)
            .imm_exception() as i32;
        ((high16 << 16) | low16) * kInstrSize as i32
    }

    fn set_unresolved_internal_reference_imm_target(&mut self, offset: i32, distance: i32) {
        debug_assert!(self.is_unresolved_internal_reference(offset));
        debug_assert_eq!(distance % kInstrSize as i32, 0);
        let target_offset = distance / kInstrSize as i32;
        let high16 = crate::codegen::arm64::instructions_arm64::unsigned_bitextract_32(
            31,
            16,
            target_offset as u32,
        );
        let low16 = crate::codegen::arm64::instructions_arm64::unsigned_bitextract_32(
            15,
            0,
            target_offset as u32,
        );
        let high = BRK | Assembler::imm_exception(high16 as i32);
        let low = BRK | Assembler::imm_exception(low16 as i32);
        self.base.write_bytes_at(offset, &high.to_le_bytes());
        self.base
            .write_bytes_at(offset + kInstrSize as i32, &low.to_le_bytes());
    }

    pub fn record_reloc_info(
        &mut self,
        rmode: Mode,
        data: i64,
        constant_pool_mode: ConstantPoolMode,
    ) {
        if rmode == Mode::InternalReference
            || rmode == Mode::ConstPool
            || rmode == Mode::VeneerPool
            || rmode == Mode::DeoptScriptOffset
            || rmode == Mode::DeoptInliningId
            || rmode == Mode::DeoptReason
            || rmode == Mode::DeoptId
            || rmode == Mode::DeoptNodeId
        {
            debug_assert!(
                RelocInfo::is_deopt_reason(rmode)
                    || RelocInfo::is_deopt_id(rmode)
                    || RelocInfo::is_deopt_node_id(rmode)
                    || RelocInfo::is_deopt_position(rmode)
                    || RelocInfo::is_internal_reference(rmode)
                    || RelocInfo::is_const_pool(rmode)
                    || RelocInfo::is_veneer_pool(rmode)
            );
        } else if constant_pool_mode == ConstantPoolMode::NeedsPoolEntry {
            let pc_offset = self.pc_offset();
            let status = if rmode == Mode::CompressedEmbeddedObject {
                self.constpool
                    .record_entry(data as u32, rmode, pc_offset)
            } else {
                self.constpool
                    .record_entry(data as u64, rmode, pc_offset)
            };
            if status == RelocInfoStatus::MustOmitForDuplicate {
                return;
            }
        }

        if !self.should_record_reloc_info(rmode) {
            return;
        }

        debug_assert!(self.constpool.is_blocked());

        let rinfo = RelocInfo::new(self.pc_offset(), rmode, data);
        self.reloc_info_writer.write(&rinfo);
    }

    pub fn record_const_pool(&mut self, size: i32) {
        self.block_pools();
        self.record_reloc_info(Mode::ConstPool, size as i64, ConstantPoolMode::NoPoolEntry);
        self.release_pools();
    }

    pub fn record_veneer_pool(&mut self, location_offset: i32, size: i32) {
        self.constpool.start_block();
        self.start_block_veneer_pool();
        let rinfo = RelocInfo::new(location_offset, Mode::VeneerPool, size as i64);
        self.reloc_info_writer.write(&rinfo);
        self.end_block_veneer_pool();
        self.constpool.end_block();
    }

    pub fn near_jump(&mut self, offset: i32, rmode: Mode) {
        self.block_pools();
        if rmode != Mode::NoInfo {
            self.record_reloc_info(rmode, offset as i64, ConstantPoolMode::NoPoolEntry);
        }
        self.b(offset);
        self.release_pools();
    }

    pub fn near_call(&mut self, offset: i32, rmode: Mode) {
        self.block_pools();
        if rmode != Mode::NoInfo {
            self.record_reloc_info(rmode, offset as i64, ConstantPoolMode::NoPoolEntry);
        }
        self.bl(offset);
        self.release_pools();
    }

    pub fn dcptr(&mut self, label: LabelId) {
        self.block_pools();
        self.record_reloc_info(
            Mode::InternalReference,
            0,
            ConstantPoolMode::NoPoolEntry,
        );
        if self.label(label).is_bound() {
            let position = self.pc_offset();
            self.internal_reference_positions.push(position);
            let target = self.label(label).pos() as u64;
            self.dc64(target);
        } else {
            let offset = self.link_and_get_branch_instruction_offset_to(label);

            let high16 = crate::codegen::arm64::instructions_arm64::unsigned_bitextract_32(31, 16, offset as u32);
            let low16 = crate::codegen::arm64::instructions_arm64::unsigned_bitextract_32(15, 0, offset as u32);

            self.brk(high16 as i32);
            self.brk(low16 as i32);
        }
        self.release_pools();
    }

    pub fn target_pointer_address_at(&self, pc: i32) -> i32 {
        let instr = self.instruction_at(pc);
        debug_assert!(instr.is_ldr_literal_x() || instr.is_ldr_literal_w());
        pc + instr.imm_pc_offset() as i32
    }

    pub fn set_embedded_object_index_referenced_from(&mut self, pc: i32, data: u64) {
        let instr = self.instruction_at(pc);
        let slot = self.target_pointer_address_at(pc);
        if instr.is_ldr_literal_x() {
            self.base.write_bytes_at(slot, &data.to_le_bytes());
        } else {
            debug_assert!(instr.is_ldr_literal_w());
            debug_assert!(crate::utils::utils::is_uint32(data as i64));
            self.base
                .write_bytes_at(slot, &(data as u32).to_le_bytes());
        }
    }

    pub fn embedded_object_index_referenced_from(&self, pc: i32) -> u64 {
        let instr = self.instruction_at(pc);
        let slot = self.target_pointer_address_at(pc) as usize;
        let bytes = self.buffer_start();
        if instr.is_ldr_literal_x() {
            let mut word = [0u8; 8];
            word.copy_from_slice(&bytes[slot..slot + 8]);
            u64::from_le_bytes(word)
        } else {
            debug_assert!(instr.is_ldr_literal_w());
            let mut word = [0u8; 4];
            word.copy_from_slice(&bytes[slot..slot + 4]);
            u32::from_le_bytes(word) as u64
        }
    }

    pub fn set_target_address_at(&mut self, pc: i32, target: i32) {
        let mut instr = self.instruction_at(pc);
        if instr.is_ldr_literal_x() {
            let slot = self.target_pointer_address_at(pc);
            self.base
                .write_bytes_at(slot, &(target as u64).to_le_bytes());
        } else {
            debug_assert!(instr.is_branch_and_link() || instr.is_unconditional_branch());
            let target = if target == 0 { pc } else { target };
            instr.set_branch_imm_target(
                ImmBranchType::UncondBranchType,
                (target - pc) as i64,
            );
            self.set_instruction_at(pc, instr);
        }
    }

    pub fn set_target_compressed_address_at(&mut self, pc: i32, target: u32) {
        let instr = self.instruction_at(pc);
        assert!(instr.is_ldr_literal_w());
        let slot = self.target_pointer_address_at(pc);
        self.base.write_bytes_at(slot, &target.to_le_bytes());
    }

    pub fn set_uint32_constant_at(&mut self, pc: i32, new_constant: u32) {
        let instr = self.instruction_at(pc);
        assert!(instr.is_ldr_literal_w());
        let slot = self.target_pointer_address_at(pc);
        self.base.write_bytes_at(slot, &new_constant.to_le_bytes());
    }

    pub fn deserialization_set_target_internal_reference_at(
        &mut self,
        pc: i32,
        target: u64,
    ) {
        self.base.write_bytes_at(pc, &target.to_le_bytes());
    }

    pub fn block_pools(&mut self) {
        self.constpool.start_block();
    }

    pub fn release_pools(&mut self) {
        self.constpool.end_block();
    }

    pub fn check_buffer(&mut self) {
        if self.pc_offset() >= self.next_veneer_pool_check {
            self.check_veneer_pool(false, true, Assembler::kVeneerDistanceMargin);
        }
        if self.constpool.is_blocked() {
            return;
        }
        let mut constpool = std::mem::take(&mut self.constpool);
        constpool.maybe_check(self);
        self.constpool = constpool;
    }

    pub fn emit_const_pool_with_jump_if_needed(&mut self, margin: usize) {
        use crate::codegen::arm64::constant_pool_arm64::{Emission, Jump};
        if self.constpool.is_empty() {
            return;
        }
        let mut constpool = std::mem::take(&mut self.constpool);
        constpool.check(self, Emission::IfNeeded, Jump::Required, margin);
        self.constpool = constpool;
    }

    pub fn force_constant_pool_emission(&mut self) {
        use crate::codegen::arm64::constant_pool_arm64::{Emission, Jump};
        let mut constpool = std::mem::take(&mut self.constpool);
        constpool.check(self, Emission::Forced, Jump::Required, 0);
        self.constpool = constpool;
    }

    pub fn constant_pool_is_empty(&self) -> bool {
        self.constpool.is_empty()
    }

    pub fn add_sub_for_macro(
        &mut self,
        rd: Register,
        rn: Register,
        operand: Operand,
        s: FlagsUpdate,
        op: Instr,
    ) {
        self.add_sub(rd, rn, operand, s, op);
    }

    pub fn logical_for_macro(
        &mut self,
        rd: Register,
        rn: Register,
        operand: Operand,
        op: Instr,
    ) {
        self.logical(rd, rn, operand, op);
    }

    pub fn logical_immediate_for_macro(
        &mut self,
        rd: Register,
        rn: Register,
        n: u32,
        imm_s: u32,
        imm_r: u32,
        op: Instr,
    ) {
        self.logical_immediate(rd, rn, n, imm_s, imm_r, op);
    }

    pub fn load_store_for_macro(&mut self, rt: impl Into<CPURegister>, addr: MemOperand, op: Instr) {
        self.load_store(rt, addr, op);
    }

    pub fn load_op_for_macro(rt: CPURegister) -> Instr {
        Assembler::load_op_for(rt)
    }

    pub fn store_op_for_macro(rt: CPURegister) -> Instr {
        Assembler::store_op_for(rt)
    }

    pub fn calc_ls_data_size_log2_for_macro(op: Instr) -> u32 {
        Assembler::calc_ls_data_size_log2(op)
    }

    fn rs(rs: impl Into<CPURegister>) -> Instr {
        let rs = rs.into();
        debug_assert_ne!(rs.code(), kSPRegInternalCode);
        (rs.code() as Instr) << Rs_offset
    }

    pub fn are_even(reg1: Register, reg2: Register) -> bool {
        reg1.is_even() && reg2.is_even()
    }

    pub fn are_consecutive(reg1: Register, reg2: Register) -> bool {
        (reg1.code() + 1) % kNumberOfRegisters == reg2.code()
    }

    pub fn ldar(&mut self, rt: Register, rn: Register) {
        debug_assert!(rn.is_64_bits());
        let op = if rt.is_32_bits() { LDAR_w } else { LDAR_x };
        let instruction = op
            | Assembler::rs(x31)
            | Assembler::rt2(x31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn ldaxr(&mut self, rt: Register, rn: Register) {
        debug_assert!(rn.is_64_bits());
        let op = if rt.is_32_bits() { LDAXR_w } else { LDAXR_x };
        let instruction = op
            | Assembler::rs(x31)
            | Assembler::rt2(x31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn stlr(&mut self, rt: Register, rn: Register) {
        debug_assert!(rn.is_64_bits());
        let op = if rt.is_32_bits() { STLR_w } else { STLR_x };
        let instruction = op
            | Assembler::rs(x31)
            | Assembler::rt2(x31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn stlxr(&mut self, rs: Register, rt: Register, rn: Register) {
        debug_assert!(rn.is_64_bits());
        debug_assert!(rs != rt && rs != rn);
        let op = if rt.is_32_bits() { STLXR_w } else { STLXR_x };
        let instruction = op
            | Assembler::rs(rs)
            | Assembler::rt2(x31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn ldarb(&mut self, rt: Register, rn: Register) {
        debug_assert!(rt.is_32_bits());
        debug_assert!(rn.is_64_bits());
        let instruction = LDAR_b
            | Assembler::rs(x31)
            | Assembler::rt2(x31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn ldaxrb(&mut self, rt: Register, rn: Register) {
        debug_assert!(rt.is_32_bits());
        debug_assert!(rn.is_64_bits());
        let instruction = LDAXR_b
            | Assembler::rs(x31)
            | Assembler::rt2(x31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn stlrb(&mut self, rt: Register, rn: Register) {
        debug_assert!(rt.is_32_bits());
        debug_assert!(rn.is_64_bits());
        let instruction = STLR_b
            | Assembler::rs(x31)
            | Assembler::rt2(x31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn stlxrb(&mut self, rs: Register, rt: Register, rn: Register) {
        debug_assert!(rs.is_32_bits());
        debug_assert!(rt.is_32_bits());
        debug_assert!(rn.is_64_bits());
        debug_assert!(rs != rt && rs != rn);
        let instruction = STLXR_b
            | Assembler::rs(rs)
            | Assembler::rt2(x31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn ldarh(&mut self, rt: Register, rn: Register) {
        debug_assert!(rt.is_32_bits());
        debug_assert!(rn.is_64_bits());
        let instruction = LDAR_h
            | Assembler::rs(x31)
            | Assembler::rt2(x31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn ldaxrh(&mut self, rt: Register, rn: Register) {
        debug_assert!(rt.is_32_bits());
        debug_assert!(rn.is_64_bits());
        let instruction = LDAXR_h
            | Assembler::rs(x31)
            | Assembler::rt2(x31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn stlrh(&mut self, rt: Register, rn: Register) {
        debug_assert!(rt.is_32_bits());
        debug_assert!(rn.is_64_bits());
        let instruction = STLR_h
            | Assembler::rs(x31)
            | Assembler::rt2(x31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn stlxrh(&mut self, rs: Register, rt: Register, rn: Register) {
        debug_assert!(rs.is_32_bits());
        debug_assert!(rt.is_32_bits());
        debug_assert!(rn.is_64_bits());
        debug_assert!(rs != rt && rs != rn);
        let instruction = STLXR_h
            | Assembler::rs(rs)
            | Assembler::rt2(x31)
            | Assembler::rn_sp(rn)
            | Assembler::rt(rt);
        self.emit(instruction);
    }

    fn v_format(vd: VRegister) -> Instr {
        if vd.is_64_bits() {
            return match vd.lane_count() {
                1 => NEON_1D,
                2 => NEON_2S,
                4 => NEON_4H,
                8 => NEON_8B,
                _ => unreachable!(),
            };
        }
        debug_assert!(vd.is_128_bits());
        match vd.lane_count() {
            2 => NEON_2D,
            4 => NEON_4S,
            8 => NEON_8H,
            16 => NEON_16B,
            _ => unreachable!(),
        }
    }

    fn s_format(vd: VRegister) -> Instr {
        debug_assert!(vd.is_scalar());
        match vd.size_in_bytes() {
            1 => NEON_B,
            2 => NEON_H,
            4 => NEON_S,
            8 => NEON_D,
            _ => unreachable!(),
        }
    }

    fn neon_3_different_l(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        vm: VRegister,
        vop: Instr,
    ) {
        debug_assert!(are_same_format(vn, &[vm]));
        let format;
        let mut op = vop;
        if vd.is_scalar() {
            op |= NEON_Q | NEONScalar;
            format = Assembler::s_format(vn);
        } else {
            format = Assembler::v_format(vn);
        }
        let instruction = format
            | op
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_3_different_w(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        vm: VRegister,
        vop: Instr,
    ) {
        debug_assert!(are_same_format(vd, &[vn]));
        let instruction = Assembler::v_format(vm)
            | vop
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_3_different_hn(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        vm: VRegister,
        vop: Instr,
    ) {
        debug_assert!(are_same_format(vm, &[vn]));
        let instruction = Assembler::v_format(vd)
            | vop
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_perm(&mut self, vd: VRegister, vn: VRegister, vm: VRegister, op: Instr) {
        debug_assert!(are_same_format(vd, &[vn, vm]));
        debug_assert!(!vd.is_1d());
        let instruction = Assembler::v_format(vd)
            | op
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn imm_barrier_domain(imm2: u32) -> Instr {
        debug_assert!(is_uint2(imm2 as i64));
        imm2 << ImmBarrierDomain_offset
    }

    fn imm_barrier_type(imm2: u32) -> Instr {
        debug_assert!(is_uint2(imm2 as i64));
        imm2 << ImmBarrierType_offset
    }

    fn imm_system_register(imm15: u32) -> Instr {
        debug_assert!(crate::utils::utils::is_uintn(imm15 as i64, 15));
        imm15 << ImmSystemRegister_offset
    }

    pub fn dmb(&mut self, domain: BarrierDomain, barrier_type: BarrierType) {
        let instruction = DMB
            | Assembler::imm_barrier_domain(domain as u32)
            | Assembler::imm_barrier_type(barrier_type as u32);
        self.emit(instruction);
    }

    pub fn dsb(&mut self, domain: BarrierDomain, barrier_type: BarrierType) {
        let instruction = DSB
            | Assembler::imm_barrier_domain(domain as u32)
            | Assembler::imm_barrier_type(barrier_type as u32);
        self.emit(instruction);
    }

    pub fn isb(&mut self) {
        let instruction = ISB
            | Assembler::imm_barrier_domain(BarrierDomain::FullSystem as u32)
            | Assembler::imm_barrier_type(BarrierType::BarrierAll as u32);
        self.emit(instruction);
    }

    pub fn csdb(&mut self) {
        self.hint(SystemHint::Csdb);
    }

    pub fn mrs(&mut self, rt: Register, sysreg: SystemRegister) {
        debug_assert!(rt.is_64_bits());
        let instruction =
            MRS | Assembler::imm_system_register(sysreg as u32) | Assembler::rt(rt);
        self.emit(instruction);
    }

    pub fn msr(&mut self, sysreg: SystemRegister, rt: Register) {
        debug_assert!(rt.is_64_bits());
        let instruction =
            MSR | Assembler::rt(rt) | Assembler::imm_system_register(sysreg as u32);
        self.emit(instruction);
    }

    pub fn pacib1716(&mut self) {
        self.emit(PACIB1716);
    }

    pub fn autib1716(&mut self) {
        self.emit(AUTIB1716);
    }

    pub fn pacibsp(&mut self) {
        self.emit(PACIBSP);
    }

    pub fn autibsp(&mut self) {
        self.emit(AUTIBSP);
    }

    pub fn bti(&mut self, id: BranchTargetIdentifier) {
        let op = match id {
            BranchTargetIdentifier::Bti => SystemHint::Bti,
            BranchTargetIdentifier::BtiCall => SystemHint::BtiC,
            BranchTargetIdentifier::BtiJump => SystemHint::BtiJ,
            BranchTargetIdentifier::BtiJumpCall => SystemHint::BtiJc,
            BranchTargetIdentifier::Pacibsp | BranchTargetIdentifier::None => unreachable!(),
        };
        self.hint(op);
    }

    fn neon_2_reg_misc(&mut self, vd: VRegister, vn: VRegister, vop: Instr) {
        debug_assert!(are_same_format(vd, &[vn]));
        let format;
        let mut op = vop;
        if vd.is_scalar() {
            op |= NEON_Q | NEONScalar;
            format = Assembler::s_format(vd);
        } else {
            format = Assembler::v_format(vd);
        }
        let instruction = format | op | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_across_lanes(&mut self, vd: VRegister, vn: VRegister, op: Instr) {
        debug_assert!(
            (vn.is_8b() && vd.is_1b())
                || (vn.is_16b() && vd.is_1b())
                || (vn.is_4h() && vd.is_1h())
                || (vn.is_8h() && vd.is_1h())
                || (vn.is_4s() && vd.is_1s())
        );
        let instruction = if (op & NEONAcrossLanesFPFMask) == NEONAcrossLanesFPFixed {
            Assembler::fp_format(vn) | op | Assembler::rn(vn) | Assembler::rd(vd)
        } else {
            Assembler::v_format(vn) | op | Assembler::rn(vn) | Assembler::rd(vd)
        };
        self.emit(instruction);
    }

    fn fp_format(vd: VRegister) -> Instr {
        if vd.lane_count() == 1 {
            debug_assert!(vd.is_32_bits() || vd.is_64_bits());
            return if vd.is_64_bits() { FP64 } else { FP32 };
        }
        if vd.lane_count() == 2 {
            debug_assert!(vd.is_64_bits() || vd.is_128_bits());
            return if vd.is_128_bits() { NEON_FP_2D } else { NEON_FP_2S };
        }
        if vd.lane_count() == 4 {
            debug_assert!(vd.is_64_bits() || vd.is_128_bits());
            return if vd.is_128_bits() { NEON_FP_4S } else { NEON_FP_4H };
        }
        debug_assert!((vd.lane_count() == 8) && vd.is_128_bits());
        NEON_FP_8H
    }

    fn imm_neon_hlm(index: i32, num_bits: i32) -> Instr {
        let h;
        let l;
        let m;
        if num_bits == 3 {
            debug_assert!(crate::utils::utils::is_uintn(index as i64, 3));
            h = (index >> 2) & 1;
            l = (index >> 1) & 1;
            m = index & 1;
        } else if num_bits == 2 {
            debug_assert!(is_uint2(index as i64));
            h = (index >> 1) & 1;
            l = index & 1;
            m = 0;
        } else {
            debug_assert!(is_uint1(index as i64) && (num_bits == 1));
            h = index & 1;
            l = 0;
            m = 0;
        }
        ((h as Instr) << NEONH_offset)
            | ((l as Instr) << NEONL_offset)
            | ((m as Instr) << NEONM_offset)
    }

    fn neon_by_element(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        vm: VRegister,
        vm_index: i32,
        vop: Instr,
    ) {
        debug_assert!(are_same_format(vd, &[vn]));
        debug_assert!(
            (vd.is_4h() && vm.is_1h())
                || (vd.is_8h() && vm.is_1h())
                || (vd.is_1h() && vm.is_1h())
                || (vd.is_2s() && vm.is_1s())
                || (vd.is_4s() && vm.is_1s())
                || (vd.is_1s() && vm.is_1s())
        );
        debug_assert!(
            (vm.is_1h() && (vm.code() < 16) && (vm_index < 8))
                || (vm.is_1s() && (vm_index < 4))
        );

        let format;
        let mut op = vop;
        let index_num_bits = if vm.is_1h() { 3 } else { 2 };
        if vd.is_scalar() {
            op |= NEONScalar | NEON_Q;
            format = Assembler::s_format(vn);
        } else {
            format = Assembler::v_format(vn);
        }
        let instruction = format
            | op
            | Assembler::imm_neon_hlm(vm_index, index_num_bits)
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_by_element_l(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        vm: VRegister,
        vm_index: i32,
        vop: Instr,
    ) {
        debug_assert!(
            (vd.is_4s() && vn.is_4h() && vm.is_1h())
                || (vd.is_4s() && vn.is_8h() && vm.is_1h())
                || (vd.is_1s() && vn.is_1h() && vm.is_1h())
                || (vd.is_2d() && vn.is_2s() && vm.is_1s())
                || (vd.is_2d() && vn.is_4s() && vm.is_1s())
                || (vd.is_1d() && vn.is_1s() && vm.is_1s())
        );
        debug_assert!(
            (vm.is_1h() && (vm.code() < 16) && (vm_index < 8))
                || (vm.is_1s() && (vm_index < 4))
        );

        let format;
        let mut op = vop;
        let index_num_bits = if vm.is_1h() { 3 } else { 2 };
        if vd.is_scalar() {
            op |= NEONScalar | NEON_Q;
            format = Assembler::s_format(vn);
        } else {
            format = Assembler::v_format(vn);
        }
        let instruction = format
            | op
            | Assembler::imm_neon_hlm(vm_index, index_num_bits)
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_fp_by_element(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        vm: VRegister,
        vm_index: i32,
        vop: Instr,
    ) {
        debug_assert!(are_same_format(vd, &[vn]));
        debug_assert!(
            (vd.is_2s() && vm.is_1s())
                || (vd.is_4s() && vm.is_1s())
                || (vd.is_1s() && vm.is_1s())
                || (vd.is_2d() && vm.is_1d())
                || (vd.is_1d() && vm.is_1d())
        );
        debug_assert!((vm.is_1s() && (vm_index < 4)) || (vm.is_1d() && (vm_index < 2)));

        let mut op = vop;
        let index_num_bits = if vm.is_1s() { 2 } else { 1 };
        if vd.is_scalar() {
            op |= NEON_Q | NEONScalar;
        }
        let instruction = Assembler::fp_format(vd)
            | op
            | Assembler::imm_neon_hlm(vm_index, index_num_bits)
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_shift_immediate(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        op: Instr,
        immh_immb: i32,
    ) {
        debug_assert!(are_same_format(vd, &[vn]));
        let q;
        let scalar;
        if vn.is_scalar() {
            q = NEON_Q;
            scalar = NEONScalar;
        } else {
            q = if vd.is_d() { 0 } else { NEON_Q };
            scalar = 0;
        }
        let instruction = q
            | op
            | scalar
            | (immh_immb as Instr)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_shift_left_immediate(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        shift: i32,
        op: Instr,
    ) {
        let lane_size_in_bits = vn.lane_size_in_bits() as i32;
        debug_assert!((shift >= 0) && (shift < lane_size_in_bits));
        self.neon_shift_immediate(vd, vn, op, (lane_size_in_bits + shift) << 16);
    }

    fn neon_shift_right_immediate(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        shift: i32,
        op: Instr,
    ) {
        let lane_size_in_bits = vn.lane_size_in_bits() as i32;
        debug_assert!((shift >= 1) && (shift <= lane_size_in_bits));
        self.neon_shift_immediate(vd, vn, op, ((2 * lane_size_in_bits) - shift) << 16);
    }

    fn neon_shift_immediate_l(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        shift: i32,
        op: Instr,
    ) {
        let lane_size_in_bits = vn.lane_size_in_bits() as i32;
        debug_assert!((shift >= 0) && (shift < lane_size_in_bits));
        let immh_immb = (lane_size_in_bits + shift) << 16;

        debug_assert!(
            (vn.is_8b() && vd.is_8h())
                || (vn.is_4h() && vd.is_4s())
                || (vn.is_2s() && vd.is_2d())
                || (vn.is_16b() && vd.is_8h())
                || (vn.is_8h() && vd.is_4s())
                || (vn.is_4s() && vd.is_2d())
        );
        let q = if vn.is_d() { 0 } else { NEON_Q };
        let instruction = q
            | op
            | (immh_immb as Instr)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_shift_immediate_n(
        &mut self,
        vd: VRegister,
        vn: VRegister,
        shift: i32,
        op: Instr,
    ) {
        let q;
        let scalar;
        let lane_size_in_bits = vd.lane_size_in_bits() as i32;
        debug_assert!((shift >= 1) && (shift <= lane_size_in_bits));
        let immh_immb = (2 * lane_size_in_bits - shift) << 16;

        if vn.is_scalar() {
            debug_assert!(
                (vd.is_1b() && vn.is_1h())
                    || (vd.is_1h() && vn.is_1s())
                    || (vd.is_1s() && vn.is_1d())
            );
            q = NEON_Q;
            scalar = NEONScalar;
        } else {
            debug_assert!(
                (vd.is_8b() && vn.is_8h())
                    || (vd.is_4h() && vn.is_4s())
                    || (vd.is_2s() && vn.is_2d())
                    || (vd.is_16b() && vn.is_8h())
                    || (vd.is_8h() && vn.is_4s())
                    || (vd.is_4s() && vn.is_2d())
            );
            scalar = 0;
            q = if vd.is_d() { 0 } else { NEON_Q };
        }
        let instruction = q
            | op
            | scalar
            | (immh_immb as Instr)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_fp_3_same(&mut self, vd: VRegister, vn: VRegister, vm: VRegister, op: Instr) {
        debug_assert!(are_same_format(vd, &[vn, vm]));
        let mut op = op;
        if vd.is_4h() || vd.is_8h() {
            op |= NEON_sz;
            op ^= NEON3SameHPMask;
        }
        let instruction = Assembler::fp_format(vd)
            | op
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_fp_2_reg_misc_zero(&mut self, vd: VRegister, vn: VRegister, vop: Instr) {
        debug_assert!(are_same_format(vd, &[vn]));
        let mut op = vop;
        if vd.is_scalar() {
            debug_assert!(vd.is_1s() || vd.is_1d());
            op |= NEON_Q | NEONScalar;
        } else if vd.is_4h() || vd.is_8h() {
            op |= NEON_sz | NEON2RegMiscHPFixed;
        } else {
            debug_assert!(vd.is_2s() || vd.is_2d() || vd.is_4s());
        }
        let instruction =
            Assembler::fp_format(vd) | op | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_fp_2_reg_misc(&mut self, vd: VRegister, vn: VRegister, op: Instr) {
        debug_assert!(are_same_format(vd, &[vn]));
        let instruction =
            Assembler::fp_format(vd) | op | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_xtn(&mut self, vd: VRegister, vn: VRegister, vop: Instr) {
        let format;
        let mut op = vop;
        if vd.is_scalar() {
            debug_assert!(
                (vd.is_1b() && vn.is_1h())
                    || (vd.is_1h() && vn.is_1s())
                    || (vd.is_1s() && vn.is_1d())
            );
            op |= NEON_Q | NEONScalar;
            format = Assembler::s_format(vd);
        } else {
            debug_assert!(
                (vd.is_8b() && vn.is_8h())
                    || (vd.is_4h() && vn.is_4s())
                    || (vd.is_2s() && vn.is_2d())
                    || (vd.is_16b() && vn.is_8h())
                    || (vd.is_8h() && vn.is_4s())
                    || (vd.is_4s() && vn.is_2d())
            );
            format = Assembler::v_format(vd);
        }
        let instruction = format | op | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_across_lanes_l(&mut self, vd: VRegister, vn: VRegister, op: Instr) {
        debug_assert!(
            (vn.is_8b() && vd.is_1h())
                || (vn.is_16b() && vd.is_1h())
                || (vn.is_4h() && vd.is_1s())
                || (vn.is_8h() && vd.is_1s())
                || (vn.is_4s() && vd.is_1d())
        );
        let instruction =
            Assembler::v_format(vn) | op | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_table(&mut self, vd: VRegister, vn: VRegister, vm: VRegister, op: Instr) {
        debug_assert!(vd.is_16b() || vd.is_8b());
        debug_assert!(vn.is_16b());
        debug_assert!(are_same_format(vd, &[vm]));
        let instruction = op
            | (if vd.is_q() { NEON_Q } else { 0 })
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn tbl(&mut self, vd: VRegister, vn: VRegister, vm: VRegister) {
        self.neon_table(vd, vn, vm, NEON_TBL_1v);
    }

    pub fn tbx(&mut self, vd: VRegister, vn: VRegister, vm: VRegister) {
        self.neon_table(vd, vn, vm, NEON_TBX_1v);
    }

    pub fn shll(&mut self, vd: VRegister, vn: VRegister, shift: i32) {
        debug_assert!(
            (vd.is_8h() && vn.is_8b() && shift == 8)
                || (vd.is_4s() && vn.is_4h() && shift == 16)
                || (vd.is_2d() && vn.is_2s() && shift == 32)
        );
        let instruction =
            Assembler::v_format(vn) | NEON_SHLL | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn shll2(&mut self, vd: VRegister, vn: VRegister, shift: i32) {
        debug_assert!(
            (vd.is_8h() && vn.is_16b() && shift == 8)
                || (vd.is_4s() && vn.is_8h() && shift == 16)
                || (vd.is_2d() && vn.is_4s() && shift == 32)
        );
        let instruction =
            Assembler::v_format(vn) | NEON_SHLL | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn sxtl(&mut self, vd: VRegister, vn: VRegister) {
        self.sshll(vd, vn, 0);
    }

    pub fn sxtl2(&mut self, vd: VRegister, vn: VRegister) {
        self.sshll2(vd, vn, 0);
    }

    pub fn uxtl(&mut self, vd: VRegister, vn: VRegister) {
        self.ushll(vd, vn, 0);
    }

    pub fn uxtl2(&mut self, vd: VRegister, vn: VRegister) {
        self.ushll2(vd, vn, 0);
    }

    pub fn fcvtl(&mut self, vd: VRegister, vn: VRegister) {
        debug_assert!((vd.is_4s() && vn.is_4h()) || (vd.is_2d() && vn.is_2s()));
        let format = if vd.is_2d() { 1 << NEONSize_offset } else { 0 };
        let instruction = format | NEON_FCVTL | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn fcvtl2(&mut self, vd: VRegister, vn: VRegister) {
        debug_assert!((vd.is_4s() && vn.is_8h()) || (vd.is_2d() && vn.is_4s()));
        let format = if vd.is_2d() { 1 << NEONSize_offset } else { 0 };
        let instruction =
            NEON_Q | format | NEON_FCVTL | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn fcvtn(&mut self, vd: VRegister, vn: VRegister) {
        debug_assert!((vn.is_4s() && vd.is_4h()) || (vn.is_2d() && vd.is_2s()));
        let format = if vn.is_2d() { 1 << NEONSize_offset } else { 0 };
        let instruction = format | NEON_FCVTN | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn fcvtn2(&mut self, vd: VRegister, vn: VRegister) {
        debug_assert!((vn.is_4s() && vd.is_8h()) || (vn.is_2d() && vd.is_4s()));
        let format = if vn.is_2d() { 1 << NEONSize_offset } else { 0 };
        let instruction =
            NEON_Q | format | NEON_FCVTN | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn fcvtxn(&mut self, vd: VRegister, vn: VRegister) {
        let format = 1 << NEONSize_offset;
        if vd.is_scalar() {
            debug_assert!(vd.is_1s() && vn.is_1d());
            let instruction =
                format | NEON_FCVTXN_scalar | Assembler::rn(vn) | Assembler::rd(vd);
            self.emit(instruction);
        } else {
            debug_assert!(vd.is_2s() && vn.is_2d());
            let instruction = format | NEON_FCVTXN | Assembler::rn(vn) | Assembler::rd(vd);
            self.emit(instruction);
        }
    }

    pub fn fcvtxn2(&mut self, vd: VRegister, vn: VRegister) {
        debug_assert!(vd.is_4s() && vn.is_2d());
        let format = 1 << NEONSize_offset;
        let instruction =
            NEON_Q | format | NEON_FCVTXN | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn not_(&mut self, vd: VRegister, vn: VRegister) {
        debug_assert!(are_same_format(vd, &[vn]));
        debug_assert!(vd.is_8b() || vd.is_16b());
        let instruction =
            Assembler::v_format(vd) | NEON_RBIT_NOT | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn saddlv(&mut self, vd: VRegister, vn: VRegister) {
        self.neon_across_lanes_l(vd, vn, NEON_SADDLV);
    }

    pub fn uaddlv(&mut self, vd: VRegister, vn: VRegister) {
        self.neon_across_lanes_l(vd, vn, NEON_UADDLV);
    }

    pub fn urecpe(&mut self, vd: VRegister, vn: VRegister) {
        debug_assert!(are_same_format(vd, &[vn]));
        debug_assert!(vd.is_2s() || vd.is_4s());
        let instruction =
            Assembler::v_format(vn) | NEON_URECPE | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn ursqrte(&mut self, vd: VRegister, vn: VRegister) {
        debug_assert!(are_same_format(vd, &[vn]));
        debug_assert!(vd.is_2s() || vd.is_4s());
        let instruction =
            Assembler::v_format(vn) | NEON_URSQRTE | Assembler::rn(vn) | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn frecpx(&mut self, vd: VRegister, vn: VRegister) {
        debug_assert!(vd.is_scalar());
        debug_assert!(are_same_format(vd, &[vn]));
        debug_assert!(vd.is_1s() || vd.is_1d());
        let instruction = Assembler::fp_format(vd)
            | NEON_FRECPX_scalar
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn pmull(&mut self, vd: VRegister, vn: VRegister, vm: VRegister) {
        debug_assert!(are_same_format(vn, &[vm]));
        debug_assert!((vn.is_8b() && vd.is_8h()) || (vn.is_1d() && vd.is_1q()));
        let instruction = Assembler::v_format(vn)
            | NEON_PMULL
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn pmull2(&mut self, vd: VRegister, vn: VRegister, vm: VRegister) {
        debug_assert!(are_same_format(vn, &[vm]));
        debug_assert!((vn.is_16b() && vd.is_8h()) || (vn.is_2d() && vd.is_1q()));
        let instruction = Assembler::v_format(vn)
            | NEON_PMULL2
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn fcsel(&mut self, fd: VRegister, fn_: VRegister, fm: VRegister, cond: Condition) {
        debug_assert_eq!(fd.size_in_bits(), fn_.size_in_bits());
        debug_assert_eq!(fd.size_in_bits(), fm.size_in_bits());
        let instruction = Assembler::fp_type(fd)
            | FCSEL
            | Assembler::rm(fm)
            | Assembler::cond(cond)
            | Assembler::rn(fn_)
            | Assembler::rd(fd);
        self.emit(instruction);
    }

    pub fn fccmp(&mut self, fn_: VRegister, fm: VRegister, nzcv: u32, cond: Condition) {
        debug_assert_eq!(fn_.size_in_bits(), fm.size_in_bits());
        let instruction = Assembler::fp_type(fn_)
            | FCCMP
            | Assembler::rm(fm)
            | Assembler::cond(cond)
            | Assembler::rn(fn_)
            | Assembler::nzcv(nzcv);
        self.emit(instruction);
    }

    fn imm_neon_ext(imm4: i32) -> Instr {
        debug_assert!(crate::utils::utils::is_uintn(imm4 as i64, 4));
        (imm4 as Instr) << ImmNEONExt_offset
    }

    fn imm_neon5(format: Instr, index: i32) -> Instr {
        debug_assert!(crate::utils::utils::is_uintn(index as i64, 4));
        let s = Assembler::lane_size_in_bytes_log2_from_neon_format(format) as i32;
        let imm5 = (index << (s + 1)) | (1 << s);
        (imm5 as Instr) << ImmNEON5_offset
    }

    fn imm_neon4(format: Instr, index: i32) -> Instr {
        debug_assert!(crate::utils::utils::is_uintn(index as i64, 4));
        let s = Assembler::lane_size_in_bytes_log2_from_neon_format(format) as i32;
        let imm4 = index << s;
        (imm4 as Instr) << ImmNEON4_offset
    }

    fn lane_size_in_bytes_log2_from_neon_format(format: Instr) -> u32 {
        match format & NEONFormatFieldMask & !NEON_Q {
            NEON_8B => 0,
            NEON_4H => 1,
            NEON_2S => 2,
            NEON_1D => 3,
            _ => unreachable!(),
        }
    }

    fn imm_neon_abcdefgh(imm8: i32) -> Instr {
        debug_assert!(crate::utils::utils::is_uintn(imm8 as i64, 8));
        let mut instruction = (((imm8 >> 5) & 7) as Instr) << ImmNEONabc_offset;
        instruction |= ((imm8 & 0x1f) as Instr) << ImmNEONdefgh_offset;
        instruction
    }

    fn neon_cmode(cmode: i32) -> Instr {
        debug_assert!(crate::utils::utils::is_uintn(cmode as i64, 4));
        (cmode as Instr) << NEONCmode_offset
    }

    fn neon_mod_imm_op(op: i32) -> Instr {
        debug_assert!(is_uint1(op as i64));
        (op as Instr) << NEONModImmOp_offset
    }

    fn neon_format_for_lane_size(lane_size: u32) -> Instr {
        match lane_size {
            1 => NEON_16B,
            2 => NEON_8H,
            4 => NEON_4S,
            _ => {
                debug_assert_eq!(lane_size, 8);
                NEON_2D
            }
        }
    }

    fn neon_modified_imm_shift_lsl(
        &mut self,
        vd: VRegister,
        imm8: i32,
        left_shift: i32,
        op: Instr,
    ) {
        debug_assert!(
            vd.is_8b() || vd.is_16b() || vd.is_4h() || vd.is_8h() || vd.is_2s() || vd.is_4s()
        );
        debug_assert!(
            (left_shift == 0) || (left_shift == 8) || (left_shift == 16) || (left_shift == 24)
        );
        debug_assert!(crate::utils::utils::is_uintn(imm8 as i64, 8));

        let cmode_1;
        let cmode_2;
        let mut cmode_3;
        if vd.is_8b() || vd.is_16b() {
            debug_assert_eq!(op, NEONModifiedImmediate_MOVI);
            cmode_1 = 1;
            cmode_2 = 1;
            cmode_3 = 1;
        } else {
            cmode_1 = (left_shift >> 3) & 1;
            cmode_2 = left_shift >> 4;
            cmode_3 = 0;
            if vd.is_4h() || vd.is_8h() {
                debug_assert!((left_shift == 0) || (left_shift == 8));
                cmode_3 = 1;
            }
        }
        let cmode = (cmode_3 << 3) | (cmode_2 << 2) | (cmode_1 << 1);

        let q = if vd.is_q() { NEON_Q } else { 0 };
        let instruction = q
            | op
            | Assembler::imm_neon_abcdefgh(imm8)
            | Assembler::neon_cmode(cmode)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn neon_modified_imm_shift_msl(
        &mut self,
        vd: VRegister,
        imm8: i32,
        shift_amount: i32,
        op: Instr,
    ) {
        debug_assert!(vd.is_2s() || vd.is_4s());
        debug_assert!((shift_amount == 8) || (shift_amount == 16));
        debug_assert!(crate::utils::utils::is_uintn(imm8 as i64, 8));

        let cmode_0 = (shift_amount >> 4) & 1;
        let cmode = 0xC | cmode_0;

        let q = if vd.is_q() { NEON_Q } else { 0 };
        let instruction = q
            | op
            | Assembler::imm_neon_abcdefgh(imm8)
            | Assembler::neon_cmode(cmode)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn movi(&mut self, vd: VRegister, imm: u64, shift: Shift, shift_amount: i32) {
        debug_assert!((shift == Shift::Lsl) || (shift == Shift::Msl));
        if vd.is_2d() || vd.is_1d() {
            debug_assert_eq!(shift_amount, 0);
            let mut imm8 = 0;
            for i in 0..8 {
                let byte = (imm >> (i * 8)) & 0xFF;
                debug_assert!((byte == 0) || (byte == 0xFF));
                if byte == 0xFF {
                    imm8 |= 1 << i;
                }
            }
            let q = if vd.is_2d() { NEON_Q } else { 0 };
            let instruction = q
                | Assembler::neon_mod_imm_op(1)
                | NEONModifiedImmediate_MOVI
                | Assembler::imm_neon_abcdefgh(imm8)
                | Assembler::neon_cmode(0xE)
                | Assembler::rd(vd);
            self.emit(instruction);
        } else if shift == Shift::Lsl {
            debug_assert!(crate::utils::utils::is_uintn(imm as i64, 8));
            self.neon_modified_imm_shift_lsl(
                vd,
                imm as i32,
                shift_amount,
                NEONModifiedImmediate_MOVI,
            );
        } else {
            debug_assert!(crate::utils::utils::is_uintn(imm as i64, 8));
            self.neon_modified_imm_shift_msl(
                vd,
                imm as i32,
                shift_amount,
                NEONModifiedImmediate_MOVI,
            );
        }
    }

    pub fn mvni(&mut self, vd: VRegister, imm8: i32, shift: Shift, shift_amount: i32) {
        debug_assert!((shift == Shift::Lsl) || (shift == Shift::Msl));
        if shift == Shift::Lsl {
            self.neon_modified_imm_shift_lsl(
                vd,
                imm8,
                shift_amount,
                NEONModifiedImmediate_MVNI,
            );
        } else {
            self.neon_modified_imm_shift_msl(
                vd,
                imm8,
                shift_amount,
                NEONModifiedImmediate_MVNI,
            );
        }
    }

    pub fn ins(&mut self, vd: VRegister, vd_index: i32, vn: VRegister, vn_index: i32) {
        debug_assert!(are_same_format(vd, &[vn]));
        let lane_size = vd.lane_size_in_bytes();
        let format = Assembler::neon_format_for_lane_size(lane_size);
        debug_assert!(
            (0 <= vd_index) && (vd_index < lane_count_from_neon_format(format))
        );
        debug_assert!(
            (0 <= vn_index) && (vn_index < lane_count_from_neon_format(format))
        );
        let instruction = NEON_INS_ELEMENT
            | Assembler::imm_neon5(format, vd_index)
            | Assembler::imm_neon4(format, vn_index)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn ins_from_register(&mut self, vd: VRegister, vd_index: i32, rn: Register) {
        let lane_size = vd.lane_size_in_bytes();
        let format = Assembler::neon_format_for_lane_size(lane_size);
        if lane_size == 8 {
            debug_assert!(rn.is_x());
        } else {
            debug_assert!(rn.is_w());
        }
        debug_assert!(
            (0 <= vd_index) && (vd_index < lane_count_from_neon_format(format))
        );
        let instruction = NEON_INS_GENERAL
            | Assembler::imm_neon5(format, vd_index)
            | Assembler::rn(rn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn dup(&mut self, vd: VRegister, vn: VRegister, vn_index: i32) {
        let q;
        let scalar;
        let lane_size = vn.lane_size_in_bytes();
        let format = Assembler::neon_format_for_lane_size(lane_size);

        if vd.is_scalar() {
            q = NEON_Q;
            scalar = NEONScalar;
        } else {
            debug_assert!(!vd.is_1d());
            q = if vd.is_d() { 0 } else { NEON_Q };
            scalar = 0;
        }
        let instruction = q
            | scalar
            | NEON_DUP_ELEMENT
            | Assembler::imm_neon5(format, vn_index)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn dup_from_register(&mut self, vd: VRegister, rn: Register) {
        debug_assert!(!vd.is_1d());
        debug_assert_eq!(vd.is_2d(), rn.is_x());
        let q = if vd.is_d() { 0 } else { NEON_Q };
        let instruction = q
            | NEON_DUP_GENERAL
            | Assembler::imm_neon5(Assembler::v_format(vd), 0)
            | Assembler::rn(rn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn smov(&mut self, rd: Register, vn: VRegister, vn_index: i32) {
        let lane_size = vn.lane_size_in_bytes();
        let format;
        match lane_size {
            1 => format = NEON_16B,
            2 => format = NEON_8H,
            _ => {
                debug_assert_eq!(lane_size, 4);
                debug_assert!(rd.is_x());
                format = NEON_4S;
            }
        }
        let q = if rd.is_w() { 0 } else { NEON_Q };
        debug_assert!(
            (0 <= vn_index) && (vn_index < lane_count_from_neon_format(format))
        );
        let instruction = q
            | NEON_SMOV
            | Assembler::imm_neon5(format, vn_index)
            | Assembler::rn(vn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn umov(&mut self, rd: Register, vn: VRegister, vn_index: i32) {
        let lane_size = vn.lane_size_in_bytes();
        let format;
        let mut q = 0;
        match lane_size {
            1 => {
                format = NEON_16B;
                debug_assert!(rd.is_w());
            }
            2 => {
                format = NEON_8H;
                debug_assert!(rd.is_w());
            }
            4 => {
                format = NEON_4S;
                debug_assert!(rd.is_w());
            }
            _ => {
                debug_assert_eq!(lane_size, 8);
                debug_assert!(rd.is_x());
                format = NEON_2D;
                q = NEON_Q;
            }
        }
        debug_assert!(
            (0 <= vn_index) && (vn_index < lane_count_from_neon_format(format))
        );
        let instruction = q
            | NEON_UMOV
            | Assembler::imm_neon5(format, vn_index)
            | Assembler::rn(vn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    pub fn ext(&mut self, vd: VRegister, vn: VRegister, vm: VRegister, index: i32) {
        debug_assert!(are_same_format(vd, &[vn, vm]));
        debug_assert!(vd.is_8b() || vd.is_16b());
        debug_assert!((0 <= index) && (index < vd.lane_count()));
        let instruction = Assembler::v_format(vd)
            | NEON_EXT
            | Assembler::rm(vm)
            | Assembler::imm_neon_ext(index)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn sdot(&mut self, vd: VRegister, vn: VRegister, vm: VRegister) {
        debug_assert!((vn.is_16b() && vd.is_4s()) || (vn.is_8b() && vd.is_2s()));
        debug_assert!(are_same_format(vn, &[vm]));
        let instruction = Assembler::v_format(vd)
            | NEON_SDOT
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn ls_v_format(vd: VRegister) -> Instr {
        if vd.is_64_bits() {
            return match vd.lane_count() {
                1 => LS_NEON_1D,
                2 => LS_NEON_2S,
                4 => LS_NEON_4H,
                8 => LS_NEON_8B,
                _ => unreachable!(),
            };
        }
        debug_assert!(vd.is_128_bits());
        match vd.lane_count() {
            2 => LS_NEON_2D,
            4 => LS_NEON_4S,
            8 => LS_NEON_8H,
            16 => LS_NEON_16B,
            _ => unreachable!(),
        }
    }

    fn load_store_struct_addr_mode_field(addr: MemOperand) -> Instr {
        let mut addr_field = Assembler::rn_sp(addr.base());
        if addr.is_post_index() {
            addr_field |= NEONLoadStoreMultiStructPostIndex;
            if addr.offset() == 0 {
                addr_field |= Assembler::rm_not31(addr.regoffset());
            } else {
                addr_field |= 0x1F << Rm_offset;
            }
        } else {
            debug_assert!(addr.is_immediate_offset() && (addr.offset() == 0));
        }
        addr_field
    }

    fn rm_not31(rm: impl Into<CPURegister>) -> Instr {
        let rm = rm.into();
        debug_assert_ne!(rm.code(), kSPRegInternalCode);
        debug_assert!(!rm.is_zero());
        Assembler::rm(rm)
    }

    fn load_store_struct(&mut self, vt: VRegister, addr: MemOperand, op: Instr) {
        debug_assert!(vt.is_vector() || vt.is_1d());
        let instruction = op
            | Assembler::load_store_struct_addr_mode_field(addr)
            | Assembler::ls_v_format(vt)
            | Assembler::rt(vt);
        self.emit(instruction);
    }

    fn load_store_struct_single_all_lanes(
        &mut self,
        vt: VRegister,
        addr: MemOperand,
        op: Instr,
    ) {
        let instruction = op
            | Assembler::load_store_struct_addr_mode_field(addr)
            | Assembler::ls_v_format(vt)
            | Assembler::rt(vt);
        self.emit(instruction);
    }

    fn load_store_struct_single(
        &mut self,
        vt: VRegister,
        lane: u32,
        addr: MemOperand,
        op: Instr,
    ) {
        let lane_size = vt.lane_size_in_bytes();
        debug_assert!(lane < (kQRegSize as u32) / lane_size);

        let mut lane = lane * lane_size;
        if lane_size == 8 {
            lane += 1;
        }

        let size = (lane << NEONLSSize_offset) & NEONLSSize_mask;
        let s = (lane << (NEONS_offset - 2)) & NEONS_mask;
        let q = (lane << (NEONQ_offset - 3)) & NEONQ_mask;

        let mut instruction = op;
        match lane_size {
            1 => instruction |= NEONLoadStoreSingle_b,
            2 => instruction |= NEONLoadStoreSingle_h,
            4 => instruction |= NEONLoadStoreSingle_s,
            _ => {
                debug_assert_eq!(lane_size, 8);
                instruction |= NEONLoadStoreSingle_d;
            }
        }

        let instruction = instruction
            | Assembler::load_store_struct_addr_mode_field(addr)
            | q
            | size
            | s
            | Assembler::rt(vt);
        self.emit(instruction);
    }

    fn neon_3_same(&mut self, vd: VRegister, vn: VRegister, vm: VRegister, vop: Instr) {
        debug_assert!(are_same_format(vd, &[vn, vm]));
        debug_assert!(vd.is_vector() || !vd.is_q());

        let format;
        let mut op = vop;
        if vd.is_scalar() {
            op |= NEON_Q | NEONScalar;
            format = Assembler::s_format(vd);
        } else {
            format = Assembler::v_format(vd);
        }

        let instruction = format
            | op
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    fn add_sub(
        &mut self,
        rd: Register,
        rn: Register,
        operand: Operand,
        s: FlagsUpdate,
        op: Instr,
    ) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        if operand.is_immediate() {
            let immediate = operand.immediate_value();
            debug_assert!(Assembler::is_imm_add_sub(immediate));
            let dest_reg = if s == FlagsUpdate::SetFlags {
                Assembler::rd(rd)
            } else {
                Assembler::rd_sp(rd)
            };
            let instruction = Assembler::sf(rd)
                | AddSubImmediateFixed
                | op
                | Assembler::flags(s)
                | Assembler::imm_add_sub(immediate as i32)
                | dest_reg
                | Assembler::rn_sp(rn);
            self.emit(instruction);
        } else if operand.is_shifted_register() {
            debug_assert_eq!(operand.reg().size_in_bits(), rd.size_in_bits());
            debug_assert_ne!(operand.shift(), Shift::Ror);

            if rn.is_sp() || rd.is_sp() {
                debug_assert!(!(rd.is_sp() && (s == FlagsUpdate::SetFlags)));
                self.data_proc_extended_register(
                    rd,
                    rn,
                    operand.to_extended_register(),
                    s,
                    AddSubExtendedFixed | op,
                );
            } else {
                self.data_proc_shifted_register(rd, rn, operand, s, AddSubShiftedFixed | op);
            }
        } else {
            debug_assert!(operand.is_extended_register());
            self.data_proc_extended_register(rd, rn, operand, s, AddSubExtendedFixed | op);
        }
    }

    fn logical(&mut self, rd: Register, rn: Register, operand: Operand, op: Instr) {
        debug_assert_eq!(rd.size_in_bits(), rn.size_in_bits());
        if operand.is_immediate() {
            let mut immediate = operand.immediate_value();
            let reg_size = rd.size_in_bits();
            let mut op = op;

            debug_assert_ne!(immediate, 0);
            debug_assert_ne!(immediate, -1);
            debug_assert!(rd.is_64_bits() || is_uint32(immediate));

            if (op & NOT) == NOT {
                op &= !NOT;
                immediate = if rd.is_64_bits() {
                    !immediate
                } else {
                    !immediate & kWRegMask
                };
            }

            let mut n = 0;
            let mut imm_s = 0;
            let mut imm_r = 0;
            if Assembler::is_imm_logical(
                immediate as u64,
                reg_size,
                &mut n,
                &mut imm_s,
                &mut imm_r,
            ) {
                self.logical_immediate(rd, rn, n, imm_s, imm_r, op);
            } else {
                unreachable!()
            }
        } else {
            debug_assert!(operand.is_shifted_register());
            debug_assert_eq!(operand.reg().size_in_bits(), rd.size_in_bits());
            let dp_op = op | LogicalShiftedFixed;
            self.data_proc_shifted_register(rd, rn, operand, FlagsUpdate::LeaveFlags, dp_op);
        }
    }

    fn data_proc_shifted_register(
        &mut self,
        rd: Register,
        rn: Register,
        operand: Operand,
        s: FlagsUpdate,
        op: Instr,
    ) {
        debug_assert!(operand.is_shifted_register());
        debug_assert!(rn.is_64_bits() || (rn.is_32_bits() && is_uint5(operand.shift_amount() as i64)));
        let instruction = Assembler::sf(rd)
            | op
            | Assembler::flags(s)
            | Assembler::shift_dp(operand.shift())
            | Assembler::imm_dp_shift(operand.shift_amount())
            | Assembler::rm(operand.reg())
            | Assembler::rn(rn)
            | Assembler::rd(rd);
        self.emit(instruction);
    }

    fn data_proc_extended_register(
        &mut self,
        rd: Register,
        rn: Register,
        operand: Operand,
        s: FlagsUpdate,
        op: Instr,
    ) {
        let dest_reg = if s == FlagsUpdate::SetFlags {
            Assembler::rd(rd)
        } else {
            Assembler::rd_sp(rd)
        };
        let instruction = Assembler::sf(rd)
            | op
            | Assembler::flags(s)
            | Assembler::extend_mode(operand.extend())
            | Assembler::imm_extend_shift(operand.shift_amount())
            | Assembler::rm(operand.reg())
            | Assembler::rn_sp(rn)
            | dest_reg;
        self.emit(instruction);
    }

    fn extend_mode(extend: Extend) -> Instr {
        (extend as Instr) << ExtendMode_offset
    }

    fn imm_extend_shift(left_shift: u32) -> Instr {
        debug_assert!(left_shift <= 4);
        left_shift << ImmExtendShift_offset
    }

    fn move_wide(&mut self, rd: Register, imm: u64, shift: i32, mov_op: Instr) {
        let mut imm = imm;
        let mut shift = shift;
        if rd.is_32_bits() {
            debug_assert!(
                ((imm >> kWRegSizeInBits) == 0)
                    || ((imm >> (kWRegSizeInBits - 1)) == 0x1FFFFFFFF)
            );
            imm &= kWRegMask as u64;
        }

        if shift >= 0 {
            debug_assert!((shift == 0) || (shift == 16) || (shift == 32) || (shift == 48));
            debug_assert!(rd.is_64_bits() || (shift == 0) || (shift == 16));
            shift /= 16;
        } else {
            shift = 0;
            if (imm & !0xFFFFu64) == 0 {
            } else if (imm & !(0xFFFFu64 << 16)) == 0 {
                imm >>= 16;
                shift = 1;
            } else if (imm & !(0xFFFFu64 << 32)) == 0 {
                debug_assert!(rd.is_64_bits());
                imm >>= 32;
                shift = 2;
            } else if (imm & !(0xFFFFu64 << 48)) == 0 {
                debug_assert!(rd.is_64_bits());
                imm >>= 48;
                shift = 3;
            }
        }

        debug_assert!(is_uint16(imm as i64));

        let instruction = Assembler::sf(rd)
            | MoveWideImmediateFixed
            | mov_op
            | Assembler::rd(rd)
            | Assembler::imm_move_wide(imm as i32)
            | Assembler::shift_move_wide(shift);
        self.emit(instruction);
    }

    fn load_store(&mut self, rt: impl Into<CPURegister>, addr: MemOperand, op: Instr) {
        let rt = rt.into();
        let memop = op | Assembler::rt(rt) | Assembler::rn_sp(addr.base());

        if addr.is_immediate_offset() {
            let size_log2 = Assembler::calc_ls_data_size_log2(op);
            let offset = addr.offset() as i32;
            if Assembler::is_imm_ls_scaled(addr.offset(), size_log2) {
                self.load_store_scaled_imm_offset(memop, offset, size_log2);
            } else {
                debug_assert!(Assembler::is_imm_ls_unscaled(addr.offset()));
                self.load_store_unscaled_imm_offset(memop, offset);
            }
        } else if addr.is_register_offset() {
            let mut ext = addr.extend();
            let shift = addr.shift();
            let shift_amount = addr.shift_amount();

            if shift == Shift::Lsl {
                ext = Extend::Uxtx;
            }

            debug_assert!(
                shift_amount == 0 || shift_amount == Assembler::calc_ls_data_size_log2(op)
            );
            let instruction = LoadStoreRegisterOffsetFixed
                | memop
                | Assembler::rm(addr.regoffset())
                | Assembler::extend_mode(ext)
                | Assembler::imm_shift_ls(if shift_amount > 0 { 1 } else { 0 });
            self.emit(instruction);
        } else {
            debug_assert!(Assembler::is_imm_ls_unscaled(addr.offset()));
            debug_assert_ne!(rt, addr.base().into());
            let offset = addr.offset() as i32;
            if addr.is_pre_index() {
                let instruction =
                    LoadStorePreIndexFixed | memop | Assembler::imm_ls(offset);
                self.emit(instruction);
            } else {
                debug_assert!(addr.is_post_index());
                let instruction =
                    LoadStorePostIndexFixed | memop | Assembler::imm_ls(offset);
                self.emit(instruction);
            }
        }
    }

    fn imm_shift_ls(shift_amount: u32) -> Instr {
        debug_assert!(is_uint1(shift_amount as i64));
        shift_amount << ImmShiftLS_offset
    }

    fn load_store_scaled_imm_offset(&mut self, memop: Instr, offset: i32, size: u32) {
        let instruction = LoadStoreUnsignedOffsetFixed
            | memop
            | Assembler::imm_ls_unsigned(offset >> size);
        self.emit(instruction);
    }

    fn load_store_unscaled_imm_offset(&mut self, memop: Instr, offset: i32) {
        let instruction =
            LoadStoreUnscaledOffsetFixed | memop | Assembler::imm_ls(offset);
        self.emit(instruction);
    }
}


macro_rules! define_compare_and_swap_w_x {
    ($(($name:ident, $op_x:ident, $op_w:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, rs: Register, rt: Register, src: MemOperand) {
                    debug_assert!(src.is_immediate_offset() && (src.offset() == 0));
                    let op = if rt.is_64_bits() { $op_x } else { $op_w };
                    let instruction = op
                        | Assembler::rs(rs)
                        | Assembler::rt(rt)
                        | Rt2_mask
                        | Assembler::rn_sp(src.base());
                    self.emit(instruction);
                }
            )*
        }
    };
}

define_compare_and_swap_w_x!(
    (cas, CAS_x, CAS_w),
    (casa, CASA_x, CASA_w),
    (casl, CASL_x, CASL_w),
    (casal, CASAL_x, CASAL_w),
);

macro_rules! define_compare_and_swap_w {
    ($(($name:ident, $op:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, rs: Register, rt: Register, src: MemOperand) {
                    debug_assert!(src.is_immediate_offset() && (src.offset() == 0));
                    let instruction = $op
                        | Assembler::rs(rs)
                        | Assembler::rt(rt)
                        | Rt2_mask
                        | Assembler::rn_sp(src.base());
                    self.emit(instruction);
                }
            )*
        }
    };
}

define_compare_and_swap_w!(
    (casb, CASB),
    (casab, CASAB),
    (caslb, CASLB),
    (casalb, CASALB),
    (cash, CASH),
    (casah, CASAH),
    (caslh, CASLH),
    (casalh, CASALH),
);

macro_rules! define_compare_and_swap_pair {
    ($(($name:ident, $op_x:ident, $op_w:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(
                    &mut self,
                    rs: Register,
                    rs1: Register,
                    rt: Register,
                    rt1: Register,
                    src: MemOperand,
                ) {
                    debug_assert!(src.is_immediate_offset() && (src.offset() == 0));
                    debug_assert!(Assembler::are_even(rs, rt));
                    debug_assert!(Assembler::are_consecutive(rs, rs1));
                    debug_assert!(Assembler::are_consecutive(rt, rt1));
                    let op = if rt.is_64_bits() { $op_x } else { $op_w };
                    let instruction = op
                        | Assembler::rs(rs)
                        | Assembler::rt(rt)
                        | Rt2_mask
                        | Assembler::rn_sp(src.base());
                    self.emit(instruction);
                }
            )*
        }
    };
}

define_compare_and_swap_pair!(
    (casp, CASP_x, CASP_w),
    (caspa, CASPA_x, CASPA_w),
    (caspl, CASPL_x, CASPL_w),
    (caspal, CASPAL_x, CASPAL_w),
);

macro_rules! define_atomic_memory_load {
    ($(($name:ident, $op_x:ident, $op_w:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, rs: Register, rt: Register, src: MemOperand) {
                    debug_assert!(src.is_immediate_offset() && (src.offset() == 0));
                    let op = if rt.is_64_bits() { $op_x } else { $op_w };
                    let instruction = op
                        | Assembler::rs(rs)
                        | Assembler::rt(rt)
                        | Assembler::rn_sp(src.base());
                    self.emit(instruction);
                }
            )*
        }
    };
}

define_atomic_memory_load!(
    (ldadd, LDADD_x, LDADD_w),
    (ldaddl, LDADDL_x, LDADDL_w),
    (ldaddb, LDADDB, LDADDB),
    (ldaddlb, LDADDLB, LDADDLB),
    (ldaddh, LDADDH, LDADDH),
    (ldaddlh, LDADDLH, LDADDLH),
    (ldadda, LDADDA_x, LDADDA_w),
    (ldaddal, LDADDAL_x, LDADDAL_w),
    (ldaddab, LDADDAB, LDADDAB),
    (ldaddalb, LDADDALB, LDADDALB),
    (ldaddah, LDADDAH, LDADDAH),
    (ldaddalh, LDADDALH, LDADDALH),
    (ldclr, LDCLR_x, LDCLR_w),
    (ldclrl, LDCLRL_x, LDCLRL_w),
    (ldclrb, LDCLRB, LDCLRB),
    (ldclrlb, LDCLRLB, LDCLRLB),
    (ldclrh, LDCLRH, LDCLRH),
    (ldclrlh, LDCLRLH, LDCLRLH),
    (ldclra, LDCLRA_x, LDCLRA_w),
    (ldclral, LDCLRAL_x, LDCLRAL_w),
    (ldclrab, LDCLRAB, LDCLRAB),
    (ldclralb, LDCLRALB, LDCLRALB),
    (ldclrah, LDCLRAH, LDCLRAH),
    (ldclralh, LDCLRALH, LDCLRALH),
    (ldeor, LDEOR_x, LDEOR_w),
    (ldeorl, LDEORL_x, LDEORL_w),
    (ldeorb, LDEORB, LDEORB),
    (ldeorlb, LDEORLB, LDEORLB),
    (ldeorh, LDEORH, LDEORH),
    (ldeorlh, LDEORLH, LDEORLH),
    (ldeora, LDEORA_x, LDEORA_w),
    (ldeoral, LDEORAL_x, LDEORAL_w),
    (ldeorab, LDEORAB, LDEORAB),
    (ldeoralb, LDEORALB, LDEORALB),
    (ldeorah, LDEORAH, LDEORAH),
    (ldeoralh, LDEORALH, LDEORALH),
    (ldset, LDSET_x, LDSET_w),
    (ldsetl, LDSETL_x, LDSETL_w),
    (ldsetb, LDSETB, LDSETB),
    (ldsetlb, LDSETLB, LDSETLB),
    (ldseth, LDSETH, LDSETH),
    (ldsetlh, LDSETLH, LDSETLH),
    (ldseta, LDSETA_x, LDSETA_w),
    (ldsetal, LDSETAL_x, LDSETAL_w),
    (ldsetab, LDSETAB, LDSETAB),
    (ldsetalb, LDSETALB, LDSETALB),
    (ldsetah, LDSETAH, LDSETAH),
    (ldsetalh, LDSETALH, LDSETALH),
    (ldsmax, LDSMAX_x, LDSMAX_w),
    (ldsmaxl, LDSMAXL_x, LDSMAXL_w),
    (ldsmaxb, LDSMAXB, LDSMAXB),
    (ldsmaxlb, LDSMAXLB, LDSMAXLB),
    (ldsmaxh, LDSMAXH, LDSMAXH),
    (ldsmaxlh, LDSMAXLH, LDSMAXLH),
    (ldsmaxa, LDSMAXA_x, LDSMAXA_w),
    (ldsmaxal, LDSMAXAL_x, LDSMAXAL_w),
    (ldsmaxab, LDSMAXAB, LDSMAXAB),
    (ldsmaxalb, LDSMAXALB, LDSMAXALB),
    (ldsmaxah, LDSMAXAH, LDSMAXAH),
    (ldsmaxalh, LDSMAXALH, LDSMAXALH),
    (ldsmin, LDSMIN_x, LDSMIN_w),
    (ldsminl, LDSMINL_x, LDSMINL_w),
    (ldsminb, LDSMINB, LDSMINB),
    (ldsminlb, LDSMINLB, LDSMINLB),
    (ldsminh, LDSMINH, LDSMINH),
    (ldsminlh, LDSMINLH, LDSMINLH),
    (ldsmina, LDSMINA_x, LDSMINA_w),
    (ldsminal, LDSMINAL_x, LDSMINAL_w),
    (ldsminab, LDSMINAB, LDSMINAB),
    (ldsminalb, LDSMINALB, LDSMINALB),
    (ldsminah, LDSMINAH, LDSMINAH),
    (ldsminalh, LDSMINALH, LDSMINALH),
    (ldumax, LDUMAX_x, LDUMAX_w),
    (ldumaxl, LDUMAXL_x, LDUMAXL_w),
    (ldumaxb, LDUMAXB, LDUMAXB),
    (ldumaxlb, LDUMAXLB, LDUMAXLB),
    (ldumaxh, LDUMAXH, LDUMAXH),
    (ldumaxlh, LDUMAXLH, LDUMAXLH),
    (ldumaxa, LDUMAXA_x, LDUMAXA_w),
    (ldumaxal, LDUMAXAL_x, LDUMAXAL_w),
    (ldumaxab, LDUMAXAB, LDUMAXAB),
    (ldumaxalb, LDUMAXALB, LDUMAXALB),
    (ldumaxah, LDUMAXAH, LDUMAXAH),
    (ldumaxalh, LDUMAXALH, LDUMAXALH),
    (ldumin, LDUMIN_x, LDUMIN_w),
    (lduminl, LDUMINL_x, LDUMINL_w),
    (lduminb, LDUMINB, LDUMINB),
    (lduminlb, LDUMINLB, LDUMINLB),
    (lduminh, LDUMINH, LDUMINH),
    (lduminlh, LDUMINLH, LDUMINLH),
    (ldumina, LDUMINA_x, LDUMINA_w),
    (lduminal, LDUMINAL_x, LDUMINAL_w),
    (lduminab, LDUMINAB, LDUMINAB),
    (lduminalb, LDUMINALB, LDUMINALB),
    (lduminah, LDUMINAH, LDUMINAH),
    (lduminalh, LDUMINALH, LDUMINALH),
);

macro_rules! define_atomic_memory_store {
    ($(($name:ident, $load:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, rs: Register, src: MemOperand) {
                    let zr = Assembler::appropriate_zero_reg_for(rs);
                    self.$load(rs, zr, src);
                }
            )*
        }
    };
}

define_atomic_memory_store!(
    (stadd, ldadd),
    (staddl, ldaddl),
    (staddb, ldaddb),
    (staddlb, ldaddlb),
    (staddh, ldaddh),
    (staddlh, ldaddlh),
    (stclr, ldclr),
    (stclrl, ldclrl),
    (stclrb, ldclrb),
    (stclrlb, ldclrlb),
    (stclrh, ldclrh),
    (stclrlh, ldclrlh),
    (steor, ldeor),
    (steorl, ldeorl),
    (steorb, ldeorb),
    (steorlb, ldeorlb),
    (steorh, ldeorh),
    (steorlh, ldeorlh),
    (stset, ldset),
    (stsetl, ldsetl),
    (stsetb, ldsetb),
    (stsetlb, ldsetlb),
    (stseth, ldseth),
    (stsetlh, ldsetlh),
    (stsmax, ldsmax),
    (stsmaxl, ldsmaxl),
    (stsmaxb, ldsmaxb),
    (stsmaxlb, ldsmaxlb),
    (stsmaxh, ldsmaxh),
    (stsmaxlh, ldsmaxlh),
    (stsmin, ldsmin),
    (stsminl, ldsminl),
    (stsminb, ldsminb),
    (stsminlb, ldsminlb),
    (stsminh, ldsminh),
    (stsminlh, ldsminlh),
    (stumax, ldumax),
    (stumaxl, ldumaxl),
    (stumaxb, ldumaxb),
    (stumaxlb, ldumaxlb),
    (stumaxh, ldumaxh),
    (stumaxlh, ldumaxlh),
    (stumin, ldumin),
    (stuminl, lduminl),
    (stuminb, lduminb),
    (stuminlb, lduminlb),
    (stuminh, lduminh),
    (stuminlh, lduminlh),
);

define_atomic_memory_load!(
    (swp, SWP_x, SWP_w),
    (swpl, SWPL_x, SWPL_w),
    (swpb, SWPB, SWPB),
    (swplb, SWPLB, SWPLB),
    (swph, SWPH, SWPH),
    (swplh, SWPLH, SWPLH),
    (swpa, SWPA_x, SWPA_w),
    (swpal, SWPAL_x, SWPAL_w),
    (swpab, SWPAB, SWPAB),
    (swpalb, SWPALB, SWPALB),
    (swpah, SWPAH, SWPAH),
    (swpalh, SWPALH, SWPALH),
);

macro_rules! define_neon_3_same {
    ($(($name:ident, $op:ident, $vd:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, $vd: VRegister, vn: VRegister, vm: VRegister) {
                    debug_assert!($as);
                    self.neon_3_same($vd, vn, vm, $op);
                }
            )*
        }
    };
}

define_neon_3_same!(
    (addp, NEON_ADDP, vd, vd.is_vector() || vd.is_1d()),
    (cmeq, NEON_CMEQ, vd, vd.is_vector() || vd.is_1d()),
    (cmge, NEON_CMGE, vd, vd.is_vector() || vd.is_1d()),
    (cmgt, NEON_CMGT, vd, vd.is_vector() || vd.is_1d()),
    (cmhi, NEON_CMHI, vd, vd.is_vector() || vd.is_1d()),
    (cmhs, NEON_CMHS, vd, vd.is_vector() || vd.is_1d()),
    (cmtst, NEON_CMTST, vd, vd.is_vector() || vd.is_1d()),
    (sshl, NEON_SSHL, vd, vd.is_vector() || vd.is_1d()),
    (ushl, NEON_USHL, vd, vd.is_vector() || vd.is_1d()),
    (srshl, NEON_SRSHL, vd, vd.is_vector() || vd.is_1d()),
    (urshl, NEON_URSHL, vd, vd.is_vector() || vd.is_1d()),
    (shadd, NEON_SHADD, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (uhadd, NEON_UHADD, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (srhadd, NEON_SRHADD, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (urhadd, NEON_URHADD, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (shsub, NEON_SHSUB, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (uhsub, NEON_UHSUB, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (smax, NEON_SMAX, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (smaxp, NEON_SMAXP, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (smin, NEON_SMIN, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (sminp, NEON_SMINP, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (umax, NEON_UMAX, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (umaxp, NEON_UMAXP, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (umin, NEON_UMIN, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (uminp, NEON_UMINP, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (saba, NEON_SABA, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (sabd, NEON_SABD, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (uaba, NEON_UABA, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (uabd, NEON_UABD, vd, vd.is_vector() && !vd.is_lane_size_d()),
    (bit, NEON_BIT, vd, vd.is_8b() || vd.is_16b()),
    (bif, NEON_BIF, vd, vd.is_8b() || vd.is_16b()),
    (bsl, NEON_BSL, vd, vd.is_8b() || vd.is_16b()),
    (pmul, NEON_PMUL, vd, vd.is_8b() || vd.is_16b()),
    (uqadd, NEON_UQADD, vd, true),
    (sqadd, NEON_SQADD, vd, true),
    (uqsub, NEON_UQSUB, vd, true),
    (sqsub, NEON_SQSUB, vd, true),
    (sqrshl, NEON_SQRSHL, vd, true),
    (uqrshl, NEON_UQRSHL, vd, true),
);

pub trait Arithmetic: Copy {
    type Rhs;

    fn add(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn sub(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn mul(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn and_(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn orr(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn orn(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn eor(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn bic(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn smull(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
    fn umull(asm: &mut Assembler, rd: Self, rn: Self, operand: Self::Rhs);
}

impl Arithmetic for Register {
    type Rhs = Operand;

    fn add(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        asm.add_sub(rd, rn, operand, FlagsUpdate::LeaveFlags, ADD);
    }

    fn sub(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        asm.add_sub(rd, rn, operand, FlagsUpdate::LeaveFlags, SUB);
    }

    fn mul(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        let rm = operand.reg();
        let zr = Assembler::appropriate_zero_reg_for(rn);
        asm.data_processing_3_source(rd, rn, rm, zr, MADD);
    }

    fn and_(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        asm.logical(rd, rn, operand, AND);
    }

    fn orr(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        asm.logical(rd, rn, operand, ORR);
    }

    fn orn(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        asm.logical(rd, rn, operand, ORN);
    }

    fn eor(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        asm.logical(rd, rn, operand, EOR);
    }

    fn bic(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        asm.logical(rd, rn, operand, BIC);
    }

    fn smull(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        let rm = operand.reg();
        debug_assert!(rd.is_64_bits());
        debug_assert!(rn.is_32_bits() && rm.is_32_bits());
        asm.data_processing_3_source(rd, rn, rm, xzr, SMADDL_x);
    }

    fn umull(asm: &mut Assembler, rd: Register, rn: Register, operand: Operand) {
        let rm = operand.reg();
        debug_assert!(rd.is_64_bits());
        debug_assert!(rn.is_32_bits() && rm.is_32_bits());
        asm.data_processing_3_source(rd, rn, rm, xzr, UMADDL_x);
    }
}

impl Arithmetic for VRegister {
    type Rhs = VOperand;

    fn add(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        debug_assert!(!operand.has_lane());
        debug_assert!(vd.is_vector() || vd.is_1d());
        asm.neon_3_same(vd, vn, operand.reg(), NEON_ADD);
    }

    fn sub(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        debug_assert!(!operand.has_lane());
        debug_assert!(vd.is_vector() || vd.is_1d());
        asm.neon_3_same(vd, vn, operand.reg(), NEON_SUB);
    }

    fn mul(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        let vm = operand.reg();
        if operand.has_lane() {
            debug_assert!(vn.is_vector());
            asm.neon_by_element(vd, vn, vm, operand.index(), NEON_MUL_byelement);
            return;
        }
        debug_assert!(vd.is_vector() && !vd.is_lane_size_d());
        asm.neon_3_same(vd, vn, vm, NEON_MUL);
    }

    fn and_(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        debug_assert!(!operand.has_lane());
        debug_assert!(vd.is_8b() || vd.is_16b());
        asm.neon_3_same(vd, vn, operand.reg(), NEON_AND);
    }

    fn orr(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        debug_assert!(!operand.has_lane());
        debug_assert!(vd.is_8b() || vd.is_16b());
        asm.neon_3_same(vd, vn, operand.reg(), NEON_ORR);
    }

    fn orn(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        debug_assert!(!operand.has_lane());
        debug_assert!(vd.is_8b() || vd.is_16b());
        asm.neon_3_same(vd, vn, operand.reg(), NEON_ORN);
    }

    fn eor(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        debug_assert!(!operand.has_lane());
        debug_assert!(vd.is_8b() || vd.is_16b());
        asm.neon_3_same(vd, vn, operand.reg(), NEON_EOR);
    }

    fn bic(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        debug_assert!(!operand.has_lane());
        debug_assert!(vd.is_8b() || vd.is_16b());
        asm.neon_3_same(vd, vn, operand.reg(), NEON_BIC);
    }

    fn smull(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        let vm = operand.reg();
        debug_assert!(vn.is_vector() && vn.is_d());
        if operand.has_lane() {
            asm.neon_by_element_l(vd, vn, vm, operand.index(), NEON_SMULL_byelement);
            return;
        }
        asm.neon_3_different_l(vd, vn, vm, NEON_SMULL);
    }

    fn umull(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: VOperand) {
        let vm = operand.reg();
        debug_assert!(vn.is_vector() && vn.is_d());
        if operand.has_lane() {
            asm.neon_by_element_l(vd, vn, vm, operand.index(), NEON_UMULL_byelement);
            return;
        }
        asm.neon_3_different_l(vd, vn, vm, NEON_UMULL);
    }
}

impl Assembler {
    pub fn add<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::add(self, rd, rn, operand.into());
    }

    pub fn sub<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::sub(self, rd, rn, operand.into());
    }

    pub fn mul<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::mul(self, rd, rn, operand.into());
    }

    pub fn and_<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::and_(self, rd, rn, operand.into());
    }

    pub fn orr<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::orr(self, rd, rn, operand.into());
    }

    pub fn orn<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::orn(self, rd, rn, operand.into());
    }

    pub fn eor<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::eor(self, rd, rn, operand.into());
    }

    pub fn bic<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::bic(self, rd, rn, operand.into());
    }

    pub fn smull<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::smull(self, rd, rn, operand.into());
    }

    pub fn umull<T: Arithmetic, R: Into<T::Rhs>>(&mut self, rd: T, rn: T, operand: R) {
        T::umull(self, rd, rn, operand.into());
    }
}

macro_rules! define_neon_3_different_l {
    ($(($name:ident, $op:ident, $vd:ident, $vn:ident, $vm:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(
                    &mut self,
                    $vd: VRegister,
                    $vn: VRegister,
                    $vm: VRegister,
                ) {
                    debug_assert!($as);
                    self.neon_3_different_l($vd, $vn, $vm, $op);
                }
            )*
        }
    };
}

define_neon_3_different_l!(
    (saddl, NEON_SADDL, vd, vn, vm, vn.is_vector() && vn.is_d()),
    (saddl2, NEON_SADDL2, vd, vn, vm, vn.is_vector() && vn.is_q()),
    (sabal, NEON_SABAL, vd, vn, vm, vn.is_vector() && vn.is_d()),
    (sabal2, NEON_SABAL2, vd, vn, vm, vn.is_vector() && vn.is_q()),
    (uabal, NEON_UABAL, vd, vn, vm, vn.is_vector() && vn.is_d()),
    (uabal2, NEON_UABAL2, vd, vn, vm, vn.is_vector() && vn.is_q()),
    (sabdl, NEON_SABDL, vd, vn, vm, vn.is_vector() && vn.is_d()),
    (sabdl2, NEON_SABDL2, vd, vn, vm, vn.is_vector() && vn.is_q()),
    (uabdl, NEON_UABDL, vd, vn, vm, vn.is_vector() && vn.is_d()),
    (uabdl2, NEON_UABDL2, vd, vn, vm, vn.is_vector() && vn.is_q()),
    (ssubl, NEON_SSUBL, vd, vn, vm, vn.is_vector() && vn.is_d()),
    (ssubl2, NEON_SSUBL2, vd, vn, vm, vn.is_vector() && vn.is_q()),
    (uaddl, NEON_UADDL, vd, vn, vm, vn.is_vector() && vn.is_d()),
    (uaddl2, NEON_UADDL2, vd, vn, vm, vn.is_vector() && vn.is_q()),
    (usubl, NEON_USUBL, vd, vn, vm, vn.is_vector() && vn.is_d()),
    (usubl2, NEON_USUBL2, vd, vn, vm, vn.is_vector() && vn.is_q()),
);

macro_rules! define_neon_3_different_hn {
    ($(($name:ident, $op:ident, $vd:ident, $vn:ident, $vm:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(
                    &mut self,
                    $vd: VRegister,
                    $vn: VRegister,
                    $vm: VRegister,
                ) {
                    debug_assert!($as);
                    self.neon_3_different_hn($vd, $vn, $vm, $op);
                }
            )*
        }
    };
}

define_neon_3_different_hn!(
    (addhn, NEON_ADDHN, vd, vn, vm, vd.is_d()),
    (addhn2, NEON_ADDHN2, vd, vn, vm, vd.is_q()),
    (raddhn, NEON_RADDHN, vd, vn, vm, vd.is_d()),
    (raddhn2, NEON_RADDHN2, vd, vn, vm, vd.is_q()),
    (subhn, NEON_SUBHN, vd, vn, vm, vd.is_d()),
    (subhn2, NEON_SUBHN2, vd, vn, vm, vd.is_q()),
    (rsubhn, NEON_RSUBHN, vd, vn, vm, vd.is_d()),
    (rsubhn2, NEON_RSUBHN2, vd, vn, vm, vd.is_q()),
);

macro_rules! define_neon_3_different_w {
    ($(($name:ident, $op:ident, $vd:ident, $vn:ident, $vm:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(
                    &mut self,
                    $vd: VRegister,
                    $vn: VRegister,
                    $vm: VRegister,
                ) {
                    debug_assert!($as);
                    self.neon_3_different_w($vd, $vn, $vm, $op);
                }
            )*
        }
    };
}

define_neon_3_different_w!(
    (uaddw, NEON_UADDW, vd, vn, vm, vm.is_d()),
    (uaddw2, NEON_UADDW2, vd, vn, vm, vm.is_q()),
    (saddw, NEON_SADDW, vd, vn, vm, vm.is_d()),
    (saddw2, NEON_SADDW2, vd, vn, vm, vm.is_q()),
    (usubw, NEON_USUBW, vd, vn, vm, vm.is_d()),
    (usubw2, NEON_USUBW2, vd, vn, vm, vm.is_q()),
    (ssubw, NEON_SSUBW, vd, vn, vm, vm.is_d()),
    (ssubw2, NEON_SSUBW2, vd, vn, vm, vm.is_q()),
);

macro_rules! define_neon_perm {
    ($(($name:ident, $op:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, vd: VRegister, vn: VRegister, vm: VRegister) {
                    self.neon_perm(vd, vn, vm, $op);
                }
            )*
        }
    };
}

define_neon_perm!(
    (trn1, NEON_TRN1),
    (trn2, NEON_TRN2),
    (uzp1, NEON_UZP1),
    (uzp2, NEON_UZP2),
    (zip1, NEON_ZIP1),
    (zip2, NEON_ZIP2),
);

macro_rules! define_neon_across_lanes {
    ($(($name:ident, $op:ident, $vd:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, $vd: VRegister, vn: VRegister) {
                    debug_assert!($as);
                    self.neon_across_lanes($vd, vn, $op);
                }
            )*
        }
    };
}

define_neon_across_lanes!(
    (fmaxv, NEON_FMAXV, vd, vd.is_1s()),
    (fminv, NEON_FMINV, vd, vd.is_1s()),
    (fmaxnmv, NEON_FMAXNMV, vd, vd.is_1s()),
    (fminnmv, NEON_FMINNMV, vd, vd.is_1s()),
    (addv, NEON_ADDV, vd, true),
    (smaxv, NEON_SMAXV, vd, true),
    (sminv, NEON_SMINV, vd, true),
    (umaxv, NEON_UMAXV, vd, true),
    (uminv, NEON_UMINV, vd, true),
);

macro_rules! define_neon_2_reg_misc {
    ($(($name:ident, $op:ident, $vd:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, $vd: VRegister, vn: VRegister) {
                    debug_assert!($as);
                    self.neon_2_reg_misc($vd, vn, $op);
                }
            )*
        }
    };
}

define_neon_2_reg_misc!(
    (suqadd, NEON_SUQADD, vd, true),
    (usqadd, NEON_USQADD, vd, true),
    (abs, NEON_ABS, vd, vd.is_vector() || vd.is_1d()),
    (sqabs, NEON_SQABS, vd, true),
    (sqneg, NEON_SQNEG, vd, true),
    (rev64, NEON_REV64, vd, !vd.is_1d()),
    (saddlp, NEON_SADDLP, vd, true),
    (uaddlp, NEON_UADDLP, vd, true),
    (sadalp, NEON_SADALP, vd, true),
    (uadalp, NEON_UADALP, vd, true),
    (cnt, NEON_CNT, vd, vd.is_8b() || vd.is_16b()),
);

macro_rules! define_neon_dual_3_same {
    ($(($name:ident, $op:ident, $op_byelement:ident, $vd:ident, $vn:ident, $vm:ident,
        $as:expr, $as_byelement:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name<T: Into<VOperand>>(
                    &mut self,
                    $vd: VRegister,
                    $vn: VRegister,
                    operand: T,
                ) {
                    let operand = operand.into();
                    let $vm = operand.reg();
                    if operand.has_lane() {
                        debug_assert!($as_byelement);
                        self.neon_by_element($vd, $vn, $vm, operand.index(), $op_byelement);
                    } else {
                        debug_assert!($as);
                        self.neon_3_same($vd, $vn, $vm, $op);
                    }
                }
            )*
        }
    };
}

define_neon_dual_3_same!(
    (mla, NEON_MLA, NEON_MLA_byelement, vd, vn, vm,
     vd.is_vector() && !vd.is_lane_size_d(), vn.is_vector()),
    (mls, NEON_MLS, NEON_MLS_byelement, vd, vn, vm,
     vd.is_vector() && !vd.is_lane_size_d(), vn.is_vector()),
    (sqdmulh, NEON_SQDMULH, NEON_SQDMULH_byelement, vd, vn, vm,
     vd.is_lane_size_h() || vd.is_lane_size_s(), true),
    (sqrdmulh, NEON_SQRDMULH, NEON_SQRDMULH_byelement, vd, vn, vm,
     vd.is_lane_size_h() || vd.is_lane_size_s(), true),
);

macro_rules! define_neon_dual_3_different_l {
    ($(($name:ident, $op:ident, $op_byelement:ident, $vd:ident, $vn:ident, $vm:ident,
        $as:expr, $as_byelement:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name<T: Into<VOperand>>(
                    &mut self,
                    $vd: VRegister,
                    $vn: VRegister,
                    operand: T,
                ) {
                    let operand = operand.into();
                    let $vm = operand.reg();
                    if operand.has_lane() {
                        debug_assert!($as_byelement);
                        self.neon_by_element_l($vd, $vn, $vm, operand.index(), $op_byelement);
                    } else {
                        debug_assert!($as);
                        self.neon_3_different_l($vd, $vn, $vm, $op);
                    }
                }
            )*
        }
    };
}

define_neon_dual_3_different_l!(
    (smlal, NEON_SMLAL, NEON_SMLAL_byelement, vd, vn, vm,
     vn.is_vector() && vn.is_d(), vn.is_vector() && vn.is_d()),
    (smlal2, NEON_SMLAL2, NEON_SMLAL_byelement, vd, vn, vm,
     vn.is_vector() && vn.is_q(), vn.is_vector() && vn.is_q()),
    (umlal, NEON_UMLAL, NEON_UMLAL_byelement, vd, vn, vm,
     vn.is_vector() && vn.is_d(), vn.is_vector() && vn.is_d()),
    (umlal2, NEON_UMLAL2, NEON_UMLAL_byelement, vd, vn, vm,
     vn.is_vector() && vn.is_q(), vn.is_vector() && vn.is_q()),
    (smlsl, NEON_SMLSL, NEON_SMLSL_byelement, vd, vn, vm,
     vn.is_vector() && vn.is_d(), vn.is_vector() && vn.is_d()),
    (smlsl2, NEON_SMLSL2, NEON_SMLSL_byelement, vd, vn, vm,
     vn.is_vector() && vn.is_q(), vn.is_vector() && vn.is_q()),
    (umlsl, NEON_UMLSL, NEON_UMLSL_byelement, vd, vn, vm,
     vn.is_vector() && vn.is_d(), vn.is_vector() && vn.is_d()),
    (umlsl2, NEON_UMLSL2, NEON_UMLSL_byelement, vd, vn, vm,
     vn.is_vector() && vn.is_q(), vn.is_vector() && vn.is_q()),
    (smull2, NEON_SMULL2, NEON_SMULL_byelement, vd, vn, vm,
     vn.is_vector() && vn.is_q(), vn.is_vector() && vn.is_q()),
    (umull2, NEON_UMULL2, NEON_UMULL_byelement, vd, vn, vm,
     vn.is_vector() && vn.is_q(), vn.is_vector() && vn.is_q()),
    (sqdmull, NEON_SQDMULL, NEON_SQDMULL_byelement, vd, vn, vm,
     vn.is_1h() || vn.is_1s() || vn.is_4h() || vn.is_2s(), vn.is_scalar() || vn.is_d()),
    (sqdmull2, NEON_SQDMULL2, NEON_SQDMULL_byelement, vd, vn, vm,
     vn.is_1h() || vn.is_1s() || vn.is_8h() || vn.is_4s(), vn.is_vector() && vn.is_q()),
    (sqdmlal, NEON_SQDMLAL, NEON_SQDMLAL_byelement, vd, vn, vm,
     vn.is_1h() || vn.is_1s() || vn.is_4h() || vn.is_2s(), vn.is_scalar() || vn.is_d()),
    (sqdmlal2, NEON_SQDMLAL2, NEON_SQDMLAL_byelement, vd, vn, vm,
     vn.is_1h() || vn.is_1s() || vn.is_8h() || vn.is_4s(), vn.is_vector() && vn.is_q()),
    (sqdmlsl, NEON_SQDMLSL, NEON_SQDMLSL_byelement, vd, vn, vm,
     vn.is_1h() || vn.is_1s() || vn.is_4h() || vn.is_2s(), vn.is_scalar() || vn.is_d()),
    (sqdmlsl2, NEON_SQDMLSL2, NEON_SQDMLSL_byelement, vd, vn, vm,
     vn.is_1h() || vn.is_1s() || vn.is_8h() || vn.is_4s(), vn.is_vector() && vn.is_q()),
);

macro_rules! define_neon_dual_fp {
    ($(($name:ident, $op:ident, $vec_op:ident, $op_byelement:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name<T: Into<VOperand>>(
                    &mut self,
                    vd: VRegister,
                    vn: VRegister,
                    operand: T,
                ) {
                    let operand = operand.into();
                    let vm = operand.reg();
                    if operand.has_lane() {
                        self.neon_fp_by_element(vd, vn, vm, operand.index(), $op_byelement);
                        return;
                    }
                    let op;
                    if vd.is_scalar() {
                        debug_assert!(vd.is_1s() || vd.is_1d());
                        op = $op;
                    } else {
                        debug_assert!(vd.is_vector());
                        op = $vec_op;
                    }
                    self.neon_fp_3_same(vd, vn, vm, op);
                }
            )*
        }
    };
}

define_neon_dual_fp!(
    (fmul, FMUL, NEON_FMUL, NEON_FMUL_byelement),
);

macro_rules! define_neon_fp_by_element {
    ($(($name:ident, $op:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(
                    &mut self,
                    vd: VRegister,
                    vn: VRegister,
                    vm: VRegister,
                    vm_index: i32,
                ) {
                    self.neon_fp_by_element(vd, vn, vm, vm_index, $op);
                }
            )*
        }
    };
}

define_neon_fp_by_element!(
    (fmla, NEON_FMLA_byelement),
    (fmls, NEON_FMLS_byelement),
);

macro_rules! define_neon_shift_left {
    ($(($name:ident, $op:ident, $vd:ident, $vn:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, $vd: VRegister, $vn: VRegister, shift: i32) {
                    debug_assert!($as);
                    self.neon_shift_left_immediate($vd, $vn, shift, $op);
                }
            )*
        }
    };
}

define_neon_shift_left!(
    (shl, NEON_SHL, vd, vn, vd.is_vector() || vd.is_1d()),
    (sli, NEON_SLI, vd, vn, vd.is_vector() || vd.is_1d()),
    (sqshlu, NEON_SQSHLU, vd, vn, true),
);

macro_rules! define_neon_shift_right {
    ($(($name:ident, $op:ident, $vd:ident, $vn:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, $vd: VRegister, $vn: VRegister, shift: i32) {
                    debug_assert!($as);
                    self.neon_shift_right_immediate($vd, $vn, shift, $op);
                }
            )*
        }
    };
}

define_neon_shift_right!(
    (sri, NEON_SRI, vd, vn, vd.is_vector() || vd.is_1d()),
    (sshr, NEON_SSHR, vd, vn, vd.is_vector() || vd.is_1d()),
    (ushr, NEON_USHR, vd, vn, vd.is_vector() || vd.is_1d()),
    (srshr, NEON_SRSHR, vd, vn, vd.is_vector() || vd.is_1d()),
    (urshr, NEON_URSHR, vd, vn, vd.is_vector() || vd.is_1d()),
    (ssra, NEON_SSRA, vd, vn, vd.is_vector() || vd.is_1d()),
    (usra, NEON_USRA, vd, vn, vd.is_vector() || vd.is_1d()),
    (srsra, NEON_SRSRA, vd, vn, vd.is_vector() || vd.is_1d()),
    (ursra, NEON_URSRA, vd, vn, vd.is_vector() || vd.is_1d()),
);

macro_rules! define_neon_shift_l {
    ($(($name:ident, $op:ident, $vd:ident, $vn:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, $vd: VRegister, $vn: VRegister, shift: i32) {
                    debug_assert!($as);
                    self.neon_shift_immediate_l($vd, $vn, shift, $op);
                }
            )*
        }
    };
}

define_neon_shift_l!(
    (sshll, NEON_SSHLL, vd, vn, vn.is_d()),
    (sshll2, NEON_SSHLL, vd, vn, vn.is_q()),
    (ushll, NEON_USHLL, vd, vn, vn.is_d()),
    (ushll2, NEON_USHLL, vd, vn, vn.is_q()),
);

macro_rules! define_neon_shift_n {
    ($(($name:ident, $op:ident, $vd:ident, $vn:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, $vd: VRegister, $vn: VRegister, shift: i32) {
                    debug_assert!($as);
                    self.neon_shift_immediate_n($vd, $vn, shift, $op);
                }
            )*
        }
    };
}

define_neon_shift_n!(
    (shrn, NEON_SHRN, vd, vn, vn.is_vector() && vd.is_d()),
    (shrn2, NEON_SHRN, vd, vn, vn.is_vector() && vd.is_q()),
    (rshrn, NEON_RSHRN, vd, vn, vn.is_vector() && vd.is_d()),
    (rshrn2, NEON_RSHRN, vd, vn, vn.is_vector() && vd.is_q()),
    (sqshrn, NEON_SQSHRN, vd, vn, vd.is_d() || (vn.is_scalar() && vd.is_scalar())),
    (sqshrn2, NEON_SQSHRN, vd, vn, vn.is_vector() && vd.is_q()),
    (sqrshrn, NEON_SQRSHRN, vd, vn, vd.is_d() || (vn.is_scalar() && vd.is_scalar())),
    (sqrshrn2, NEON_SQRSHRN, vd, vn, vn.is_vector() && vd.is_q()),
    (sqshrun, NEON_SQSHRUN, vd, vn, vd.is_d() || (vn.is_scalar() && vd.is_scalar())),
    (sqshrun2, NEON_SQSHRUN, vd, vn, vn.is_vector() && vd.is_q()),
    (sqrshrun, NEON_SQRSHRUN, vd, vn, vd.is_d() || (vn.is_scalar() && vd.is_scalar())),
    (sqrshrun2, NEON_SQRSHRUN, vd, vn, vn.is_vector() && vd.is_q()),
    (uqshrn, NEON_UQSHRN, vd, vn, vd.is_d() || (vn.is_scalar() && vd.is_scalar())),
    (uqshrn2, NEON_UQSHRN, vd, vn, vn.is_vector() && vd.is_q()),
    (uqrshrn, NEON_UQRSHRN, vd, vn, vd.is_d() || (vn.is_scalar() && vd.is_scalar())),
    (uqrshrn2, NEON_UQRSHRN, vd, vn, vn.is_vector() && vd.is_q()),
);

pub trait NeonShift: Copy {
    fn sqshl(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: Self);
    fn uqshl(asm: &mut Assembler, vd: VRegister, vn: VRegister, operand: Self);
}

impl NeonShift for VRegister {
    fn sqshl(asm: &mut Assembler, vd: VRegister, vn: VRegister, vm: VRegister) {
        asm.neon_3_same(vd, vn, vm, NEON_SQSHL);
    }

    fn uqshl(asm: &mut Assembler, vd: VRegister, vn: VRegister, vm: VRegister) {
        asm.neon_3_same(vd, vn, vm, NEON_UQSHL);
    }
}

impl NeonShift for i32 {
    fn sqshl(asm: &mut Assembler, vd: VRegister, vn: VRegister, shift: i32) {
        asm.neon_shift_left_immediate(vd, vn, shift, NEON_SQSHL_imm);
    }

    fn uqshl(asm: &mut Assembler, vd: VRegister, vn: VRegister, shift: i32) {
        asm.neon_shift_left_immediate(vd, vn, shift, NEON_UQSHL_imm);
    }
}

impl Assembler {
    pub fn sqshl<T: NeonShift>(&mut self, vd: VRegister, vn: VRegister, operand: T) {
        T::sqshl(self, vd, vn, operand);
    }

    pub fn uqshl<T: NeonShift>(&mut self, vd: VRegister, vn: VRegister, operand: T) {
        T::uqshl(self, vd, vn, operand);
    }
}

macro_rules! define_neon_fp_3_same {
    ($(($name:ident, $vec_op:ident, $sca_op:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, vd: VRegister, vn: VRegister, vm: VRegister) {
                    let op;
                    if vd.is_scalar() {
                        debug_assert!(vd.is_1s() || vd.is_1d());
                        op = $sca_op;
                    } else {
                        debug_assert!(vd.is_vector());
                        debug_assert!(
                            vd.is_2s() || vd.is_2d() || vd.is_4s() || vd.is_4h() || vd.is_8h()
                        );
                        op = $vec_op;
                    }
                    self.neon_fp_3_same(vd, vn, vm, op);
                }
            )*
        }
    };
}

define_neon_fp_3_same!(
    (fadd, NEON_FADD, FADD),
    (fsub, NEON_FSUB, FSUB),
    (fdiv, NEON_FDIV, FDIV),
    (fmax, NEON_FMAX, FMAX),
    (fmaxnm, NEON_FMAXNM, FMAXNM),
    (fmin, NEON_FMIN, FMIN),
    (fminnm, NEON_FMINNM, FMINNM),
    (frecps, NEON_FRECPS, NEON_FRECPS_scalar),
    (frsqrts, NEON_FRSQRTS, NEON_FRSQRTS_scalar),
    (fabd, NEON_FABD, NEON_FABD_scalar),
    (facge, NEON_FACGE, NEON_FACGE_scalar),
    (facgt, NEON_FACGT, NEON_FACGT_scalar),
    (fcmeq, NEON_FCMEQ, NEON_FCMEQ_scalar),
    (fcmge, NEON_FCMGE, NEON_FCMGE_scalar),
    (fcmgt, NEON_FCMGT, NEON_FCMGT_scalar),
);

macro_rules! define_neon_fp_3_same_vector_only {
    ($(($name:ident, $vec_op:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, vd: VRegister, vn: VRegister, vm: VRegister) {
                    debug_assert!(vd.is_vector());
                    debug_assert!(
                        vd.is_2s() || vd.is_2d() || vd.is_4s() || vd.is_4h() || vd.is_8h()
                    );
                    self.neon_fp_3_same(vd, vn, vm, $vec_op);
                }
            )*
        }
    };
}

define_neon_fp_3_same_vector_only!(
    (faddp, NEON_FADDP),
    (fmaxp, NEON_FMAXP),
    (fminp, NEON_FMINP),
    (fmaxnmp, NEON_FMAXNMP),
    (fminnmp, NEON_FMINNMP),
);

macro_rules! define_neon_fp_2_reg_misc {
    ($(($name:ident, $vec_op:ident, $sca_op:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, vd: VRegister, vn: VRegister) {
                    if vd.is_scalar() {
                        debug_assert!(vd.is_1s() || vd.is_1d());
                        self.neon_fp_2_reg_misc(vd, vn, $sca_op);
                    } else {
                        self.neon_fp_2_reg_misc_zero(vd, vn, $vec_op);
                    }
                }
            )*
        }
    };
}

define_neon_fp_2_reg_misc!(
    (fabs, NEON_FABS, FABS),
    (fneg, NEON_FNEG, FNEG),
    (fsqrt, NEON_FSQRT, FSQRT),
    (frintn, NEON_FRINTN, FRINTN),
    (frinta, NEON_FRINTA, FRINTA),
    (frintp, NEON_FRINTP, FRINTP),
    (frintm, NEON_FRINTM, FRINTM),
    (frintx, NEON_FRINTX, FRINTX),
    (frintz, NEON_FRINTZ, FRINTZ),
    (frinti, NEON_FRINTI, FRINTI),
    (frsqrte, NEON_FRSQRTE, NEON_FRSQRTE_scalar),
    (frecpe, NEON_FRECPE, NEON_FRECPE_scalar),
);

macro_rules! define_neon_fp_convert {
    ($(($name:ident, $op:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, vd: VRegister, vn: VRegister) {
                    self.neon_fp_2_reg_misc_zero(vd, vn, $op);
                }
            )*
        }
    };
}

define_neon_fp_convert!(
    (fcvtns, NEON_FCVTNS),
    (fcvtnu, NEON_FCVTNU),
    (fcvtps, NEON_FCVTPS),
    (fcvtpu, NEON_FCVTPU),
    (fcvtms, NEON_FCVTMS),
    (fcvtmu, NEON_FCVTMU),
    (fcvtas, NEON_FCVTAS),
    (fcvtau, NEON_FCVTAU),
);

impl Assembler {
    pub fn fmulx<T: Into<VOperand>>(&mut self, vd: VRegister, vn: VRegister, operand: T) {
        let operand = operand.into();
        let vm = operand.reg();
        if operand.has_lane() {
            self.neon_fp_by_element(vd, vn, vm, operand.index(), NEON_FMULX_byelement);
            return;
        }
        let op;
        if vd.is_scalar() {
            debug_assert!(vd.is_1s() || vd.is_1d());
            op = NEON_FMULX_scalar;
        } else {
            debug_assert!(vd.is_vector());
            op = NEON_FMULX;
        }
        self.neon_fp_3_same(vd, vn, vm, op);
    }
}

macro_rules! define_neon_xtn {
    ($(($name:ident, $op:ident, $vd:ident, $as:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, $vd: VRegister, vn: VRegister) {
                    debug_assert!($as);
                    self.neon_xtn($vd, vn, $op);
                }
            )*
        }
    };
}

define_neon_xtn!(
    (xtn, NEON_XTN, vd, vd.is_vector() && vd.is_d()),
    (xtn2, NEON_XTN, vd, vd.is_vector() && vd.is_q()),
    (sqxtn, NEON_SQXTN, vd, vd.is_scalar() || vd.is_d()),
    (sqxtn2, NEON_SQXTN, vd, vd.is_vector() && vd.is_q()),
    (uqxtn, NEON_UQXTN, vd, vd.is_scalar() || vd.is_d()),
    (uqxtn2, NEON_UQXTN, vd, vd.is_vector() && vd.is_q()),
    (sqxtun, NEON_SQXTUN, vd, vd.is_scalar() || vd.is_d()),
    (sqxtun2, NEON_SQXTUN, vd, vd.is_vector() && vd.is_q()),
);

macro_rules! define_neon_compare_zero {
    ($(($name:ident, $op:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, vd: VRegister, vn: VRegister, value: i32) {
                    debug_assert!(vd.is_vector() || vd.is_1d());
                    debug_assert_eq!(value, 0);
                    self.neon_2_reg_misc(vd, vn, $op);
                }
            )*
        }
    };
}

define_neon_compare_zero!(
    (cmle, NEON_CMLE_zero),
    (cmlt, NEON_CMLT_zero),
);

macro_rules! define_neon_fp_compare_zero {
    ($(($name:ident, $op:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, vd: VRegister, vn: VRegister, value: f64) {
                    debug_assert_eq!(value, 0.0);
                    self.neon_fp_2_reg_misc_zero(vd, vn, $op);
                }
            )*
        }
    };
}

define_neon_fp_compare_zero!(
    (fcmle, NEON_FCMLE_zero),
    (fcmlt, NEON_FCMLT_zero),
);

pub trait LoadStoreStructOperand {
    fn load_store_struct(
        asm: &mut Assembler,
        regs: Self,
        addr: MemOperand,
        op1: Instr,
        op2: Instr,
        op3: Instr,
        op4: Instr,
    );
}

impl LoadStoreStructOperand for &[VRegister] {
    fn load_store_struct(
        asm: &mut Assembler,
        regs: &[VRegister],
        addr: MemOperand,
        op1: Instr,
        op2: Instr,
        op3: Instr,
        op4: Instr,
    ) {
        debug_assert!(!regs.is_empty() && regs.len() <= 4);
        let vt = regs[0];
        debug_assert!(are_same_format(vt, &regs[1..]));
        for window in regs.windows(2) {
            debug_assert!(Assembler::are_consecutive_v(window[0], window[1]));
        }
        let op = match regs.len() {
            1 => op1,
            2 => op2,
            3 => op3,
            _ => op4,
        };
        asm.load_store_struct(vt, addr, op);
    }
}

macro_rules! define_load_store_struct {
    ($(($name:ident, $op1:ident, $op2:ident, $op3:ident, $op4:ident)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name<T: LoadStoreStructOperand>(&mut self, regs: T, addr: MemOperand) {
                    T::load_store_struct(self, regs, addr, $op1, $op2, $op3, $op4);
                }
            )*
        }
    };
}

define_load_store_struct!(
    (ld1, NEON_LD1_1v, NEON_LD1_2v, NEON_LD1_3v, NEON_LD1_4v),
    (st1, NEON_ST1_1v, NEON_ST1_2v, NEON_ST1_3v, NEON_ST1_4v),
);

macro_rules! define_load_store_struct_fixed {
    ($(($name:ident, $op:ident, $count:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, regs: &[VRegister], addr: MemOperand) {
                    debug_assert_eq!(regs.len(), $count);
                    let vt = regs[0];
                    debug_assert!(are_same_format(vt, &regs[1..]));
                    for window in regs.windows(2) {
                        debug_assert!(Assembler::are_consecutive_v(window[0], window[1]));
                    }
                    self.load_store_struct(vt, addr, $op);
                }
            )*
        }
    };
}

define_load_store_struct_fixed!(
    (ld2, NEON_LD2, 2),
    (ld3, NEON_LD3, 3),
    (ld4, NEON_LD4, 4),
    (st2, NEON_ST2, 2),
    (st3, NEON_ST3, 3),
    (st4, NEON_ST4, 4),
);

macro_rules! define_load_store_struct_all_lanes {
    ($(($name:ident, $op:ident, $count:expr)),* $(,)?) => {
        impl Assembler {
            $(
                pub fn $name(&mut self, regs: &[VRegister], addr: MemOperand) {
                    debug_assert_eq!(regs.len(), $count);
                    let vt = regs[0];
                    debug_assert!(are_same_format(vt, &regs[1..]));
                    self.load_store_struct_single_all_lanes(vt, addr, $op);
                }
            )*
        }
    };
}

define_load_store_struct_all_lanes!(
    (ld1r, NEON_LD1R, 1),
    (ld2r, NEON_LD2R, 2),
    (ld3r, NEON_LD3R, 3),
    (ld4r, NEON_LD4R, 4),
);

impl LoadStoreStructOperand for VOperand {
    fn load_store_struct(
        asm: &mut Assembler,
        operand: VOperand,
        addr: MemOperand,
        op1: Instr,
        _op2: Instr,
        _op3: Instr,
        _op4: Instr,
    ) {
        let single = if op1 == NEON_LD1_1v {
            NEONLoadStoreSingleStructLoad1
        } else {
            NEONLoadStoreSingleStructStore1
        };
        asm.load_store_struct_single(
            operand.reg(),
            operand.index() as u32,
            addr,
            single,
        );
    }
}

impl Assembler {
    pub fn are_consecutive_v(reg1: VRegister, reg2: VRegister) -> bool {
        (reg1.code() + 1) % kNumberOfVRegisters == reg2.code()
    }

    pub fn ld2_lane(&mut self, operand: VOperand, src: MemOperand) {
        self.load_store_struct_single(
            operand.reg(),
            operand.index() as u32,
            src,
            NEONLoadStoreSingleStructLoad2,
        );
    }

    pub fn ld3_lane(&mut self, operand: VOperand, src: MemOperand) {
        self.load_store_struct_single(
            operand.reg(),
            operand.index() as u32,
            src,
            NEONLoadStoreSingleStructLoad3,
        );
    }

    pub fn ld4_lane(&mut self, operand: VOperand, src: MemOperand) {
        self.load_store_struct_single(
            operand.reg(),
            operand.index() as u32,
            src,
            NEONLoadStoreSingleStructLoad4,
        );
    }

    pub fn st2_lane(&mut self, operand: VOperand, dst: MemOperand) {
        self.load_store_struct_single(
            operand.reg(),
            operand.index() as u32,
            dst,
            NEONLoadStoreSingleStructStore2,
        );
    }

    pub fn st3_lane(&mut self, operand: VOperand, dst: MemOperand) {
        self.load_store_struct_single(
            operand.reg(),
            operand.index() as u32,
            dst,
            NEONLoadStoreSingleStructStore3,
        );
    }

    pub fn st4_lane(&mut self, operand: VOperand, dst: MemOperand) {
        self.load_store_struct_single(
            operand.reg(),
            operand.index() as u32,
            dst,
            NEONLoadStoreSingleStructStore4,
        );
    }

    pub fn bcax(&mut self, vd: VRegister, vn: VRegister, vm: VRegister, va: VRegister) {
        debug_assert!(vd.is_16b() && vn.is_16b() && vm.is_16b() && va.is_16b());
        let instruction = NEON_BCAX
            | Assembler::rm(vm)
            | Assembler::ra(va)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn eor3(&mut self, vd: VRegister, vn: VRegister, vm: VRegister, va: VRegister) {
        debug_assert!(vd.is_16b() && vn.is_16b() && vm.is_16b() && va.is_16b());
        let instruction = NEON_EOR3
            | Assembler::rm(vm)
            | Assembler::ra(va)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn xar(&mut self, vd: VRegister, vn: VRegister, vm: VRegister, rotate: i32) {
        debug_assert!(vd.is_2d() && vn.is_2d() && vm.is_2d());
        debug_assert!(crate::utils::utils::is_uintn(rotate as i64, 6));
        let instruction = NEON_XAR
            | ((rotate as Instr) << 10)
            | Assembler::rm(vm)
            | Assembler::rn(vn)
            | Assembler::rd(vd);
        self.emit(instruction);
    }

    pub fn dc8(&mut self, data: u8) {
        self.base.emit_bytes(&[data]);
    }

    pub fn db(&mut self, data: u8) {
        self.block_pools();
        self.dc8(data);
        self.release_pools();
    }

    pub fn dci(&mut self, raw_instr: Instr) {
        self.block_pools();
        self.emit(raw_instr);
        self.release_pools();
    }

    pub fn dp(&mut self, data: u64) {
        self.block_pools();
        self.dc64(data);
        self.release_pools();
    }
}

impl Assembler {
    fn cpy(&mut self, op: Instr, rd: Register, rs: Register, rn: Register) {
        let instruction =
            op | Assembler::rd(rd) | Assembler::rs(rs) | Assembler::rn(rn);
        self.emit(instruction);
    }

    fn set(&mut self, op: Instr, rd: Register, rn: Register, rs: Register) {
        let instruction =
            op | Assembler::rd(rd) | Assembler::rn(rn) | Assembler::rs(rs);
        self.emit(instruction);
    }

    pub fn cpyp(&mut self, rd: Register, rs: Register, rn: Register) {
        self.cpy(CPYP, rd, rs, rn);
    }

    pub fn cpym(&mut self, rd: Register, rs: Register, rn: Register) {
        self.cpy(CPYM, rd, rs, rn);
    }

    pub fn cpye(&mut self, rd: Register, rs: Register, rn: Register) {
        self.cpy(CPYE, rd, rs, rn);
    }

    pub fn setp(&mut self, rd: Register, rn: Register, rs: Register) {
        self.set(SETP, rd, rn, rs);
    }

    pub fn setm(&mut self, rd: Register, rn: Register, rs: Register) {
        self.set(SETM, rd, rn, rs);
    }

    pub fn sete(&mut self, rd: Register, rn: Register, rs: Register) {
        self.set(SETE, rd, rn, rs);
    }
}
