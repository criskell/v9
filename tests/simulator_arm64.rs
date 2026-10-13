#![allow(non_upper_case_globals)]

use vx::codegen::arm64::assembler_arm64::{Assembler, MemOperand, Operand};
use vx::codegen::arm64::constants_arm64::{AddrMode, Condition, Shift};
#[allow(unused_imports)]
use vx::codegen::arm64::register_arm64::{d0, d1, d2, d3, d4, d5, d6, d7, s0, s1, s2, s3};
use vx::codegen::arm64::register_arm64::*;
use vx::execution::arm64::simulator_arm64::Simulator;

fn run(build: impl FnOnce(&mut Assembler)) -> Simulator {
    let mut assembler = Assembler::new();
    build(&mut assembler);
    assembler.brk(0);
    let mut simulator = Simulator::new(assembler.buffer_start());
    simulator.run_from(0);
    assert_eq!(simulator.unsupported_instruction(), None);
    simulator
}

#[test]
fn an_add_of_two_immediates() {
    let simulator = run(|a| {
        a.movz(x0, 12, -1);
        a.movz(x1, 30, -1);
        a.add(x2, x0, x1);
    });
    assert_eq!(simulator.xreg(2), 42);
}

#[test]
fn a_move_wide_builds_a_sixty_four_bit_constant() {
    let simulator = run(|a| {
        a.movz(x0, 0xcdef, 0);
        a.movk(x0, 0x89ab, 16);
        a.movk(x0, 0x4567, 32);
        a.movk(x0, 0x0123, 48);
        a.movn(x1, 0, -1);
    });
    assert_eq!(simulator.xreg(0), 0x0123456789abcdef);
    assert_eq!(simulator.xreg(1), u64::MAX);
}

#[test]
fn subtraction_sets_the_flags_and_a_conditional_select_reads_them() {
    let simulator = run(|a| {
        a.movz(x0, 7, -1);
        a.movz(x1, 9, -1);
        a.cmp(x0, x1);
        a.cset(x2, Condition::Lt);
        a.cset(x3, Condition::Gt);
        a.csel(x4, x0, x1, Condition::Lt);
    });
    assert_eq!(simulator.xreg(2), 1);
    assert_eq!(simulator.xreg(3), 0);
    assert_eq!(simulator.xreg(4), 7);
}

#[test]
fn the_logical_immediate_round_trips_through_the_encoder() {
    let simulator = run(|a| {
        a.movn(x0, 0, -1);
        a.and_(x1, x0, 0xffi64);
        a.orr(x2, xzr, 0x5555555555555555u64 as i64);
        a.eor(x3, x0, 0xffi64);
    });
    assert_eq!(simulator.xreg(1), 0xff);
    assert_eq!(simulator.xreg(2), 0x5555555555555555);
    assert_eq!(simulator.xreg(3), !0xffu64);
}

#[test]
fn shifts_and_bitfield_extracts() {
    let simulator = run(|a| {
        a.movz(x0, 0xabcd, -1);
        a.lsl(x1, x0, 4);
        a.lsr(x2, x0, 4);
        a.ubfx(x3, x0, 4, 8);
        a.movn(x4, 0, -1);
        a.asr(x5, x4, 1);
        a.sxtb(x6, w0);
    });
    assert_eq!(simulator.xreg(1), 0xabcd0);
    assert_eq!(simulator.xreg(2), 0xabc);
    assert_eq!(simulator.xreg(3), 0xbc);
    assert_eq!(simulator.xreg(5), u64::MAX);
    assert_eq!(simulator.xreg(6), 0xffffffffffffffcd);
}

#[test]
fn multiplication_and_division() {
    let simulator = run(|a| {
        a.movz(x0, 7, -1);
        a.movz(x1, 6, -1);
        a.mul(x2, x0, x1);
        a.movz(x3, 100, -1);
        a.sdiv(x4, x3, x0);
        a.udiv(x5, x3, x0);
        a.msub(x6, x4, x0, x3);
        a.movz(x7, 0, -1);
        a.sdiv(x8, x3, x7);
    });
    assert_eq!(simulator.xreg(2), 42);
    assert_eq!(simulator.xreg(4), 14);
    assert_eq!(simulator.xreg(5), 14);
    assert_eq!(simulator.xreg(6), 2);
    assert_eq!(simulator.xreg(8), 0);
}

