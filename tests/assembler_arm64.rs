#![allow(non_upper_case_globals)]

use vx::codegen::arm64::assembler_arm64::{Assembler, MemOperand, Operand};
use vx::codegen::arm64::constants_arm64::{
    AddrMode, Condition, Shift, SystemHint, CFlag, NoFlag, NFlag, VFlag, ZFlag,
};
use vx::codegen::arm64::register_arm64::*;

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
    assert_eq!(assemble(|a| a.add(x0, x1, x2)), vec![0x8b020020]);
    assert_eq!(assemble(|a| a.add(w0, w1, w2)), vec![0x0b020020]);
    assert_eq!(assemble(|a| a.sub(x5, x6, x7)), vec![0xcb0700c5]);
    assert_eq!(assemble(|a| a.subs(w8, w9, w10)), vec![0x6b0a0128]);
    assert_eq!(
        assemble(|a| a.add(x0, x1, Operand::with_shift(x2, Shift::Lsl, 3))),
        vec![0x8b020c20]
    );
}

#[test]
fn add_of_an_immediate_uses_the_shifted_form_when_it_has_to() {
    assert_eq!(assemble(|a| a.add(x0, x1, 42)), vec![0x9100a820]);
    assert_eq!(assemble(|a| a.add(x0, x1, 4096)), vec![0x91400420]);
    assert_eq!(assemble(|a| a.adds(x3, x4, 1)), vec![0xb1000483]);
}

#[test]
fn a_comparison_writes_to_the_zero_register() {
    assert_eq!(assemble(|a| a.cmp(x0, 0)), vec![0xf100001f]);
    assert_eq!(assemble(|a| a.cmp(x0, x1)), vec![0xeb01001f]);
    assert_eq!(assemble(|a| a.cmn(x0, 1)), vec![0xb100041f]);
    assert_eq!(assemble(|a| a.neg(x0, x1)), vec![0xcb0103e0]);
}

#[test]
fn logical_operations_of_two_registers() {
    assert_eq!(assemble(|a| a.and_(x0, x1, x2)), vec![0x8a020020]);
    assert_eq!(assemble(|a| a.orr(x0, x1, x2)), vec![0xaa020020]);
    assert_eq!(assemble(|a| a.eor(x0, x1, x2)), vec![0xca020020]);
}

#[test]
fn a_register_move_is_an_or_with_the_zero_register() {
    assert_eq!(assemble(|a| a.mov(x0, x1)), vec![0xaa0103e0]);
    assert_eq!(assemble(|a| a.mov(w0, w1)), vec![0x2a0103e0]);
}

#[test]
fn a_move_involving_the_stack_pointer_is_an_add_of_zero() {
    assert_eq!(assemble(|a| a.mov(sp, x0)), vec![0x9100001f]);
    assert_eq!(assemble(|a| a.add(x0, sp, 16)), vec![0x910043e0]);
    assert_eq!(assemble(|a| a.add(sp, sp, 32)), vec![0x910083ff]);
}

#[test]
fn move_wide_picks_the_shift_from_the_immediate() {
    assert_eq!(assemble(|a| a.movz(x0, 0x1234, -1)), vec![0xd2824680]);
    assert_eq!(assemble(|a| a.movz(x0, 0x1234, 16)), vec![0xd2a24680]);
    assert_eq!(assemble(|a| a.movn(x0, 0, -1)), vec![0x92800000]);
    assert_eq!(assemble(|a| a.movk(x0, 0xabcd, 32)), vec![0xf2d579a0]);
}

#[test]
fn a_scaled_immediate_offset_load_and_store() {
    let offset = |base, offset| MemOperand::new(base, offset, AddrMode::Offset);
    assert_eq!(assemble(|a| a.ldr(x0, offset(x1, 0))), vec![0xf9400020]);
    assert_eq!(assemble(|a| a.ldr(x0, offset(x1, 8))), vec![0xf9400420]);
    assert_eq!(assemble(|a| a.ldr(w0, offset(x1, 4))), vec![0xb9400420]);
    assert_eq!(assemble(|a| a.str(x0, offset(x1, 16))), vec![0xf9000820]);
    assert_eq!(assemble(|a| a.ldrb(w0, offset(x1, 1))), vec![0x39400420]);
    assert_eq!(assemble(|a| a.strb(w0, offset(x1, 1))), vec![0x39000420]);
    assert_eq!(assemble(|a| a.ldrh(w0, offset(x1, 2))), vec![0x79400420]);
    assert_eq!(assemble(|a| a.ldrsw(x0, offset(x1, 4))), vec![0xb9800420]);
}

