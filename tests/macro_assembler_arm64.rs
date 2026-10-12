use vx::codegen::arm64::assembler_arm64::{MemOperand, Operand};
use vx::codegen::arm64::constants_arm64::{AddrMode, Shift};
use vx::codegen::arm64::macro_assembler_arm64::MacroAssembler;
use vx::codegen::arm64::register_arm64::*;
use vx::execution::arm64::simulator_arm64::Simulator;

fn words(masm: &MacroAssembler) -> Vec<u32> {
    masm.buffer_start()
        .chunks(4)
        .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect()
}

fn assemble(build: impl FnOnce(&mut MacroAssembler)) -> Vec<u32> {
    let mut masm = MacroAssembler::new();
    build(&mut masm);
    words(&masm)
}

fn run(build: impl FnOnce(&mut MacroAssembler)) -> Simulator {
    let mut masm = MacroAssembler::new();
    build(&mut masm);
    masm.asm().brk(0);
    let mut simulator = Simulator::new(masm.buffer_start());
    simulator.run_from(0);
    assert_eq!(simulator.unsupported_instruction(), None);
    simulator
}

#[test]
fn a_small_immediate_move_is_one_instruction() {
    assert_eq!(assemble(|m| m.mov_immediate(X0, 1234)), vec![0xd2809a40]);
    assert_eq!(
        assemble(|m| m.mov_immediate(X0, u64::MAX)),
        vec![0x92800000]
    );
    assert_eq!(
        assemble(|m| m.mov_immediate(X0, 0x5555555555555555)),
        vec![0xb200f3e0]
    );
}

#[test]
fn a_register_move_and_a_shifted_move() {
    assert_eq!(assemble(|m| m.mov(X0, X1)), vec![0xaa0103e0]);
    assert_eq!(assemble(|m| m.mov(W0, W1)), vec![0x2a0103e0]);
    assert_eq!(
        assemble(|m| m.mov(X0, Operand::with_shift(X1, Shift::Lsl, 4))),
        vec![0xd37cec20]
    );
}

#[test]
fn an_add_of_a_shifted_immediate_stays_one_instruction() {
    assert_eq!(assemble(|m| m.add(X0, X1, 4096i64)), vec![0x91400420]);
    assert_eq!(assemble(|m| m.sub(SP, SP, 32i64)), vec![0xd10083ff]);
    assert_eq!(assemble(|m| m.add(SP, SP, 32i64)), vec![0x910083ff]);
}

#[test]
fn an_add_of_zero_to_itself_is_elided() {
    assert_eq!(assemble(|m| m.add(X0, X0, 0i64)), Vec::<u32>::new());
    assert_eq!(assemble(|m| m.claim(0, 8)), Vec::<u32>::new());
    assert_eq!(assemble(|m| m.drop(0, 8)), Vec::<u32>::new());
}

#[test]
fn a_push_and_a_pop_of_one_register() {
    assert_eq!(assemble(|m| m.push(&[X0])), vec![0xf81f8fe0]);
    assert_eq!(assemble(|m| m.pop(&[X0])), vec![0xf84087e0]);
}

#[test]
fn a_push_and_a_pop_of_a_pair() {
    assert_eq!(assemble(|m| m.push(&[X0, X1])), vec![0xa9bf03e1]);
    assert_eq!(assemble(|m| m.pop(&[X0, X1])), vec![0xa8c107e0]);
}

#[test]
fn a_wide_immediate_move_expands_to_several_instructions() {
    let simulator = run(|m| {
        m.mov_immediate(X0, 0x0123456789abcdef);
        m.mov_immediate(X1, 0xffff0000ffff0000);
        m.mov_immediate(X2, 1);
        m.mov_immediate(W3, 0xdeadbeef);
    });
    assert_eq!(simulator.xreg(0), 0x0123456789abcdef);
    assert_eq!(simulator.xreg(1), 0xffff0000ffff0000);
    assert_eq!(simulator.xreg(2), 1);
    assert_eq!(simulator.xreg(3), 0xdeadbeef);
}

#[test]
fn an_inverted_move_is_chosen_when_it_is_shorter() {
    let masm_count = assemble(|m| m.mov_immediate(X0, 0xffffffffffff0000)).len();
    assert_eq!(masm_count, 1);
    let simulator = run(|m| m.mov_immediate(X0, 0xffffffffffff0000));
    assert_eq!(simulator.xreg(0), 0xffffffffffff0000);
}