#[test]
fn a_loop_that_sums_the_first_ten_integers() {
    let simulator = run(|a| {
        let top = a.new_label();
        let done = a.new_label();
        a.movz(x0, 0, -1);
        a.movz(x1, 1, -1);
        a.bind(top);
        a.cmp(x1, 10);
        a.b_cond(done, Condition::Gt);
        a.add(x0, x0, x1);
        a.add(x1, x1, 1);
        a.b(top);
        a.bind(done);
    });
    assert_eq!(simulator.xreg(0), 55);
    assert_eq!(simulator.xreg(1), 11);
}

#[test]
fn a_store_followed_by_a_load_round_trips_through_memory() {
    let simulator = run(|a| {
        a.movz(x0, 0x1234, -1);
        a.sub(sp, sp, 32);
        a.str(x0, MemOperand::new(sp, 8, AddrMode::Offset));
        a.ldr(x1, MemOperand::new(sp, 8, AddrMode::Offset));
        a.strb(w0, MemOperand::new(sp, 16, AddrMode::Offset));
        a.ldrb(w2, MemOperand::new(sp, 16, AddrMode::Offset));
        a.movn(x3, 0, -1);
        a.strh(w3, MemOperand::new(sp, 20, AddrMode::Offset));
        a.ldrsh(x4, MemOperand::new(sp, 20, AddrMode::Offset));
        a.add(sp, sp, 32);
    });
    assert_eq!(simulator.xreg(1), 0x1234);
    assert_eq!(simulator.xreg(2), 0x34);
    assert_eq!(simulator.xreg(4), u64::MAX);
}

#[test]
fn a_pair_store_and_load_with_writeback() {
    let simulator = run(|a| {
        a.movz(x0, 11, -1);
        a.movz(x1, 22, -1);
        a.stp(x0, x1, MemOperand::new(sp, -16, AddrMode::PreIndex));
        a.ldp(x2, x3, MemOperand::new(sp, 16, AddrMode::PostIndex));
    });
    assert_eq!(simulator.xreg(2), 11);
    assert_eq!(simulator.xreg(3), 22);
}

#[test]
fn a_call_and_return_through_the_link_register() {
    let simulator = run(|a| {
        let callee = a.new_label();
        let after = a.new_label();
        a.movz(x0, 3, -1);
        a.bl(callee);
        a.b(after);
        a.bind(callee);
        a.add(x0, x0, x0);
        a.ret(lr);
        a.bind(after);
        a.add(x0, x0, 1);
    });
    assert_eq!(simulator.xreg(0), 7);
}

#[test]
fn a_compare_and_branch_on_zero() {
    let simulator = run(|a| {
        let skip = a.new_label();
        a.movz(x0, 0, -1);
        a.movz(x1, 1, -1);
        a.cbz(x0, skip);
        a.movz(x1, 99, -1);
        a.bind(skip);
        let skip2 = a.new_label();
        a.movz(x2, 8, -1);
        a.tbnz(x2, 3, skip2);
        a.movz(x2, 77, -1);
        a.bind(skip2);
    });
    assert_eq!(simulator.xreg(1), 1);
    assert_eq!(simulator.xreg(2), 8);
}

#[test]
fn a_literal_load_reads_the_constant_pool() {
    let simulator = run(|a| {
        a.ldr_literal(x0, 0x0123456789abcdef);
        a.ldr_literal(w1, 0xdeadbeef);
        a.force_constant_pool_emission();
    });
    assert_eq!(simulator.xreg(0), 0x0123456789abcdef);
    assert_eq!(simulator.xreg(1), 0xdeadbeef);
}

#[test]
fn an_extended_register_add_and_a_shifted_register_add() {
    let simulator = run(|a| {
        a.movn(x0, 0, -1);
        a.movz(x1, 1, -1);
        a.add(x2, x1, Operand::with_shift(x1, Shift::Lsl, 4));
        a.add(x3, x1, Operand::with_extend(w0, vx::codegen::arm64::constants_arm64::Extend::Uxtb, 0));
    });
    assert_eq!(simulator.xreg(2), 17);
    assert_eq!(simulator.xreg(3), 256);
}

#[test]
fn an_address_of_a_label_is_loaded_by_adr() {
    let simulator = run(|a| {
        let here = a.new_label();
        a.adr(x0, here);
        a.nop();
        a.bind(here);
    });
    assert_eq!(simulator.xreg(0), 8);
}

