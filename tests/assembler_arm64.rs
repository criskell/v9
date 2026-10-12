use vx::codegen::arm64::assembler_arm64::{Assembler, MemOperand, Operand};
use vx::codegen::arm64::constants_arm64::{
    AddrMode, Condition, Shift, SystemHint, C_FLAG, NO_FLAG, N_FLAG, V_FLAG, Z_FLAG,
};
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

#[test]
fn logical_operations_with_a_bitmask_immediate() {
    assert_eq!(assemble(|a| a.and_(X0, X1, 0xffi64)), vec![0x92401c20]);
    assert_eq!(
        assemble(|a| a.and_(X0, X1, -4096i64)),
        vec![0x9274cc20]
    );
    assert_eq!(assemble(|a| a.orr(X0, X1, 1i64)), vec![0xb2400020]);
    assert_eq!(
        assemble(|a| a.eor(W0, W1, 0xf0f0f0f0i64)),
        vec![0x5204cc20]
    );
    assert_eq!(assemble(|a| a.ands(X0, X1, 3i64)), vec![0xf2400420]);
    assert_eq!(assemble(|a| a.bic(X0, X1, 0xffi64)), vec![0x9278dc20]);
    assert_eq!(assemble(|a| a.tst(X0, 1i64)), vec![0xf240001f]);
}

#[test]
fn the_remaining_logical_aliases() {
    assert_eq!(assemble(|a| a.tst(X0, X1)), vec![0xea01001f]);
    assert_eq!(assemble(|a| a.mvn(X0, X1)), vec![0xaa2103e0]);
    assert_eq!(assemble(|a| a.orn(X0, X1, X2)), vec![0xaa220020]);
    assert_eq!(assemble(|a| a.eon(X0, X1, X2)), vec![0xca220020]);
    assert_eq!(assemble(|a| a.negs(X0, X1)), vec![0xeb0103e0]);
}

#[test]
fn add_and_sub_with_carry() {
    assert_eq!(assemble(|a| a.adc(X0, X1, X2)), vec![0x9a020020]);
    assert_eq!(assemble(|a| a.adcs(X0, X1, X2)), vec![0xba020020]);
    assert_eq!(assemble(|a| a.sbc(X0, X1, X2)), vec![0xda020020]);
    assert_eq!(assemble(|a| a.sbcs(W0, W1, W2)), vec![0x7a020020]);
    assert_eq!(assemble(|a| a.ngc(X0, X1)), vec![0xda0103e0]);
    assert_eq!(assemble(|a| a.ngcs(X0, X1)), vec![0xfa0103e0]);
}

#[test]
fn shifts_by_an_immediate_are_bitfield_moves() {
    assert_eq!(assemble(|a| a.lsl(X0, X1, 4)), vec![0xd37cec20]);
    assert_eq!(assemble(|a| a.lsr(X0, X1, 4)), vec![0xd344fc20]);
    assert_eq!(assemble(|a| a.asr(W0, W1, 3)), vec![0x13037c20]);
    assert_eq!(assemble(|a| a.ror(X0, X1, 8)), vec![0x93c12020]);
    assert_eq!(assemble(|a| a.extr(X0, X1, X2, 7)), vec![0x93c21c20]);
}

#[test]
fn bitfield_extracts_and_inserts() {
    assert_eq!(assemble(|a| a.ubfx(X0, X1, 4, 8)), vec![0xd3442c20]);
    assert_eq!(assemble(|a| a.sbfx(X0, X1, 4, 8)), vec![0x93442c20]);
    assert_eq!(assemble(|a| a.ubfiz(X0, X1, 4, 8)), vec![0xd37c1c20]);
    assert_eq!(assemble(|a| a.sbfiz(X0, X1, 4, 8)), vec![0x937c1c20]);
    assert_eq!(assemble(|a| a.bfi(X0, X1, 4, 8)), vec![0xb37c1c20]);
    assert_eq!(assemble(|a| a.bfxil(X0, X1, 4, 8)), vec![0xb3442c20]);
}

#[test]
fn sign_and_zero_extensions() {
    assert_eq!(assemble(|a| a.sxtb(X0, W1)), vec![0x93401c20]);
    assert_eq!(assemble(|a| a.sxth(X0, W1)), vec![0x93403c20]);
    assert_eq!(assemble(|a| a.sxtw(X0, W1)), vec![0x93407c20]);
    assert_eq!(assemble(|a| a.uxtb(W0, W1)), vec![0x53001c20]);
    assert_eq!(assemble(|a| a.uxth(W0, W1)), vec![0x53003c20]);
}

#[test]
fn shifts_by_a_register() {
    assert_eq!(assemble(|a| a.lslv(X0, X1, X2)), vec![0x9ac22020]);
    assert_eq!(assemble(|a| a.lsrv(X0, X1, X2)), vec![0x9ac22420]);
    assert_eq!(assemble(|a| a.asrv(W0, W1, W2)), vec![0x1ac22820]);
    assert_eq!(assemble(|a| a.rorv(X0, X1, X2)), vec![0x9ac22c20]);
}