#[test]
fn an_offset_that_does_not_scale_falls_back_to_the_unscaled_form() {
    assert_eq!(
        assemble(|a| a.ldr(x0, MemOperand::new(x1, -8, AddrMode::Offset))),
        vec![0xf85f8020]
    );
}

#[test]
fn a_register_offset_load() {
    assert_eq!(
        assemble(|a| a.ldr(x0, MemOperand::with_shift(x1, x2, Shift::Lsl, 0))),
        vec![0xf8626820]
    );
}

#[test]
fn pre_and_post_index_loads() {
    assert_eq!(
        assemble(|a| a.ldr(x0, MemOperand::new(x1, 8, AddrMode::PreIndex))),
        vec![0xf8408c20]
    );
    assert_eq!(
        assemble(|a| a.ldr(x0, MemOperand::new(x1, 8, AddrMode::PostIndex))),
        vec![0xf8408420]
    );
}

#[test]
fn branches_to_a_register() {
    assert_eq!(assemble(|a| a.br(x0)), vec![0xd61f0000]);
    assert_eq!(assemble(|a| a.blr(x1)), vec![0xd63f0020]);
    assert_eq!(assemble(|a| a.ret(lr)), vec![0xd65f03c0]);
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
    let fwd = assembler.new_label();
    assembler.b_cond(fwd, Condition::Eq);
    assembler.b(fwd);
    assembler.nop();
    assembler.bind(fwd);
    assembler.nop();
    assert_eq!(
        words(&assembler),
        vec![0x54000060, 0x14000002, 0xd503201f, 0xd503201f]
    );
}

#[test]
fn branches_to_a_bound_label_point_backwards() {
    let mut assembler = Assembler::new();
    let back = assembler.new_label();
    assembler.nop();
    assembler.bind(back);
    assembler.nop();
    assembler.b(back);
    assembler.cbz(x0, back);
    assembler.cbnz(w1, back);
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
    let back = assembler.new_label();
    assembler.b_cond(back, Condition::Ne);
    assembler.bind(back);
    assembler.ret(lr);
    assert_eq!(words(&assembler), vec![0x54000021, 0xd65f03c0]);
}

#[test]
fn logical_operations_with_a_bitmask_immediate() {
    assert_eq!(assemble(|a| a.and_(x0, x1, 0xffi64)), vec![0x92401c20]);
    assert_eq!(
        assemble(|a| a.and_(x0, x1, -4096i64)),
        vec![0x9274cc20]
    );
    assert_eq!(assemble(|a| a.orr(x0, x1, 1i64)), vec![0xb2400020]);
    assert_eq!(
        assemble(|a| a.eor(w0, w1, 0xf0f0f0f0i64)),
        vec![0x5204cc20]
    );
    assert_eq!(assemble(|a| a.ands(x0, x1, 3i64)), vec![0xf2400420]);
    assert_eq!(assemble(|a| a.bic(x0, x1, 0xffi64)), vec![0x9278dc20]);
    assert_eq!(assemble(|a| a.tst(x0, 1i64)), vec![0xf240001f]);
}

#[test]
fn the_remaining_logical_aliases() {
    assert_eq!(assemble(|a| a.tst(x0, x1)), vec![0xea01001f]);
    assert_eq!(assemble(|a| a.mvn(x0, x1)), vec![0xaa2103e0]);
    assert_eq!(assemble(|a| a.orn(x0, x1, x2)), vec![0xaa220020]);
    assert_eq!(assemble(|a| a.eon(x0, x1, x2)), vec![0xca220020]);
    assert_eq!(assemble(|a| a.negs(x0, x1)), vec![0xeb0103e0]);
}

#[test]
fn add_and_sub_with_carry() {
    assert_eq!(assemble(|a| a.adc(x0, x1, x2)), vec![0x9a020020]);
    assert_eq!(assemble(|a| a.adcs(x0, x1, x2)), vec![0xba020020]);
    assert_eq!(assemble(|a| a.sbc(x0, x1, x2)), vec![0xda020020]);
    assert_eq!(assemble(|a| a.sbcs(w0, w1, w2)), vec![0x7a020020]);
    assert_eq!(assemble(|a| a.ngc(x0, x1)), vec![0xda0103e0]);
    assert_eq!(assemble(|a| a.ngcs(x0, x1)), vec![0xfa0103e0]);
}

