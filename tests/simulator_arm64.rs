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
