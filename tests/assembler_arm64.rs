use vx::codegen::arm64::assembler_arm64::{Assembler, MemOperand, Operand};
use vx::codegen::arm64::constants_arm64::{AddrMode, Condition, Shift, SystemHint};
use vx::codegen::arm64::register_arm64::*;
use vx::codegen::label::Label;

fn words(assembler: &Assembler) -> Vec<u32> {
    assembler
        .buffer_start()
        .chunks(4)
        .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect()
}

fn assemble(build: impl FnOnce(&mut Assembler)) -> Vec<u32> {
    let mut assembler = Assembler::new();
    build(&mut assembler);
    words(&assembler)
}

#[test]
fn add_and_sub_of_two_registers() {
    assert_eq!(assemble(|a| a.add(X0, X1, X2)), vec![0x8b020020]);
    assert_eq!(assemble(|a| a.add(W0, W1, W2)), vec![0x0b020020]);
    assert_eq!(assemble(|a| a.sub(X5, X6, X7)), vec![0xcb0700c5]);
    assert_eq!(assemble(|a| a.subs(W8, W9, W10)), vec![0x6b0a0128]);
    assert_eq!(
        assemble(|a| a.add(X0, X1, Operand::with_shift(X2, Shift::Lsl, 3))),
        vec![0x8b020c20]
    );
}

#[test]
fn add_of_an_immediate_uses_the_shifted_form_when_it_has_to() {
    assert_eq!(assemble(|a| a.add(X0, X1, 42)), vec![0x9100a820]);
    assert_eq!(assemble(|a| a.add(X0, X1, 4096)), vec![0x91400420]);
    assert_eq!(assemble(|a| a.adds(X3, X4, 1)), vec![0xb1000483]);
}

#[test]
fn a_comparison_writes_to_the_zero_register() {
    assert_eq!(assemble(|a| a.cmp(X0, 0)), vec![0xf100001f]);
    assert_eq!(assemble(|a| a.cmp(X0, X1)), vec![0xeb01001f]);
    assert_eq!(assemble(|a| a.cmn(X0, 1)), vec![0xb100041f]);
    assert_eq!(assemble(|a| a.neg(X0, X1)), vec![0xcb0103e0]);
}

#[test]
fn logical_operations_of_two_registers() {
    assert_eq!(assemble(|a| a.and_(X0, X1, X2)), vec![0x8a020020]);
    assert_eq!(assemble(|a| a.orr(X0, X1, X2)), vec![0xaa020020]);
    assert_eq!(assemble(|a| a.eor(X0, X1, X2)), vec![0xca020020]);
}

#[test]
fn a_register_move_is_an_or_with_the_zero_register() {
    assert_eq!(assemble(|a| a.mov(X0, X1)), vec![0xaa0103e0]);
    assert_eq!(assemble(|a| a.mov(W0, W1)), vec![0x2a0103e0]);
}

#[test]
fn a_move_involving_the_stack_pointer_is_an_add_of_zero() {
    assert_eq!(assemble(|a| a.mov(SP, X0)), vec![0x9100001f]);
    assert_eq!(assemble(|a| a.add(X0, SP, 16)), vec![0x910043e0]);
    assert_eq!(assemble(|a| a.add(SP, SP, 32)), vec![0x910083ff]);
}

#[test]
fn move_wide_picks_the_shift_from_the_immediate() {
    assert_eq!(assemble(|a| a.movz(X0, 0x1234, -1)), vec![0xd2824680]);
    assert_eq!(assemble(|a| a.movz(X0, 0x1234, 16)), vec![0xd2a24680]);
    assert_eq!(assemble(|a| a.movn(X0, 0, -1)), vec![0x92800000]);
    assert_eq!(assemble(|a| a.movk(X0, 0xabcd, 32)), vec![0xf2d579a0]);
}