#[test]
fn shifts_by_an_immediate_are_bitfield_moves() {
    assert_eq!(assemble(|a| a.lsl(x0, x1, 4)), vec![0xd37cec20]);
    assert_eq!(assemble(|a| a.lsr(x0, x1, 4)), vec![0xd344fc20]);
    assert_eq!(assemble(|a| a.asr(w0, w1, 3)), vec![0x13037c20]);
    assert_eq!(assemble(|a| a.ror(x0, x1, 8)), vec![0x93c12020]);
    assert_eq!(assemble(|a| a.extr(x0, x1, x2, 7)), vec![0x93c21c20]);
}

#[test]
fn bitfield_extracts_and_inserts() {
    assert_eq!(assemble(|a| a.ubfx(x0, x1, 4, 8)), vec![0xd3442c20]);
    assert_eq!(assemble(|a| a.sbfx(x0, x1, 4, 8)), vec![0x93442c20]);
    assert_eq!(assemble(|a| a.ubfiz(x0, x1, 4, 8)), vec![0xd37c1c20]);
    assert_eq!(assemble(|a| a.sbfiz(x0, x1, 4, 8)), vec![0x937c1c20]);
    assert_eq!(assemble(|a| a.bfi(x0, x1, 4, 8)), vec![0xb37c1c20]);
    assert_eq!(assemble(|a| a.bfxil(x0, x1, 4, 8)), vec![0xb3442c20]);
}

#[test]
fn sign_and_zero_extensions() {
    assert_eq!(assemble(|a| a.sxtb(x0, w1)), vec![0x93401c20]);
    assert_eq!(assemble(|a| a.sxth(x0, w1)), vec![0x93403c20]);
    assert_eq!(assemble(|a| a.sxtw(x0, w1)), vec![0x93407c20]);
    assert_eq!(assemble(|a| a.uxtb(w0, w1)), vec![0x53001c20]);
    assert_eq!(assemble(|a| a.uxth(w0, w1)), vec![0x53003c20]);
}

#[test]
fn shifts_by_a_register() {
    assert_eq!(assemble(|a| a.lslv(x0, x1, x2)), vec![0x9ac22020]);
    assert_eq!(assemble(|a| a.lsrv(x0, x1, x2)), vec![0x9ac22420]);
    assert_eq!(assemble(|a| a.asrv(w0, w1, w2)), vec![0x1ac22820]);
    assert_eq!(assemble(|a| a.rorv(x0, x1, x2)), vec![0x9ac22c20]);
}

#[test]
fn division_and_multiplication() {
    assert_eq!(assemble(|a| a.sdiv(x0, x1, x2)), vec![0x9ac20c20]);
    assert_eq!(assemble(|a| a.udiv(w0, w1, w2)), vec![0x1ac20820]);
    assert_eq!(assemble(|a| a.mul(x0, x1, x2)), vec![0x9b027c20]);
    assert_eq!(assemble(|a| a.mneg(x0, x1, x2)), vec![0x9b02fc20]);
    assert_eq!(assemble(|a| a.madd(x0, x1, x2, x3)), vec![0x9b020c20]);
    assert_eq!(assemble(|a| a.msub(x0, x1, x2, x3)), vec![0x9b028c20]);
    assert_eq!(assemble(|a| a.smull(x0, w1, w2)), vec![0x9b227c20]);
    assert_eq!(assemble(|a| a.umull(x0, w1, w2)), vec![0x9ba27c20]);
    assert_eq!(assemble(|a| a.smulh(x0, x1, x2)), vec![0x9b427c20]);
    assert_eq!(assemble(|a| a.umulh(x0, x1, x2)), vec![0x9bc27c20]);
    assert_eq!(assemble(|a| a.smaddl(x0, w1, w2, x3)), vec![0x9b220c20]);
    assert_eq!(assemble(|a| a.umsubl(x0, w1, w2, x3)), vec![0x9ba28c20]);
}