#[test]
fn floating_point_arithmetic_executes() {
    let simulator = run(|a| {
        a.fmov_immediate(d0, 1.5);
        a.fmov_immediate(d1, 0.5);
        a.fadd(d2, d0, d1);
        a.fsub(d3, d0, d1);
        a.fmul(d4, d0, d1);
        a.fdiv(d5, d0, d1);
        a.fnmul(d6, d0, d1);
    });
    assert_eq!(simulator.dreg(2), 2.0);
    assert_eq!(simulator.dreg(3), 1.0);
    assert_eq!(simulator.dreg(4), 0.75);
    assert_eq!(simulator.dreg(5), 3.0);
    assert_eq!(simulator.dreg(6), -0.75);
}

#[test]
fn single_precision_arithmetic_executes() {
    let simulator = run(|a| {
        a.fmov_immediate(s0, 1.5);
        a.fmov_immediate(s1, 2.0);
        a.fadd(s2, s0, s1);
        a.fdiv(s3, s0, s1);
    });
    assert_eq!(simulator.sreg(2), 3.5);
    assert_eq!(simulator.sreg(3), 0.75);
}

#[test]
fn floating_point_unary_operations_execute() {
    let simulator = run(|a| {
        a.fmov_immediate(d0, -2.0);
        a.fabs(d1, d0);
        a.fneg(d2, d0);
        a.fmov_immediate(d3, 9.0);
        a.fsqrt(d4, d3);
        a.fmov(d5, d3);
    });
    assert_eq!(simulator.dreg(1), 2.0);
    assert_eq!(simulator.dreg(2), 2.0);
    assert_eq!(simulator.dreg(4), 3.0);
    assert_eq!(simulator.dreg(5), 9.0);
}

#[test]
fn the_rounding_modes_execute() {
    let simulator = run(|a| {
        a.fmov_immediate(d0, 2.5);
        a.frintn(d1, d0);
        a.frintp(d2, d0);
        a.frintm(d3, d0);
        a.frintz(d4, d0);
        a.frinta(d5, d0);
        a.fmov_immediate(d6, 3.5);
        a.frintn(d7, d6);
    });
    assert_eq!(simulator.dreg(1), 2.0);
    assert_eq!(simulator.dreg(2), 3.0);
    assert_eq!(simulator.dreg(3), 2.0);
    assert_eq!(simulator.dreg(4), 2.0);
    assert_eq!(simulator.dreg(5), 3.0);
    assert_eq!(simulator.dreg(7), 4.0);
}

#[test]
fn a_floating_point_comparison_sets_the_flags() {
    let simulator = run(|a| {
        a.fmov_immediate(d0, 1.0);
        a.fmov_immediate(d1, 2.0);
        a.fcmp(d0, d1);
        a.cset(x0, Condition::Mi);
        a.fcmp(d1, d0);
        a.cset(x1, Condition::Gt);
        a.fcmp(d0, d0);
        a.cset(x2, Condition::Eq);
        a.fcmp_zero(d0, 0.0);
        a.cset(x3, Condition::Ne);
    });
    assert_eq!(simulator.xreg(0), 1);
    assert_eq!(simulator.xreg(1), 1);
    assert_eq!(simulator.xreg(2), 1);
    assert_eq!(simulator.xreg(3), 1);
}

#[test]
fn fused_multiply_add_executes() {
    let simulator = run(|a| {
        a.fmov_immediate(d0, 2.0);
        a.fmov_immediate(d1, 3.0);
        a.fmov_immediate(d2, 4.0);
        a.fmadd(d3, d0, d1, d2);
        a.fmsub(d4, d0, d1, d2);
        a.fnmadd(d5, d0, d1, d2);
        a.fnmsub(d6, d0, d1, d2);
    });
    assert_eq!(simulator.dreg(3), 10.0);
    assert_eq!(simulator.dreg(4), -2.0);
    assert_eq!(simulator.dreg(5), -10.0);
    assert_eq!(simulator.dreg(6), 2.0);
}

#[test]
fn conversions_between_integer_and_floating_point_execute() {
    let simulator = run(|a| {
        a.movz(x0, 42, -1);
        a.scvtf(d0, x0, 0);
        a.ucvtf(d1, x0, 0);
        a.fmov_immediate(d2, 3.75);
        a.fcvtzs(x1, d2, 0);
        a.fcvtzu(x2, d2, 0);
        a.movn(x3, 0, -1);
        a.scvtf(d3, x3, 0);
        a.fcvtzs(x4, d3, 0);
        a.scvtf(d4, x0, 2);
    });
    assert_eq!(simulator.dreg(0), 42.0);
    assert_eq!(simulator.dreg(1), 42.0);
    assert_eq!(simulator.xreg(1), 3);
    assert_eq!(simulator.xreg(2), 3);
    assert_eq!(simulator.dreg(3), -1.0);
    assert_eq!(simulator.xreg(4), u64::MAX);
    assert_eq!(simulator.dreg(4), 10.5);
}