#[test]
fn a_scaled_immediate_offset_load_and_store() {
    let offset = |base, offset| MemOperand::new(base, offset, AddrMode::Offset);
    assert_eq!(assemble(|a| a.ldr(X0, offset(X1, 0))), vec![0xf9400020]);
    assert_eq!(assemble(|a| a.ldr(X0, offset(X1, 8))), vec![0xf9400420]);
    assert_eq!(assemble(|a| a.ldr(W0, offset(X1, 4))), vec![0xb9400420]);
    assert_eq!(assemble(|a| a.str(X0, offset(X1, 16))), vec![0xf9000820]);
    assert_eq!(assemble(|a| a.ldrb(W0, offset(X1, 1))), vec![0x39400420]);
    assert_eq!(assemble(|a| a.strb(W0, offset(X1, 1))), vec![0x39000420]);
    assert_eq!(assemble(|a| a.ldrh(W0, offset(X1, 2))), vec![0x79400420]);
    assert_eq!(assemble(|a| a.ldrsw(X0, offset(X1, 4))), vec![0xb9800420]);
}

#[test]
fn an_offset_that_does_not_scale_falls_back_to_the_unscaled_form() {
    assert_eq!(
        assemble(|a| a.ldr(X0, MemOperand::new(X1, -8, AddrMode::Offset))),
        vec![0xf85f8020]
    );
}

#[test]
fn a_register_offset_load() {
    assert_eq!(
        assemble(|a| a.ldr(X0, MemOperand::with_shift(X1, X2, Shift::Lsl, 0))),
        vec![0xf8626820]
    );
}

#[test]
fn pre_and_post_index_loads() {
    assert_eq!(
        assemble(|a| a.ldr(X0, MemOperand::new(X1, 8, AddrMode::PreIndex))),
        vec![0xf8408c20]
    );
    assert_eq!(
        assemble(|a| a.ldr(X0, MemOperand::new(X1, 8, AddrMode::PostIndex))),
        vec![0xf8408420]
    );
}

#[test]
fn branches_to_a_register() {
    assert_eq!(assemble(|a| a.br(X0)), vec![0xd61f0000]);
    assert_eq!(assemble(|a| a.blr(X1)), vec![0xd63f0020]);
    assert_eq!(assemble(|a| a.ret(LR)), vec![0xd65f03c0]);
}

#[test]
fn a_breakpoint_and_a_nop() {
    assert_eq!(assemble(|a| a.brk(0)), vec![0xd4200000]);
    assert_eq!(assemble(|a| a.nop()), vec![0xd503201f]);
    assert_eq!(assemble(|a| a.hint(SystemHint::Nop)), vec![0xd503201f]);
}

#[test]
fn two_branches_to_one_forward_label_are_patched_when_it_binds() {
    let mut assembler = Assembler::new();
    let mut fwd = Label::new();
    assembler.b_cond_label(&mut fwd, Condition::Eq);
    assembler.b_label(&mut fwd);
    assembler.nop();
    assembler.bind(&mut fwd);
    assembler.nop();
    assert_eq!(
        words(&assembler),
        vec![0x54000060, 0x14000002, 0xd503201f, 0xd503201f]
    );
}

#[test]
fn branches_to_a_bound_label_point_backwards() {
    let mut assembler = Assembler::new();
    let mut back = Label::new();
    assembler.nop();
    assembler.bind(&mut back);
    assembler.nop();
    assembler.b_label(&mut back);
    assembler.cbz_label(X0, &mut back);
    assembler.cbnz_label(W1, &mut back);
    assert_eq!(
        words(&assembler),
        vec![
            0xd503201f, 0xd503201f, 0x17ffffff, 0xb4ffffc0, 0x35ffffa1
        ]
    );
}

#[test]
fn a_label_bound_at_the_instruction_after_a_branch_to_it() {
    let mut assembler = Assembler::new();
    let mut back = Label::new();
    assembler.b_cond_label(&mut back, Condition::Ne);
    assembler.bind(&mut back);
    assembler.ret(LR);
    assert_eq!(words(&assembler), vec![0x54000021, 0xd65f03c0]);
}