#[test]
fn an_add_of_an_immediate_too_large_for_the_field_uses_a_scratch() {
    let simulator = run(|m| {
        m.mov_immediate(X1, 1000);
        m.add(X0, X1, 0x123456i64);
        m.sub(X2, X1, 0x123456i64);
    });
    assert_eq!(simulator.xreg(0), 1000 + 0x123456);
    assert_eq!(simulator.xreg(2), (1000i64 - 0x123456) as u64);
}

#[test]
fn a_logical_immediate_that_does_not_encode_uses_a_scratch() {
    let simulator = run(|m| {
        m.mov_immediate(X1, u64::MAX);
        m.and_(X0, X1, 0x12345i64);
        m.orr(X2, XZR, 0x12345i64);
        m.eor(X3, X1, 0x12345i64);
    });
    assert_eq!(simulator.xreg(0), 0x12345);
    assert_eq!(simulator.xreg(2), 0x12345);
    assert_eq!(simulator.xreg(3), !0x12345u64);
}

#[test]
fn the_special_cases_of_a_logical_immediate() {
    let simulator = run(|m| {
        m.mov_immediate(X1, 0xabcd);
        m.and_(X0, X1, 0i64);
        m.orr(X2, X1, 0i64);
        m.and_(X3, X1, -1i64);
        m.eor(X4, X1, -1i64);
    });
    assert_eq!(simulator.xreg(0), 0);
    assert_eq!(simulator.xreg(2), 0xabcd);
    assert_eq!(simulator.xreg(3), 0xabcd);
    assert_eq!(simulator.xreg(4), !0xabcdu64);
}

#[test]
fn a_load_with_an_offset_beyond_the_field_uses_a_scratch() {
    let simulator = run(|m| {
        m.mov_immediate(X0, 0xcafe);
        m.sub(SP, SP, 0x200i64);
        m.mov(X1, SP);
        m.str(X0, MemOperand::new(X1, 300, AddrMode::Offset));
        m.ldr(X2, MemOperand::new(X1, 300, AddrMode::Offset));
        m.add(SP, SP, 0x200i64);
    });
    assert_eq!(simulator.xreg(2), 0xcafe);
}

#[test]
fn claim_and_drop_move_the_stack_pointer() {
    let simulator = run(|m| {
        m.mov(X0, SP);
        m.claim(2, 8);
        m.mov(X1, SP);
        m.drop(2, 8);
        m.mov(X2, SP);
    });
    assert_eq!(simulator.xreg(0) - simulator.xreg(1), 16);
    assert_eq!(simulator.xreg(0), simulator.xreg(2));
}

#[test]
fn a_push_and_pop_round_trip_through_the_stack() {
    let simulator = run(|m| {
        m.mov_immediate(X0, 11);
        m.mov_immediate(X1, 22);
        m.push(&[X0, X1]);
        m.mov_immediate(X0, 0);
        m.mov_immediate(X1, 0);
        m.pop(&[X2, X3]);
    });
    assert_eq!(simulator.xreg(2), 22);
    assert_eq!(simulator.xreg(3), 11);
}

#[test]
fn a_scratch_register_is_returned_when_the_scope_closes() {
    let simulator = run(|m| {
        m.mov_immediate(X1, 1);
        m.add(X0, X1, 0x123456i64);
        m.add(X2, X1, 0x123456i64);
        m.add(X3, X1, 0x123456i64);
    });
    assert_eq!(simulator.xreg(0), 1 + 0x123456);
    assert_eq!(simulator.xreg(2), 1 + 0x123456);
    assert_eq!(simulator.xreg(3), 1 + 0x123456);
}

#[test]
fn an_extended_register_operand_is_expanded() {
    let simulator = run(|m| {
        m.mov_immediate(X1, 0xff);
        m.mov_immediate(X2, 0xabcd);
        m.and_(
            X0,
            X2,
            Operand::with_extend(W1, vx::codegen::arm64::constants_arm64::Extend::Uxtb, 0),
        );
    });
    assert_eq!(simulator.xreg(0), 0xcd);
}

#[test]
fn a_comparison_through_the_macro_assembler() {
    let simulator = run(|m| {
        m.mov_immediate(X0, 0x123456);
        m.cmp(X0, 0x123456i64);
        m.asm().cset(X1, vx::codegen::arm64::constants_arm64::Condition::Eq);
        m.tst(X0, 0x123456i64);
        m.asm().cset(X2, vx::codegen::arm64::constants_arm64::Condition::Ne);
    });
    assert_eq!(simulator.xreg(1), 1);
    assert_eq!(simulator.xreg(2), 1);
}
