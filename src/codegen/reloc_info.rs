#![allow(non_upper_case_globals)]

use crate::base::vlq::{vlq_decode_unsigned, vlq_encode_unsigned};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Mode {
    NoInfo = 0,

    CodeTarget,
    RelativeCodeTarget,
    CompressedEmbeddedObject,
    FullEmbeddedObject,
    JsDispatchHandle,

    WasmCall,
    WasmStubCall,
    WasmCodePointerTableEntry,
    WasmCanonicalSigId,
    WasmCodePointer,

    ExternalReference,
    InternalReference,

    InternalReferenceEncoded,

    OffHeapTarget,
    NearBuiltinEntry,

    ConstPool,
    VeneerPool,

    DeoptScriptOffset,
    DeoptInliningId,
    DeoptReason,
    DeoptId,
    DeoptNodeId,

    PcJump,

    NumberOfModes,
}

pub const LAST_CODE_TARGET_MODE: Mode = Mode::RelativeCodeTarget;
pub const FIRST_REAL_RELOC_MODE: Mode = Mode::CodeTarget;
pub const LAST_REAL_RELOC_MODE: Mode = Mode::VeneerPool;
pub const FIRST_EMBEDDED_OBJECT_RELOC_MODE: Mode = Mode::CompressedEmbeddedObject;
pub const LAST_EMBEDDED_OBJECT_RELOC_MODE: Mode = Mode::FullEmbeddedObject;
pub const LAST_GCED_ENUM: Mode = Mode::JsDispatchHandle;
pub const FIRST_BUILTIN_ENTRY_MODE: Mode = Mode::OffHeapTarget;
pub const LAST_BUILTIN_ENTRY_MODE: Mode = Mode::NearBuiltinEntry;
pub const FIRST_SHAREABLE_RELOC_MODE: Mode = Mode::WasmCall;
pub const FIRST_DEOPT_MODE: Mode = Mode::DeoptScriptOffset;
pub const LAST_DEOPT_MODE: Mode = Mode::DeoptNodeId;

const kTagBits: u32 = 2;
const kTagMask: u32 = (1 << kTagBits) - 1;
const kLongTagBits: u32 = 6;

const kEmbeddedObjectTag: u32 = 0;
const kCodeTargetTag: u32 = 1;
const kWasmStubCallTag: u32 = 2;
const kDefaultTag: u32 = 3;

const kSmallPCDeltaBits: u32 = 8 - kTagBits;
const kSmallPCDeltaMask: u32 = (1 << kSmallPCDeltaBits) - 1;

const kIntSize: usize = 4;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct RelocInfo {
    pc: i32,
    rmode: Mode,
    data: i64,
}

impl RelocInfo {
    pub fn new(pc: i32, rmode: Mode, data: i64) -> RelocInfo {
        RelocInfo { pc, rmode, data }
    }

    pub fn pc(&self) -> i32 {
        self.pc
    }

    pub fn rmode(&self) -> Mode {
        self.rmode
    }

    pub fn data(&self) -> i64 {
        self.data
    }

    pub fn is_real_reloc_mode(mode: Mode) -> bool {
        mode >= FIRST_REAL_RELOC_MODE && mode <= LAST_REAL_RELOC_MODE
    }

    pub fn is_gc_reloc_mode(mode: Mode) -> bool {
        mode <= LAST_GCED_ENUM
    }

    pub fn is_shareable_reloc_mode(mode: Mode) -> bool {
        mode == Mode::NoInfo || mode >= FIRST_SHAREABLE_RELOC_MODE
    }

    pub fn is_code_target(mode: Mode) -> bool {
        mode == Mode::CodeTarget
    }

    pub fn is_code_target_mode(mode: Mode) -> bool {
        mode <= LAST_CODE_TARGET_MODE
    }

    pub fn is_relative_code_target(mode: Mode) -> bool {
        mode == Mode::RelativeCodeTarget
    }

    pub fn is_full_embedded_object(mode: Mode) -> bool {
        mode == Mode::FullEmbeddedObject
    }

    pub fn is_embedded_object_mode(mode: Mode) -> bool {
        (mode >= FIRST_EMBEDDED_OBJECT_RELOC_MODE)
            && (mode <= LAST_EMBEDDED_OBJECT_RELOC_MODE)
    }

    pub fn is_const_pool(mode: Mode) -> bool {
        mode == Mode::ConstPool
    }

    pub fn is_veneer_pool(mode: Mode) -> bool {
        mode == Mode::VeneerPool
    }

    pub fn is_deopt_mode(mode: Mode) -> bool {
        mode >= FIRST_DEOPT_MODE && mode <= LAST_DEOPT_MODE
    }

    pub fn is_deopt_position(mode: Mode) -> bool {
        mode == Mode::DeoptScriptOffset || mode == Mode::DeoptInliningId
    }

    pub fn is_deopt_reason(mode: Mode) -> bool {
        mode == Mode::DeoptReason
    }