#[test]
fn bit_counting_and_reversal() {
    assert_eq!(assemble(|a| a.rbit(x0, x1)), vec![0xdac00020]);
    assert_eq!(assemble(|a| a.rev(x0, x1)), vec![0xdac00c20]);
    assert_eq!(assemble(|a| a.rev(w0, w1)), vec![0x5ac00820]);
    assert_eq!(assemble(|a| a.rev16(x0, x1)), vec![0xdac00420]);
    assert_eq!(assemble(|a| a.rev32(x0, x1)), vec![0xdac00820]);
    assert_eq!(assemble(|a| a.clz(x0, x1)), vec![0xdac01020]);
    assert_eq!(assemble(|a| a.cls(w0, w1)), vec![0x5ac01420]);
}

#[test]
fn conditional_selects() {
    assert_eq!(
        assemble(|a| a.csel(x0, x1, x2, Condition::Eq)),
        vec![0x9a820020]
    );
    assert_eq!(
        assemble(|a| a.csinc(x0, x1, x2, Condition::Ne)),
        vec![0x9a821420]
    );
    assert_eq!(
        assemble(|a| a.csinv(x0, x1, x2, Condition::Lt)),
        vec![0xda82b020]
    );
    assert_eq!(
        assemble(|a| a.csneg(x0, x1, x2, Condition::Gt)),
        vec![0xda82c420]
    );
}

#[test]
fn conditional_select_aliases_invert_the_condition() {
    assert_eq!(assemble(|a| a.cset(x0, Condition::Eq)), vec![0x9a9f17e0]);
    assert_eq!(assemble(|a| a.csetm(w0, Condition::Ne)), vec![0x5a9f03e0]);
    assert_eq!(
        assemble(|a| a.cinc(x0, x1, Condition::Le)),
        vec![0x9a81c420]
    );
    assert_eq!(
        assemble(|a| a.cinv(x0, x1, Condition::Ge)),
        vec![0xda81b020]
    );
    assert_eq!(
        assemble(|a| a.cneg(x0, x1, Condition::Mi)),
        vec![0xda815420]
    );
}

#[test]
fn conditional_compares() {
    let all = NFlag | ZFlag | CFlag | VFlag;
    assert_eq!(
        assemble(|a| a.ccmp(x0, 3i64, all, Condition::Eq)),
        vec![0xfa43080f]
    );
    assert_eq!(
        assemble(|a| a.ccmp(x0, x1, NoFlag, Condition::Ne)),
        vec![0xfa411000]
    );
    assert_eq!(
        assemble(|a| a.ccmn(x0, 1i64, ZFlag, Condition::Lt)),
        vec![0xba41b804]
    );
    assert_eq!(
        assemble(|a| a.ccmp(w0, w1, CFlag, Condition::Ne)),
        vec![0x7a411002]
    );
}

#[test]
fn load_and_store_pair() {
    let offset = |base, offset| MemOperand::new(base, offset, AddrMode::Offset);
    assert_eq!(assemble(|a| a.ldp(x0, x1, offset(x2, 0))), vec![0xa9400440]);
    assert_eq!(
        assemble(|a| a.ldp(x0, x1, offset(x2, 16))),
        vec![0xa9410440]
    );
    assert_eq!(assemble(|a| a.ldp(w0, w1, offset(x2, 8))), vec![0x29410440]);
    assert_eq!(
        assemble(|a| a.ldpsw(x0, x1, offset(x2, 8))),
        vec![0x69410440]
    );
}

#[test]
fn a_frame_prologue_and_epilogue() {
    let mut assembler = Assembler::new();
    assembler.stp(fp, lr, MemOperand::new(sp, -16, AddrMode::PreIndex));
    assembler.mov(fp, sp);
    assembler.ldp(x0, x1, MemOperand::new(sp, 16, AddrMode::PostIndex));
    assembler.ret(lr);
    assert_eq!(
        words(&assembler),
        vec![0xa9bf7bfd, 0x910003fd, 0xa8c107e0, 0xd65f03c0]
    );
}

#[test]
fn a_test_and_branch_on_one_bit() {
    let mut assembler = Assembler::new();
    let fwd = assembler.new_label();
    assembler.tbz(x0, 3, fwd);
    assembler.tbnz(w1, 31, fwd);
    assembler.bind(fwd);
    assembler.nop();
    assert_eq!(
        words(&assembler),
        vec![0x36180040, 0x37f80021, 0xd503201f]
    );
}

#[test]
fn an_address_of_a_label() {
    let mut assembler = Assembler::new();
    let here = assembler.new_label();
    assembler.adr(x0, here);
    assembler.nop();
    assembler.bind(here);
    assert_eq!(words(&assembler), vec![0x10000040, 0xd503201f]);
}

