use vx::codegen::arm64::assembler_arm64::{Assembler, MemOperand, Operand};
use vx::codegen::arm64::constants_arm64::{AddrMode, Condition, Shift};
#[allow(unused_imports)]
use vx::codegen::arm64::register_arm64::{D0, D1, D2, D3, D4, D5, D6, D7, S0, S1, S2, S3};
use vx::codegen::arm64::register_arm64::*;
use vx::codegen::label::Label;
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
        a.movz(X0, 12, -1);
        a.movz(X1, 30, -1);
        a.add(X2, X0, X1);
    });
    assert_eq!(simulator.xreg(2), 42);
}

#[test]
fn a_move_wide_builds_a_sixty_four_bit_constant() {
    let simulator = run(|a| {
        a.movz(X0, 0xcdef, 0);
        a.movk(X0, 0x89ab, 16);
        a.movk(X0, 0x4567, 32);
        a.movk(X0, 0x0123, 48);
        a.movn(X1, 0, -1);
    });
    assert_eq!(simulator.xreg(0), 0x0123456789abcdef);
    assert_eq!(simulator.xreg(1), u64::MAX);
}

#[test]
fn subtraction_sets_the_flags_and_a_conditional_select_reads_them() {
    let simulator = run(|a| {
        a.movz(X0, 7, -1);
        a.movz(X1, 9, -1);
        a.cmp(X0, X1);
        a.cset(X2, Condition::Lt);
        a.cset(X3, Condition::Gt);
        a.csel(X4, X0, X1, Condition::Lt);
    });
    assert_eq!(simulator.xreg(2), 1);
    assert_eq!(simulator.xreg(3), 0);
    assert_eq!(simulator.xreg(4), 7);
}

#[test]
fn the_logical_immediate_round_trips_through_the_encoder() {
    let simulator = run(|a| {
        a.movn(X0, 0, -1);
        a.and_(X1, X0, 0xffi64);
        a.orr(X2, XZR, 0x5555555555555555u64 as i64);
        a.eor(X3, X0, 0xffi64);
    });
    assert_eq!(simulator.xreg(1), 0xff);
    assert_eq!(simulator.xreg(2), 0x5555555555555555);
    assert_eq!(simulator.xreg(3), !0xffu64);
}

#[test]
fn shifts_and_bitfield_extracts() {
    let simulator = run(|a| {
        a.movz(X0, 0xabcd, -1);
        a.lsl(X1, X0, 4);
        a.lsr(X2, X0, 4);
        a.ubfx(X3, X0, 4, 8);
        a.movn(X4, 0, -1);
        a.asr(X5, X4, 1);
        a.sxtb(X6, W0);
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
        a.movz(X0, 7, -1);
        a.movz(X1, 6, -1);
        a.mul(X2, X0, X1);
        a.movz(X3, 100, -1);
        a.sdiv(X4, X3, X0);
        a.udiv(X5, X3, X0);
        a.msub(X6, X4, X0, X3);
        a.movz(X7, 0, -1);
        a.sdiv(X8, X3, X7);
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
        let mut top = Label::new();
        let mut done = Label::new();
        a.movz(X0, 0, -1);
        a.movz(X1, 1, -1);
        a.bind(&mut top);
        a.cmp(X1, 10);
        a.b_cond_label(&mut done, Condition::Gt);
        a.add(X0, X0, X1);
        a.add(X1, X1, 1);
        a.b_label(&mut top);
        a.bind(&mut done);
    });
    assert_eq!(simulator.xreg(0), 55);
    assert_eq!(simulator.xreg(1), 11);
}

#[test]
fn a_store_followed_by_a_load_round_trips_through_memory() {
    let simulator = run(|a| {
        a.movz(X0, 0x1234, -1);
        a.sub(SP, SP, 32);
        a.str(X0, MemOperand::new(SP, 8, AddrMode::Offset));
        a.ldr(X1, MemOperand::new(SP, 8, AddrMode::Offset));
        a.strb(W0, MemOperand::new(SP, 16, AddrMode::Offset));
        a.ldrb(W2, MemOperand::new(SP, 16, AddrMode::Offset));
        a.movn(X3, 0, -1);
        a.strh(W3, MemOperand::new(SP, 20, AddrMode::Offset));
        a.ldrsh(X4, MemOperand::new(SP, 20, AddrMode::Offset));
        a.add(SP, SP, 32);
    });
    assert_eq!(simulator.xreg(1), 0x1234);
    assert_eq!(simulator.xreg(2), 0x34);
    assert_eq!(simulator.xreg(4), u64::MAX);
}