#[test]
fn division_and_multiplication() {
    assert_eq!(assemble(|a| a.sdiv(X0, X1, X2)), vec![0x9ac20c20]);
    assert_eq!(assemble(|a| a.udiv(W0, W1, W2)), vec![0x1ac20820]);
    assert_eq!(assemble(|a| a.mul(X0, X1, X2)), vec![0x9b027c20]);
    assert_eq!(assemble(|a| a.mneg(X0, X1, X2)), vec![0x9b02fc20]);
    assert_eq!(assemble(|a| a.madd(X0, X1, X2, X3)), vec![0x9b020c20]);
    assert_eq!(assemble(|a| a.msub(X0, X1, X2, X3)), vec![0x9b028c20]);
    assert_eq!(assemble(|a| a.smull(X0, W1, W2)), vec![0x9b227c20]);
    assert_eq!(assemble(|a| a.umull(X0, W1, W2)), vec![0x9ba27c20]);
    assert_eq!(assemble(|a| a.smulh(X0, X1, X2)), vec![0x9b427c20]);
    assert_eq!(assemble(|a| a.umulh(X0, X1, X2)), vec![0x9bc27c20]);
    assert_eq!(assemble(|a| a.smaddl(X0, W1, W2, X3)), vec![0x9b220c20]);
    assert_eq!(assemble(|a| a.umsubl(X0, W1, W2, X3)), vec![0x9ba28c20]);
}

#[test]
fn bit_counting_and_reversal() {
    assert_eq!(assemble(|a| a.rbit(X0, X1)), vec![0xdac00020]);
    assert_eq!(assemble(|a| a.rev(X0, X1)), vec![0xdac00c20]);
    assert_eq!(assemble(|a| a.rev(W0, W1)), vec![0x5ac00820]);
    assert_eq!(assemble(|a| a.rev16(X0, X1)), vec![0xdac00420]);
    assert_eq!(assemble(|a| a.rev32(X0, X1)), vec![0xdac00820]);
    assert_eq!(assemble(|a| a.clz(X0, X1)), vec![0xdac01020]);
    assert_eq!(assemble(|a| a.cls(W0, W1)), vec![0x5ac01420]);
}

#[test]
fn conditional_selects() {
    assert_eq!(
        assemble(|a| a.csel(X0, X1, X2, Condition::Eq)),
        vec![0x9a820020]
    );
    assert_eq!(
        assemble(|a| a.csinc(X0, X1, X2, Condition::Ne)),
        vec![0x9a821420]
    );
    assert_eq!(
        assemble(|a| a.csinv(X0, X1, X2, Condition::Lt)),
        vec![0xda82b020]
    );
    assert_eq!(
        assemble(|a| a.csneg(X0, X1, X2, Condition::Gt)),
        vec![0xda82c420]
    );
}

#[test]
fn conditional_select_aliases_invert_the_condition() {
    assert_eq!(assemble(|a| a.cset(X0, Condition::Eq)), vec![0x9a9f17e0]);
    assert_eq!(assemble(|a| a.csetm(W0, Condition::Ne)), vec![0x5a9f03e0]);
    assert_eq!(
        assemble(|a| a.cinc(X0, X1, Condition::Le)),
        vec![0x9a81c420]
    );
    assert_eq!(
        assemble(|a| a.cinv(X0, X1, Condition::Ge)),
        vec![0xda81b020]
    );
    assert_eq!(
        assemble(|a| a.cneg(X0, X1, Condition::Mi)),
        vec![0xda815420]
    );
}

#[test]
fn conditional_compares() {
    let all = N_FLAG | Z_FLAG | C_FLAG | V_FLAG;
    assert_eq!(
        assemble(|a| a.ccmp(X0, 3i64, all, Condition::Eq)),
        vec![0xfa43080f]
    );
    assert_eq!(
        assemble(|a| a.ccmp(X0, X1, NO_FLAG, Condition::Ne)),
        vec![0xfa411000]
    );
    assert_eq!(
        assemble(|a| a.ccmn(X0, 1i64, Z_FLAG, Condition::Lt)),
        vec![0xba41b804]
    );
    assert_eq!(
        assemble(|a| a.ccmp(W0, W1, C_FLAG, Condition::Ne)),
        vec![0x7a411002]
    );
}

#[test]
fn load_and_store_pair() {
    let offset = |base, offset| MemOperand::new(base, offset, AddrMode::Offset);
    assert_eq!(assemble(|a| a.ldp(X0, X1, offset(X2, 0))), vec![0xa9400440]);
    assert_eq!(
        assemble(|a| a.ldp(X0, X1, offset(X2, 16))),
        vec![0xa9410440]
    );
    assert_eq!(assemble(|a| a.ldp(W0, W1, offset(X2, 8))), vec![0x29410440]);
    assert_eq!(
        assemble(|a| a.ldpsw(X0, X1, offset(X2, 8))),
        vec![0x69410440]
    );
}

#[test]
fn a_frame_prologue_and_epilogue() {
    let mut assembler = Assembler::new();
    assembler.stp(FP, LR, MemOperand::new(SP, -16, AddrMode::PreIndex));
    assembler.mov(FP, SP);
    assembler.ldp(X0, X1, MemOperand::new(SP, 16, AddrMode::PostIndex));
    assembler.ret(LR);
    assert_eq!(
        words(&assembler),
        vec![0xa9bf7bfd, 0x910003fd, 0xa8c107e0, 0xd65f03c0]
    );
}

#[test]
fn a_test_and_branch_on_one_bit() {
    let mut assembler = Assembler::new();
    let mut fwd = Label::new();
    assembler.tbz_label(X0, 3, &mut fwd);
    assembler.tbnz_label(W1, 31, &mut fwd);
    assembler.bind(&mut fwd);
    assembler.nop();
    assert_eq!(
        words(&assembler),
        vec![0x36180040, 0x37f80021, 0xd503201f]
    );
}

#[test]
fn an_address_of_a_label() {
    let mut assembler = Assembler::new();
    let mut here = Label::new();
    assembler.adr_label(X0, &mut here);
    assembler.nop();
    assembler.bind(&mut here);
    assert_eq!(words(&assembler), vec![0x10000040, 0xd503201f]);
}