#[test]
fn floating_point_moves() {
    assert_eq!(assemble(|a| a.fmov(d0, d1)), vec![0x1e604020]);
    assert_eq!(assemble(|a| a.fmov(s0, s1)), vec![0x1e204020]);
    assert_eq!(
        assemble(|a| a.fmov_from_register(d0, x1)),
        vec![0x9e670020]
    );
    assert_eq!(assemble(|a| a.fmov_to_register(x0, d1)), vec![0x9e660020]);
    assert_eq!(
        assemble(|a| a.fmov_from_register(s0, w1)),
        vec![0x1e270020]
    );
    assert_eq!(assemble(|a| a.fmov_to_register(w0, s1)), vec![0x1e260020]);
}

#[test]
fn a_floating_point_immediate_is_an_eight_bit_pattern() {
    assert_eq!(assemble(|a| a.fmov_immediate(d0, 1.0)), vec![0x1e6e1000]);
    assert_eq!(assemble(|a| a.fmov_immediate(s0, -2.0)), vec![0x1e301000]);
    assert_eq!(assemble(|a| a.fmov_immediate(d0, 0.5)), vec![0x1e6c1000]);
    assert!(Assembler::is_imm_fp64(1.0f64.to_bits()));
    assert!(!Assembler::is_imm_fp64(0.1f64.to_bits()));
}

#[test]
fn floating_point_comparisons() {
    assert_eq!(assemble(|a| a.fcmp(d0, d1)), vec![0x1e612000]);
    assert_eq!(assemble(|a| a.fcmp(s0, s1)), vec![0x1e212000]);
    assert_eq!(assemble(|a| a.fcmp_zero(d0, 0.0)), vec![0x1e602008]);
}

#[test]
fn floating_point_arithmetic() {
    assert_eq!(assemble(|a| a.fadd(d0, d1, d2)), vec![0x1e622820]);
    assert_eq!(assemble(|a| a.fsub(s0, s1, s2)), vec![0x1e223820]);
    assert_eq!(assemble(|a| a.fmul(d0, d1, d2)), vec![0x1e620820]);
    assert_eq!(assemble(|a| a.fdiv(d0, d1, d2)), vec![0x1e621820]);
    assert_eq!(assemble(|a| a.fmax(d0, d1, d2)), vec![0x1e624820]);
    assert_eq!(assemble(|a| a.fmin(d0, d1, d2)), vec![0x1e625820]);
    assert_eq!(assemble(|a| a.fmaxnm(d0, d1, d2)), vec![0x1e626820]);
    assert_eq!(assemble(|a| a.fminnm(d0, d1, d2)), vec![0x1e627820]);
    assert_eq!(assemble(|a| a.fnmul(d0, d1, d2)), vec![0x1e628820]);
}

#[test]
fn floating_point_unary_operations() {
    assert_eq!(assemble(|a| a.fabs(d0, d1)), vec![0x1e60c020]);
    assert_eq!(assemble(|a| a.fneg(s0, s1)), vec![0x1e214020]);
    assert_eq!(assemble(|a| a.fsqrt(d0, d1)), vec![0x1e61c020]);
}

#[test]
fn the_rounding_modes() {
    assert_eq!(assemble(|a| a.frintn(d0, d1)), vec![0x1e644020]);
    assert_eq!(assemble(|a| a.frintp(d0, d1)), vec![0x1e64c020]);
    assert_eq!(assemble(|a| a.frintm(d0, d1)), vec![0x1e654020]);
    assert_eq!(assemble(|a| a.frintz(d0, d1)), vec![0x1e65c020]);
    assert_eq!(assemble(|a| a.frinta(d0, d1)), vec![0x1e664020]);
    assert_eq!(assemble(|a| a.frintx(d0, d1)), vec![0x1e674020]);
    assert_eq!(assemble(|a| a.frinti(d0, d1)), vec![0x1e67c020]);
}

#[test]
fn conversions_between_the_floating_point_sizes() {
    assert_eq!(assemble(|a| a.fcvt(d0, s1)), vec![0x1e22c020]);
    assert_eq!(assemble(|a| a.fcvt(s0, d1)), vec![0x1e624020]);
}