#[test]
fn a_pair_store_and_load_with_writeback() {
    let simulator = run(|a| {
        a.movz(X0, 11, -1);
        a.movz(X1, 22, -1);
        a.stp(X0, X1, MemOperand::new(SP, -16, AddrMode::PreIndex));
        a.ldp(X2, X3, MemOperand::new(SP, 16, AddrMode::PostIndex));
    });
    assert_eq!(simulator.xreg(2), 11);
    assert_eq!(simulator.xreg(3), 22);
}

#[test]
fn a_call_and_return_through_the_link_register() {
    let simulator = run(|a| {
        let mut callee = Label::new();
        let mut after = Label::new();
        a.movz(X0, 3, -1);
        a.bl_label(&mut callee);
        a.b_label(&mut after);
        a.bind(&mut callee);
        a.add(X0, X0, X0);
        a.ret(LR);
        a.bind(&mut after);
        a.add(X0, X0, 1);
    });
    assert_eq!(simulator.xreg(0), 7);
}

#[test]
fn a_compare_and_branch_on_zero() {
    let simulator = run(|a| {
        let mut skip = Label::new();
        a.movz(X0, 0, -1);
        a.movz(X1, 1, -1);
        a.cbz_label(X0, &mut skip);
        a.movz(X1, 99, -1);
        a.bind(&mut skip);
        let mut skip2 = Label::new();
        a.movz(X2, 8, -1);
        a.tbnz_label(X2, 3, &mut skip2);
        a.movz(X2, 77, -1);
        a.bind(&mut skip2);
    });
    assert_eq!(simulator.xreg(1), 1);
    assert_eq!(simulator.xreg(2), 8);
}

#[test]
fn a_literal_load_reads_the_constant_pool() {
    let simulator = run(|a| {
        a.ldr_literal(X0, 0x0123456789abcdef);
        a.ldr_literal(W1, 0xdeadbeef);
        a.force_constant_pool_emission();
    });
    assert_eq!(simulator.xreg(0), 0x0123456789abcdef);
    assert_eq!(simulator.xreg(1), 0xdeadbeef);
}

#[test]
fn an_extended_register_add_and_a_shifted_register_add() {
    let simulator = run(|a| {
        a.movn(X0, 0, -1);
        a.movz(X1, 1, -1);
        a.add(X2, X1, Operand::with_shift(X1, Shift::Lsl, 4));
        a.add(X3, X1, Operand::with_extend(W0, vx::codegen::arm64::constants_arm64::Extend::Uxtb, 0));
    });
    assert_eq!(simulator.xreg(2), 17);
    assert_eq!(simulator.xreg(3), 256);
}

#[test]
fn an_address_of_a_label_is_loaded_by_adr() {
    let simulator = run(|a| {
        let mut here = Label::new();
        a.adr_label(X0, &mut here);
        a.nop();
        a.bind(&mut here);
    });
    assert_eq!(simulator.xreg(0), 8);
}

#[test]
fn floating_point_arithmetic_executes() {
    let simulator = run(|a| {
        a.fmov_immediate(D0, 1.5);
        a.fmov_immediate(D1, 0.5);
        a.fadd(D2, D0, D1);
        a.fsub(D3, D0, D1);
        a.fmul(D4, D0, D1);
        a.fdiv(D5, D0, D1);
        a.fnmul(D6, D0, D1);
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
        a.fmov_immediate(S0, 1.5);
        a.fmov_immediate(S1, 2.0);
        a.fadd(S2, S0, S1);
        a.fdiv(S3, S0, S1);
    });
    assert_eq!(simulator.sreg(2), 3.5);
    assert_eq!(simulator.sreg(3), 0.75);
}

#[test]
fn floating_point_unary_operations_execute() {
    let simulator = run(|a| {
        a.fmov_immediate(D0, -2.0);
        a.fabs(D1, D0);
        a.fneg(D2, D0);
        a.fmov_immediate(D3, 9.0);
        a.fsqrt(D4, D3);
        a.fmov(D5, D3);
    });
    assert_eq!(simulator.dreg(1), 2.0);
    assert_eq!(simulator.dreg(2), 2.0);
    assert_eq!(simulator.dreg(4), 3.0);
    assert_eq!(simulator.dreg(5), 9.0);
}