#[test]
fn bit_moves_between_integer_and_floating_point_registers() {
    let simulator = run(|a| {
        a.fmov_immediate(d0, 1.5);
        a.fmov_to_register(x0, d0);
        a.fmov_from_register(d1, x0);
        a.fmov_immediate(s2, 2.5);
        a.fmov_to_register(w1, s2);
        a.fmov_from_register(s3, w1);
    });
    assert_eq!(simulator.xreg(0), 1.5f64.to_bits());
    assert_eq!(simulator.dreg(1), 1.5);
    assert_eq!(simulator.wreg(1), 2.5f32.to_bits());
    assert_eq!(simulator.sreg(3), 2.5);
}

#[test]
fn conversions_between_the_floating_point_sizes_execute() {
    let simulator = run(|a| {
        a.fmov_immediate(s0, 1.5);
        a.fcvt(d1, s0);
        a.fmov_immediate(d2, 2.5);
        a.fcvt(s3, d2);
    });
    assert_eq!(simulator.dreg(1), 1.5);
    assert_eq!(simulator.sreg(3), 2.5);
}

#[test]
fn a_double_loaded_from_the_constant_pool() {
    let simulator = run(|a| {
        a.ldr_literal(d0, 1.2345f64.to_bits());
        a.fmov_immediate(d1, 2.0);
        a.fmul(d2, d0, d1);
        a.force_constant_pool_emission();
    });
    assert_eq!(simulator.dreg(0), 1.2345);
    assert_eq!(simulator.dreg(2), 2.469);
}

#[test]
fn floating_point_maximum_and_minimum() {
    let simulator = run(|a| {
        a.fmov_immediate(d0, 1.0);
        a.fmov_immediate(d1, 2.0);
        a.fmax(d2, d0, d1);
        a.fmin(d3, d0, d1);
        a.fmaxnm(d4, d0, d1);
        a.fminnm(d5, d0, d1);
    });
    assert_eq!(simulator.dreg(2), 2.0);
    assert_eq!(simulator.dreg(3), 1.0);
    assert_eq!(simulator.dreg(4), 2.0);
    assert_eq!(simulator.dreg(5), 1.0);
}

#[test]
fn a_double_stored_and_loaded_through_memory() {
    let simulator = run(|a| {
        a.fmov_immediate(d0, 1.5);
        a.sub(sp, sp, 16);
        a.str(d0, MemOperand::new(sp, 8, AddrMode::Offset));
        a.ldr(d1, MemOperand::new(sp, 8, AddrMode::Offset));
        a.add(sp, sp, 16);
    });
    assert_eq!(simulator.dreg(1), 1.5);
}

fn lanes8(simulator: &Simulator, code: u32) -> [u8; 16] {
    simulator.qreg(code).val
}

fn lanes16(simulator: &Simulator, code: u32) -> [u16; 8] {
    let bytes = simulator.qreg(code).val;
    let mut lanes = [0u16; 8];
    for (i, lane) in lanes.iter_mut().enumerate() {
        *lane = u16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]]);
    }
    lanes
}

fn lanes32(simulator: &Simulator, code: u32) -> [u32; 4] {
    let bytes = simulator.qreg(code).val;
    let mut lanes = [0u32; 4];
    for (i, lane) in lanes.iter_mut().enumerate() {
        *lane = u32::from_le_bytes([
            bytes[4 * i],
            bytes[4 * i + 1],
            bytes[4 * i + 2],
            bytes[4 * i + 3],
        ]);
    }
    lanes
}

fn lanes64(simulator: &Simulator, code: u32) -> [u64; 2] {
    let bytes = simulator.qreg(code).val;
    let mut lanes = [0u64; 2];
    for (i, lane) in lanes.iter_mut().enumerate() {
        let mut word = [0u8; 8];
        word.copy_from_slice(&bytes[8 * i..8 * i + 8]);
        *lane = u64::from_le_bytes(word);
    }
    lanes
}