    pub fn is_deopt_id(mode: Mode) -> bool {
        mode == Mode::DeoptId
    }

    pub fn is_deopt_node_id(mode: Mode) -> bool {
        mode == Mode::DeoptNodeId
    }

    pub fn is_external_reference(mode: Mode) -> bool {
        mode == Mode::ExternalReference
    }

    pub fn is_internal_reference(mode: Mode) -> bool {
        mode == Mode::InternalReference
    }

    pub fn is_internal_reference_encoded(mode: Mode) -> bool {
        mode == Mode::InternalReferenceEncoded
    }

    pub fn is_off_heap_target(mode: Mode) -> bool {
        mode == Mode::OffHeapTarget
    }

    pub fn is_near_builtin_entry(mode: Mode) -> bool {
        mode == Mode::NearBuiltinEntry
    }

    pub fn is_builtin_entry_mode(mode: Mode) -> bool {
        (mode >= FIRST_BUILTIN_ENTRY_MODE) && (mode <= LAST_BUILTIN_ENTRY_MODE)
    }

    pub fn mode_mask(mode: Mode) -> i32 {
        1 << (mode as i32)
    }
}

fn mode_from_value(value: u32) -> Mode {
    debug_assert!(value < Mode::NumberOfModes as u32);
    const MODES: [Mode; Mode::NumberOfModes as usize] = [
        Mode::NoInfo,
        Mode::CodeTarget,
        Mode::RelativeCodeTarget,
        Mode::CompressedEmbeddedObject,
        Mode::FullEmbeddedObject,
        Mode::JsDispatchHandle,
        Mode::WasmCall,
        Mode::WasmStubCall,
        Mode::WasmCodePointerTableEntry,
        Mode::WasmCanonicalSigId,
        Mode::WasmCodePointer,
        Mode::ExternalReference,
        Mode::InternalReference,
        Mode::InternalReferenceEncoded,
        Mode::OffHeapTarget,
        Mode::NearBuiltinEntry,
        Mode::ConstPool,
        Mode::VeneerPool,
        Mode::DeoptScriptOffset,
        Mode::DeoptInliningId,
        Mode::DeoptReason,
        Mode::DeoptId,
        Mode::DeoptNodeId,
        Mode::PcJump,
    ];
    MODES[value as usize]
}

#[derive(Default)]
pub struct RelocInfoWriter {
    buffer: Vec<u8>,
    last_pc: i32,
}

impl RelocInfoWriter {
    pub const kMaxSize: usize = 1 + 4 + 1 + 1 + 8;

    pub fn new() -> RelocInfoWriter {
        RelocInfoWriter {
            buffer: Vec::new(),
            last_pc: 0,
        }
    }

    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    pub fn last_pc(&self) -> i32 {
        self.last_pc
    }

    fn write_byte(&mut self, byte: u8) {
        self.buffer.push(byte);
    }

    fn write_long_pc_jump(&mut self, pc_delta: u32) -> u32 {
        if crate::utils::utils::is_uintn(pc_delta as i64, kSmallPCDeltaBits) {
            return pc_delta;
        }
        self.write_mode(Mode::PcJump);
        let pc_jump = pc_delta >> kSmallPCDeltaBits;
        debug_assert!(pc_jump > 0);
        let mut bytes = Vec::new();
        vlq_encode_unsigned(&mut |byte| bytes.push(byte), pc_jump);
        for byte in bytes {
            self.write_byte(byte);
        }
        pc_delta & kSmallPCDeltaMask
    }

    fn write_short_tagged_pc(&mut self, pc_delta: u32, tag: u32) {
        let pc_delta = self.write_long_pc_jump(pc_delta);
        self.write_byte((pc_delta << kTagBits | tag) as u8);
    }

    fn write_short_data(&mut self, data_delta: u8) {
        self.write_byte(data_delta);
    }

    fn write_mode(&mut self, rmode: Mode) {
        debug_assert!((Mode::NumberOfModes as u32) <= (1 << kLongTagBits));
        self.write_byte((((rmode as u32) << kTagBits) | kDefaultTag) as u8);
    }

    fn write_mode_and_pc(&mut self, pc_delta: u32, rmode: Mode) {
        let pc_delta = self.write_long_pc_jump(pc_delta);
        self.write_mode(rmode);
        self.write_byte(pc_delta as u8);
    }

    fn write_int_data(&mut self, number: i32) {
        let mut number = number;
        for _ in 0..kIntSize {
            self.write_byte(number as u8);
            number >>= 8;
        }
    }