#[test]
fn the_rounding_modes_execute() {
    let simulator = run(|a| {
        a.fmov_immediate(D0, 2.5);
        a.frintn(D1, D0);
        a.frintp(D2, D0);
        a.frintm(D3, D0);
        a.frintz(D4, D0);
        a.frinta(D5, D0);
        a.fmov_immediate(D6, 3.5);
        a.frintn(D7, D6);
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
        a.fmov_immediate(D0, 1.0);
        a.fmov_immediate(D1, 2.0);
        a.fcmp(D0, D1);
        a.cset(X0, Condition::Mi);
        a.fcmp(D1, D0);
        a.cset(X1, Condition::Gt);
        a.fcmp(D0, D0);
        a.cset(X2, Condition::Eq);
        a.fcmp_zero(D0, 0.0);
        a.cset(X3, Condition::Ne);
    });
    assert_eq!(simulator.xreg(0), 1);
    assert_eq!(simulator.xreg(1), 1);
    assert_eq!(simulator.xreg(2), 1);
    assert_eq!(simulator.xreg(3), 1);
}

#[test]
fn fused_multiply_add_executes() {
    let simulator = run(|a| {
        a.fmov_immediate(D0, 2.0);
        a.fmov_immediate(D1, 3.0);
        a.fmov_immediate(D2, 4.0);
        a.fmadd(D3, D0, D1, D2);
        a.fmsub(D4, D0, D1, D2);
        a.fnmadd(D5, D0, D1, D2);
        a.fnmsub(D6, D0, D1, D2);
    });
    assert_eq!(simulator.dreg(3), 10.0);
    assert_eq!(simulator.dreg(4), -2.0);
    assert_eq!(simulator.dreg(5), -10.0);
    assert_eq!(simulator.dreg(6), 2.0);
}

#[test]
fn conversions_between_integer_and_floating_point_execute() {
    let simulator = run(|a| {
        a.movz(X0, 42, -1);
        a.scvtf(D0, X0, 0);
        a.ucvtf(D1, X0, 0);
        a.fmov_immediate(D2, 3.75);
        a.fcvtzs(X1, D2, 0);
        a.fcvtzu(X2, D2, 0);
        a.movn(X3, 0, -1);
        a.scvtf(D3, X3, 0);
        a.fcvtzs(X4, D3, 0);
        a.scvtf(D4, X0, 2);
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
        a.fmov_immediate(D0, 1.5);
        a.fmov_to_register(X0, D0);
        a.fmov_from_register(D1, X0);
        a.fmov_immediate(S2, 2.5);
        a.fmov_to_register(W1, S2);
        a.fmov_from_register(S3, W1);
    });
    assert_eq!(simulator.xreg(0), 1.5f64.to_bits());
    assert_eq!(simulator.dreg(1), 1.5);
    assert_eq!(simulator.wreg(1), 2.5f32.to_bits());
    assert_eq!(simulator.sreg(3), 2.5);
}

#[test]
fn conversions_between_the_floating_point_sizes_execute() {
    let simulator = run(|a| {
        a.fmov_immediate(S0, 1.5);
        a.fcvt(D1, S0);
        a.fmov_immediate(D2, 2.5);
        a.fcvt(S3, D2);
    });
    assert_eq!(simulator.dreg(1), 1.5);
    assert_eq!(simulator.sreg(3), 2.5);
}

#[test]
fn a_double_loaded_from_the_constant_pool() {
    let simulator = run(|a| {
        a.ldr_literal(D0, 1.2345f64.to_bits());
        a.fmov_immediate(D1, 2.0);
        a.fmul(D2, D0, D1);
        a.force_constant_pool_emission();
    });
    assert_eq!(simulator.dreg(0), 1.2345);
    assert_eq!(simulator.dreg(2), 2.469);
}

#[test]
fn floating_point_maximum_and_minimum() {
    let simulator = run(|a| {
        a.fmov_immediate(D0, 1.0);
        a.fmov_immediate(D1, 2.0);
        a.fmax(D2, D0, D1);
        a.fmin(D3, D0, D1);
        a.fmaxnm(D4, D0, D1);
        a.fminnm(D5, D0, D1);
    });
    assert_eq!(simulator.dreg(2), 2.0);
    assert_eq!(simulator.dreg(3), 1.0);
    assert_eq!(simulator.dreg(4), 2.0);
    assert_eq!(simulator.dreg(5), 1.0);
}

#[test]
fn a_double_stored_and_loaded_through_memory() {
    let simulator = run(|a| {
        a.fmov_immediate(D0, 1.5);
        a.sub(SP, SP, 16);
        a.str(D0, MemOperand::new(SP, 8, AddrMode::Offset));
        a.ldr(D1, MemOperand::new(SP, 8, AddrMode::Offset));
        a.add(SP, SP, 16);
    });
    assert_eq!(simulator.dreg(1), 1.5);
}