fn movi_16b(a: &mut Assembler, reg: VRegister, bytes: [u8; 16]) {
    let scratch = x9;
    let mut low = 0u64;
    let mut high = 0u64;
    for i in 0..8 {
        low |= (bytes[i] as u64) << (8 * i);
        high |= (bytes[8 + i] as u64) << (8 * i);
    }
    for (index, half) in [low, high].iter().enumerate() {
        a.movz(scratch, *half & 0xffff, 0);
        a.movk(scratch, (*half >> 16) & 0xffff, 16);
        a.movk(scratch, (*half >> 32) & 0xffff, 32);
        a.movk(scratch, (*half >> 48) & 0xffff, 48);
        a.ins_from_register(reg.v1d(), index as i32, scratch);
    }
}

#[test]
fn a_vector_add_of_sixteen_bytes() {
    let simulator = run(|a| {
        movi_16b(a, v0, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        movi_16b(a, v1, [16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1]);
        a.add(v2.v16b(), v0.v16b(), v1.v16b());
    });
    assert_eq!(lanes8(&simulator, 2), [17; 16]);
}

#[test]
fn a_vector_multiply_of_four_words() {
    let simulator = run(|a| {
        movi_16b(a, v0, [2, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0, 5, 0, 0, 0]);
        movi_16b(a, v1, [10, 0, 0, 0, 10, 0, 0, 0, 10, 0, 0, 0, 10, 0, 0, 0]);
        a.mul(v2.v4s(), v0.v4s(), v1.v4s());
    });
    assert_eq!(lanes32(&simulator, 2), [20, 30, 40, 50]);
}

#[test]
fn vector_logical_operations() {
    let simulator = run(|a| {
        movi_16b(a, v0, [0xf0; 16]);
        movi_16b(a, v1, [0x3c; 16]);
        a.and_(v2.v16b(), v0.v16b(), v1.v16b());
        a.orr(v3.v16b(), v0.v16b(), v1.v16b());
        a.eor(v4.v16b(), v0.v16b(), v1.v16b());
        a.bic(v5.v16b(), v0.v16b(), v1.v16b());
    });
    assert_eq!(lanes8(&simulator, 2), [0x30; 16]);
    assert_eq!(lanes8(&simulator, 3), [0xfc; 16]);
    assert_eq!(lanes8(&simulator, 4), [0xcc; 16]);
    assert_eq!(lanes8(&simulator, 5), [0xc0; 16]);
}

#[test]
fn a_saturating_vector_add_clamps_each_lane() {
    let simulator = run(|a| {
        movi_16b(a, v0, [0x7f, 0x7f, 0x80, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        movi_16b(a, v1, [0x10, 0x01, 0xf0, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        a.sqadd(v2.v16b(), v0.v16b(), v1.v16b());
        a.uqadd(v3.v16b(), v0.v16b(), v1.v16b());
    });
    let signed = lanes8(&simulator, 2);
    assert_eq!(&signed[..4], &[0x7f, 0x7f, 0x80, 0x80]);
    let unsigned = lanes8(&simulator, 3);
    assert_eq!(&unsigned[..4], &[0x8f, 0x80, 0xff, 0xff]);
}

#[test]
fn vector_min_max_and_across_lanes() {
    let simulator = run(|a| {
        movi_16b(a, v0, [3, 9, 1, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        movi_16b(a, v1, [5, 2, 8, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        a.smax(v2.v8b(), v0.v8b(), v1.v8b());
        a.smin(v3.v8b(), v0.v8b(), v1.v8b());
        a.addv(b4, v0.v8b());
        a.umaxv(b5, v0.v8b());
    });
    assert_eq!(&lanes8(&simulator, 2)[..4], &[5, 9, 8, 7]);
    assert_eq!(&lanes8(&simulator, 3)[..4], &[3, 2, 1, 4]);
    assert_eq!(lanes8(&simulator, 4)[0], 20);
    assert_eq!(lanes8(&simulator, 5)[0], 9);
}

#[test]
fn a_vector_shift_left_by_immediate() {
    let simulator = run(|a| {
        movi_16b(a, v0, [1, 2, 3, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        a.shl(v1.v8b(), v0.v8b(), 2);
        a.ushr(v2.v8b(), v0.v8b(), 1);
    });
    assert_eq!(&lanes8(&simulator, 1)[..4], &[4, 8, 12, 16]);
    assert_eq!(&lanes8(&simulator, 2)[..4], &[0, 1, 1, 2]);
}

#[test]
fn a_widening_vector_multiply() {
    let simulator = run(|a| {
        movi_16b(a, v0, [0xff, 0x10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        movi_16b(a, v1, [0x02, 0x10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        a.umull(v2.v8h(), v0.v8b(), v1.v8b());
    });
    assert_eq!(&lanes16(&simulator, 2)[..2], &[0x1fe, 0x100]);
}

#[test]
fn a_vector_zip_and_transpose() {
    let simulator = run(|a| {
        movi_16b(a, v0, [1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 0, 0, 0, 0]);
        movi_16b(a, v1, [9, 10, 11, 12, 13, 14, 15, 16, 0, 0, 0, 0, 0, 0, 0, 0]);
        a.zip1(v2.v8b(), v0.v8b(), v1.v8b());
        a.trn1(v3.v8b(), v0.v8b(), v1.v8b());
    });
    assert_eq!(&lanes8(&simulator, 2)[..8], &[1, 9, 2, 10, 3, 11, 4, 12]);
    assert_eq!(&lanes8(&simulator, 3)[..8], &[1, 9, 3, 11, 5, 13, 7, 15]);
}

#[test]
fn a_vector_floating_point_add_and_multiply() {
    let simulator = run(|a| {
        a.fmov_immediate(d0, 1.5);
        a.fmov_immediate(d1, 2.5);
        a.dup(v2.v2d(), v0.v1d(), 0);
        a.dup(v3.v2d(), v1.v1d(), 0);
        a.fadd(v4.v2d(), v2.v2d(), v3.v2d());
        a.fmul(v5.v2d(), v2.v2d(), v3.v2d());
    });
    let sum = lanes64(&simulator, 4);
    assert_eq!(f64::from_bits(sum[0]), 4.0);
    assert_eq!(f64::from_bits(sum[1]), 4.0);
    let product = lanes64(&simulator, 5);
    assert_eq!(f64::from_bits(product[0]), 3.75);
}

#[test]
fn a_vector_store_and_load_round_trip() {
    let simulator = run(|a| {
        movi_16b(a, v0, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        a.sub(x10, sp, 64);
        a.st1(&[v0.v16b()][..], MemOperand::new(x10, 0, AddrMode::Offset));
        a.ld1(&[v1.v16b()][..], MemOperand::new(x10, 0, AddrMode::Offset));
    });
    assert_eq!(
        lanes8(&simulator, 1),
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
    );
}

#[test]
fn a_table_lookup_selects_bytes() {
    let simulator = run(|a| {
        movi_16b(a, v0, [10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 0, 0, 0, 0]);
        movi_16b(a, v1, [3, 0, 11, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        a.tbl(v2.v8b(), v0.v16b(), v1.v8b());
    });
    assert_eq!(&lanes8(&simulator, 2)[..4], &[40, 10, 120, 0]);
}

#[test]
fn a_vector_compare_sets_all_ones() {
    let simulator = run(|a| {
        movi_16b(a, v0, [1, 2, 3, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        movi_16b(a, v1, [1, 5, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        a.cmeq(v2.v8b(), v0.v8b(), v1.v8b());
        a.cmgt(v3.v8b(), v0.v8b(), v1.v8b());
    });
    assert_eq!(&lanes8(&simulator, 2)[..4], &[0xff, 0, 0xff, 0]);
    assert_eq!(&lanes8(&simulator, 3)[..4], &[0, 0, 0, 0xff]);
}

#[test]
fn an_atomic_add_returns_the_old_value() {
    let simulator = run(|a| {
        a.sub(x10, sp, 64);
        a.movz(x0, 7, -1);
        a.str(x0, MemOperand::new(x10, 0, AddrMode::Offset));
        a.movz(x1, 35, -1);
        a.ldadd(x1, x2, MemOperand::new(x10, 0, AddrMode::Offset));
        a.ldr(x3, MemOperand::new(x10, 0, AddrMode::Offset));
    });
    assert_eq!(simulator.xreg(2), 7);
    assert_eq!(simulator.xreg(3), 42);
}

#[test]
fn a_compare_and_swap_updates_memory_on_a_match() {
    let simulator = run(|a| {
        a.sub(x10, sp, 64);
        a.movz(x0, 7, -1);
        a.str(x0, MemOperand::new(x10, 0, AddrMode::Offset));
        a.movz(x1, 7, -1);
        a.movz(x2, 42, -1);
        a.cas(x1, x2, MemOperand::new(x10, 0, AddrMode::Offset));
        a.ldr(x3, MemOperand::new(x10, 0, AddrMode::Offset));
    });
    assert_eq!(simulator.xreg(1), 7);
    assert_eq!(simulator.xreg(3), 42);
}