    pub fn write(&mut self, rinfo: &RelocInfo) {
        let rmode = rinfo.rmode();
        debug_assert!(rmode < Mode::NumberOfModes);
        debug_assert!(rinfo.pc() - self.last_pc >= 0);
        let pc_delta = (rinfo.pc() - self.last_pc) as u32;

        if rmode == Mode::FullEmbeddedObject {
            self.write_short_tagged_pc(pc_delta, kEmbeddedObjectTag);
        } else if rmode == Mode::CodeTarget {
            self.write_short_tagged_pc(pc_delta, kCodeTargetTag);
        } else if rmode == Mode::WasmStubCall {
            self.write_short_tagged_pc(pc_delta, kWasmStubCallTag);
        } else {
            self.write_mode_and_pc(pc_delta, rmode);
            if RelocInfo::is_deopt_reason(rmode) {
                debug_assert!(rinfo.data() < (1 << 8));
                self.write_short_data(rinfo.data() as u8);
            } else if RelocInfo::is_const_pool(rmode)
                || RelocInfo::is_veneer_pool(rmode)
                || RelocInfo::is_deopt_id(rmode)
                || RelocInfo::is_deopt_position(rmode)
                || RelocInfo::is_deopt_node_id(rmode)
            {
                self.write_int_data(rinfo.data() as i32);
            }
        }
        self.last_pc = rinfo.pc();
    }
}

pub struct RelocIterator<'a> {
    buffer: &'a [u8],
    position: usize,
    rinfo: RelocInfo,
    mode_mask: i32,
    done: bool,
}

impl<'a> RelocIterator<'a> {
    pub const kAllModesMask: i32 = -1;

    pub fn new(buffer: &'a [u8], mode_mask: i32) -> RelocIterator<'a> {
        let mut iterator = RelocIterator {
            buffer,
            position: 0,
            rinfo: RelocInfo::new(0, Mode::NoInfo, 0),
            mode_mask,
            done: false,
        };
        iterator.next();
        iterator
    }

    pub fn done(&self) -> bool {
        self.done
    }

    pub fn rinfo(&self) -> &RelocInfo {
        debug_assert!(!self.done());
        &self.rinfo
    }

    fn advance(&mut self) {
        self.position += 1;
    }

    fn advance_by(&mut self, bytes: usize) {
        self.position += bytes;
    }

    fn advance_get_tag(&mut self) -> u32 {
        let tag = self.buffer[self.position] as u32 & kTagMask;
        self.advance();
        tag
    }

    fn get_mode(&self) -> Mode {
        mode_from_value((self.buffer[self.position - 1] as u32 >> kTagBits)
            & ((1 << kLongTagBits) - 1))
    }

    fn read_short_tagged_pc(&mut self) {
        self.rinfo.pc += (self.buffer[self.position - 1] >> kTagBits) as i32;
    }

    fn advance_read_pc(&mut self) {
        self.rinfo.pc += self.buffer[self.position] as i32;
        self.advance();
    }

    fn advance_read_int(&mut self) {
        let mut x: i32 = 0;
        for i in 0..kIntSize {
            x |= (self.buffer[self.position] as i32) << (i * 8);
            self.advance();
        }
        self.rinfo.data = x as i64;
    }

    fn advance_read_long_pc_jump(&mut self) {
        let buffer = self.buffer;
        let mut position = self.position;
        let pc_jump = vlq_decode_unsigned(&mut || {
            let byte = buffer[position];
            position += 1;
            byte
        });
        self.position = position;
        self.rinfo.pc += (pc_jump << kSmallPCDeltaBits) as i32;
    }

    fn read_short_data(&mut self) {
        self.rinfo.data = self.buffer[self.position - 1] as i64;
    }

    fn set_mode(&mut self, mode: Mode) -> bool {
        self.rinfo.rmode = mode;
        (RelocInfo::mode_mask(mode) & self.mode_mask) != 0
    }

    pub fn next(&mut self) {
        debug_assert!(!self.done);
        while self.position < self.buffer.len() {
            let tag = self.advance_get_tag();
            if tag == kEmbeddedObjectTag {
                self.read_short_tagged_pc();
                if self.set_mode(Mode::FullEmbeddedObject) {
                    return;
                }
            } else if tag == kCodeTargetTag {
                self.read_short_tagged_pc();
                if self.set_mode(Mode::CodeTarget) {
                    return;
                }
            } else if tag == kWasmStubCallTag {
                self.read_short_tagged_pc();
                if self.set_mode(Mode::WasmStubCall) {
                    return;
                }
            } else {
                debug_assert_eq!(tag, kDefaultTag);
                let rmode = self.get_mode();
                if rmode == Mode::PcJump {
                    self.advance_read_long_pc_jump();
                } else {
                    self.advance_read_pc();
                    if RelocInfo::is_deopt_reason(rmode) {
                        self.advance();
                        if self.set_mode(rmode) {
                            self.read_short_data();
                            return;
                        }
                    } else if RelocInfo::is_const_pool(rmode)
                        || RelocInfo::is_veneer_pool(rmode)
                        || RelocInfo::is_deopt_id(rmode)
                        || RelocInfo::is_deopt_position(rmode)
                        || RelocInfo::is_deopt_node_id(rmode)
                    {
                        if self.set_mode(rmode) {
                            self.advance_read_int();
                            return;
                        }
                        self.advance_by(kIntSize);
                    } else if self.set_mode(rmode) {
                        return;
                    }
                }
            }
        }
        self.done = true;
    }
}
