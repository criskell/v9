#![allow(non_upper_case_globals)]

use vx::codegen::arm64::assembler_arm64::{Assembler, MemOperand, Operand};
use vx::codegen::arm64::constants_arm64::{AddrMode, BarrierDomain, BarrierType, Condition, Shift};
use vx::codegen::arm64::decoder_arm64::decode;
use vx::codegen::arm64::instructions_arm64::Instruction;
use vx::codegen::arm64::register_arm64::*;
use vx::diagnostics::arm64::disasm_arm64::DisassemblingDecoder;

fn disassemble(build: impl FnOnce(&mut Assembler)) -> Vec<String> {
    let mut assembler = Assembler::new();
    build(&mut assembler);
    assembler
        .buffer_start()
        .chunks(4)
        .map(|bytes| {
            let word = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            let mut decoder = DisassemblingDecoder::new();
            decode(&mut decoder, Instruction::new(word));
            decoder.get_output().to_string()
        })
        .collect()
}

fn one(build: impl FnOnce(&mut Assembler)) -> String {
    disassemble(build).remove(0)
}

#[test]
fn integer_arithmetic_disassembles() {
    assert_eq!(one(|a| a.add(x0, x1, 42)), "add x0, x1, #0x2a (42)");
    assert_eq!(one(|a| a.sub(w2, w3, w4)), "sub w2, w3, w4");
    assert_eq!(
        one(|a| a.adds(x5, x6, Operand::with_shift(x7, Shift::Lsl, 3))),
        "adds x5, x6, x7, lsl #3"
    );
    assert_eq!(one(|a| a.and_(x8, x9, 0xff)), "and x8, x9, #0xff");
    assert_eq!(one(|a| a.mul(x10, x11, x12)), "mul x10, x11, x12");
    assert_eq!(one(|a| a.cmp(x13, x14)), "cmp x13, x14");
    assert_eq!(one(|a| a.neg(x15, x16)), "neg x15, x16");
}

#[test]
fn moves_and_bitfields_disassemble() {
    assert_eq!(one(|a| a.movz(x10, 0x1234, 16)), "movz x10, #0x12340000");
    assert_eq!(one(|a| a.movk(x1, 0x5678, 0)), "movk x1, #0x5678");
    assert_eq!(one(|a| a.ubfx(x2, x3, 4, 8)), "ubfx x2, x3, #4, #8");
    assert_eq!(one(|a| a.sxtb(x4, w5)), "sxtb x4, w5");
    assert_eq!(one(|a| a.lsl(x6, x7, 3)), "lsl x6, x7, #3");
}

#[test]
fn loads_and_stores_disassemble() {
    assert_eq!(
        one(|a| a.ldr(x11, MemOperand::new(sp, 8, AddrMode::Offset))),
        "ldr x11, [sp, #8]"
    );
    assert_eq!(
        one(|a| a.str(w12, MemOperand::new(x13, 16, AddrMode::PreIndex))),
        "str w12, [x13, #16]!"
    );
    assert_eq!(
        one(|a| a.ldp(x14, x15, MemOperand::new(x16, 32, AddrMode::Offset))),
        "ldp x14, x15, [x16, #32]"
    );
    assert_eq!(
        one(|a| a.ldrb(w0, MemOperand::new(x1, 1, AddrMode::PostIndex))),
        "ldrb w0, [x1], #1"
    );
}

#[test]
fn branches_disassemble() {
    assert_eq!(one(|a| a.b_cond(8, Condition::Eq)), "b.eq #+0x20");
    assert_eq!(one(|a| a.cbz(x17, 4)), "cbz x17, #+0x10");
    assert_eq!(one(|a| a.tbz(x18, 3, 2)), "tbz w18, #3, #+0x8");
    assert_eq!(one(|a| a.ret(lr)), "ret");
    assert_eq!(one(|a| a.blr(x1)), "blr x1");
}

#[test]
fn floating_point_disassembles() {
    assert_eq!(one(|a| a.fadd(d0, d1, d2)), "fadd d0, d1, d2");
    assert_eq!(one(|a| a.fmul(s3, s4, s5)), "fmul s3, s4, s5");
    assert_eq!(one(|a| a.fcvtzs(w6, d7, 0)), "fcvtzs w6, d7");
    assert_eq!(one(|a| a.fsqrt(d8, d9)), "fsqrt d8, d9");
    assert_eq!(one(|a| a.fcmp(d10, d11)), "fcmp d10, d11");
}

#[test]
fn vector_instructions_disassemble() {
    assert_eq!(
        one(|a| a.add(v0.v16b(), v1.v16b(), v2.v16b())),
        "add v0.16b, v1.16b, v2.16b"
    );
    assert_eq!(
        one(|a| a.mul(v3.v4s(), v4.v4s(), v5.v4s())),
        "mul v3.4s, v4.4s, v5.4s"
    );
    assert_eq!(
        one(|a| a.umull(v6.v8h(), v7.v8b(), v8.v8b())),
        "umull v6.8h, v7.8b, v8.8b"
    );
    assert_eq!(
        one(|a| a.fadd(v9.v2d(), v10.v2d(), v11.v2d())),
        "fadd v9.2d, v10.2d, v11.2d"
    );
    assert_eq!(
        one(|a| a.cmeq(v12.v8b(), v13.v8b(), v14.v8b())),
        "cmeq v12.8b, v13.8b, v14.8b"
    );
    assert_eq!(
        one(|a| a.shl(v15.v4h(), v16.v4h(), 3)),
        "shl v15.4h, v16.4h, #3"
    );
    assert_eq!(
        one(|a| a.tbl(v17.v8b(), v18.v16b(), v19.v8b())),
        "tbl v17.8b, {v18.16b}, v19.8b"
    );
    assert_eq!(
        one(|a| a.zip1(v20.v4s(), v21.v4s(), v22.v4s())),
        "zip1 v20.4s, v21.4s, v22.4s"
    );
    assert_eq!(one(|a| a.addv(b23, v24.v8b())), "addv b23, v24.8b");
    assert_eq!(
        one(|a| a.dup(v25.v2d(), v26.v1d(), 0)),
        "dup v25.2d, v26.d[0]"
    );
}

#[test]
fn vector_memory_and_atomics_disassemble() {
    assert_eq!(
        one(|a| a.st1(
            &[v27.v16b()][..],
            MemOperand::new(x28, 0, AddrMode::Offset)
        )),
        "st1 {v27.16b}, [x28]"
    );
    assert_eq!(
        one(|a| a.ldadd(x1, x2, MemOperand::new(x3, 0, AddrMode::Offset))),
        "ldadd x1, x2, [x3]"
    );
    assert_eq!(
        one(|a| a.cas(x4, x5, MemOperand::new(x6, 0, AddrMode::Offset))),
        "cas x4, x5, [x6]"
    );
    assert_eq!(
        one(|a| a.ldaxr(x7, x8)),
        "ldaxr x7, [x8]"
    );
}

#[test]
fn system_instructions_disassemble() {
    assert_eq!(one(|a| a.nop()), "nop");
    assert_eq!(one(|a| a.brk(7)), "brk #0x7");
    assert_eq!(one(|a| a.hlt(1)), "hlt #0x1");
    assert_eq!(one(|a| a.dmb(BarrierDomain::InnerShareable, BarrierType::BarrierAll)), "dmb ish");
}