#[test]
fn fused_multiply_add() {
    assert_eq!(assemble(|a| a.fmadd(d0, d1, d2, d3)), vec![0x1f420c20]);
    assert_eq!(assemble(|a| a.fmsub(d0, d1, d2, d3)), vec![0x1f428c20]);
    assert_eq!(assemble(|a| a.fnmadd(s0, s1, s2, s3)), vec![0x1f220c20]);
    assert_eq!(assemble(|a| a.fnmsub(d0, d1, d2, d3)), vec![0x1f628c20]);
}

#[test]
fn conversions_between_integer_and_floating_point() {
    assert_eq!(assemble(|a| a.scvtf(d0, x1, 0)), vec![0x9e620020]);
    assert_eq!(assemble(|a| a.scvtf(s0, w1, 0)), vec![0x1e220020]);
    assert_eq!(assemble(|a| a.ucvtf(d0, x1, 0)), vec![0x9e630020]);
    assert_eq!(assemble(|a| a.scvtf(d0, x1, 4)), vec![0x9e42f020]);
    assert_eq!(assemble(|a| a.fcvtzs(x0, d1, 0)), vec![0x9e780020]);
    assert_eq!(assemble(|a| a.fcvtzs(w0, s1, 0)), vec![0x1e380020]);
    assert_eq!(assemble(|a| a.fcvtzu(x0, d1, 0)), vec![0x9e790020]);
    assert_eq!(assemble(|a| a.fcvtzs(x0, d1, 8)), vec![0x9e58e020]);
}

#[test]
fn loads_and_stores_of_vector_registers() {
    let offset = |base, offset| MemOperand::new(base, offset, AddrMode::Offset);
    assert_eq!(assemble(|a| a.ldr(d0, offset(x1, 8))), vec![0xfd400420]);
    assert_eq!(assemble(|a| a.str(d0, offset(x1, 8))), vec![0xfd000420]);
    assert_eq!(assemble(|a| a.ldr(s0, offset(x1, 4))), vec![0xbd400420]);
    assert_eq!(assemble(|a| a.ldr(q0, offset(x1, 16))), vec![0x3dc00420]);
    assert_eq!(
        assemble(|a| a.ldp(d0, d1, offset(x2, 16))),
        vec![0x6d410440]
    );
    assert_eq!(
        assemble(|a| a.stp(d8, d9, MemOperand::new(sp, -16, AddrMode::PreIndex))),
        vec![0x6dbf27e8]
    );
}

#[test]
fn a_literal_load_is_patched_when_the_pool_is_emitted() {
    let mut assembler = Assembler::new();
    assembler.ldr_literal(x0, 0x0123456789abcdef);
    assembler.ret(lr);
    assembler.force_constant_pool_emission();
    assert_eq!(
        words(&assembler),
        vec![
            0x580000c0, 0xd65f03c0, 0x14000006, 0x5800009f, 0xd63f03e0, 0xd503201f,
            0x89abcdef, 0x01234567,
        ]
    );
    assert!(assembler.constant_pool_is_empty());
}

#[test]
fn two_loads_of_the_same_constant_share_one_pool_entry() {
    let mut assembler = Assembler::new();
    assembler.ldr_literal(x0, 7);
    assembler.ldr_literal(x1, 7);
    assembler.force_constant_pool_emission();
    let words = words(&assembler);
    assert_eq!(words.len(), 8);
    assert_eq!(words[0] >> 5 & 0x7ffff, 6);
    assert_eq!(words[1] >> 5 & 0x7ffff, 5);
    assert_eq!(words[6], 7);
    assert_eq!(words[7], 0);
}

#[test]
fn a_thirty_two_bit_entry_needs_no_alignment_padding() {
    let mut assembler = Assembler::new();
    assembler.ldr_literal(w0, 0xdeadbeef);
    assembler.force_constant_pool_emission();
    assert_eq!(
        words(&assembler),
        vec![0x18000080, 0x14000004, 0x5800005f, 0xd63f03e0, 0xdeadbeef]
    );
}

#[test]
fn a_literal_load_of_a_double() {
    let mut assembler = Assembler::new();
    assembler.ldr_literal(d0, 1.5f64.to_bits());
    assembler.force_constant_pool_emission();
    let words = words(&assembler);
    assert_eq!(words[0] & 0xff000000, 0x5c000000);
    assert_eq!(
        ((words[words.len() - 1] as u64) << 32) | words[words.len() - 2] as u64,
        1.5f64.to_bits()
    );
}
