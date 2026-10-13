#![allow(non_upper_case_globals)]

use vx::codegen::arm64::assembler_arm64::{MemOperand, Operand};
use vx::codegen::arm64::constants_arm64::*;
use vx::codegen::cpu_features::CpuFeature;
use vx::codegen::arm64::instructions_arm64::Instruction;
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
    assert_eq!(assemble(|m| m.mov_immediate(x0, 1234)), vec![0xd2809a40]);
    assert_eq!(
        assemble(|m| m.mov_immediate(x0, u64::MAX)),
        vec![0x92800000]
    );
    assert_eq!(
        assemble(|m| m.mov_immediate(x0, 0x5555555555555555)),
        vec![0xb200f3e0]
    );
}

#[test]
fn a_register_move_and_a_shifted_move() {
    assert_eq!(assemble(|m| m.mov(x0, x1)), vec![0xaa0103e0]);
    assert_eq!(assemble(|m| m.mov(w0, w1)), vec![0x2a0103e0]);
    assert_eq!(
        assemble(|m| m.mov(x0, Operand::with_shift(x1, Shift::Lsl, 4))),
        vec![0xd37cec20]
    );
}

#[test]
fn an_add_of_a_shifted_immediate_stays_one_instruction() {
    assert_eq!(assemble(|m| m.add(x0, x1, 4096i64)), vec![0x91400420]);
    assert_eq!(assemble(|m| m.sub(sp, sp, 32i64)), vec![0xd10083ff]);
    assert_eq!(assemble(|m| m.add(sp, sp, 32i64)), vec![0x910083ff]);
}

#[test]
fn an_add_of_zero_to_itself_is_elided() {
    assert_eq!(assemble(|m| m.add(x0, x0, 0i64)), Vec::<u32>::new());
    assert_eq!(assemble(|m| m.claim(0, 8)), Vec::<u32>::new());
    assert_eq!(assemble(|m| m.drop(0, 8)), Vec::<u32>::new());
}

#[test]
fn a_push_and_a_pop_of_one_register() {
    assert_eq!(assemble(|m| m.push(&[x0])), vec![0xf81f8fe0]);
    assert_eq!(assemble(|m| m.pop(&[x0])), vec![0xf84087e0]);
}

#[test]
fn a_push_and_a_pop_of_a_pair() {
    assert_eq!(assemble(|m| m.push(&[x0, x1])), vec![0xa9bf03e1]);
    assert_eq!(assemble(|m| m.pop(&[x0, x1])), vec![0xa8c107e0]);
}

#[test]
fn a_wide_immediate_move_expands_to_several_instructions() {
    let simulator = run(|m| {
        m.mov_immediate(x0, 0x0123456789abcdef);
        m.mov_immediate(x1, 0xffff0000ffff0000);
        m.mov_immediate(x2, 1);
        m.mov_immediate(w3, 0xdeadbeef);
    });
    assert_eq!(simulator.xreg(0), 0x0123456789abcdef);
    assert_eq!(simulator.xreg(1), 0xffff0000ffff0000);
    assert_eq!(simulator.xreg(2), 1);
    assert_eq!(simulator.xreg(3), 0xdeadbeef);
}

#[test]
fn an_inverted_move_is_chosen_when_it_is_shorter() {
    let masm_count = assemble(|m| m.mov_immediate(x0, 0xffffffffffff0000)).len();
    assert_eq!(masm_count, 1);
    let simulator = run(|m| m.mov_immediate(x0, 0xffffffffffff0000));
    assert_eq!(simulator.xreg(0), 0xffffffffffff0000);
}

#[test]
fn an_add_of_an_immediate_too_large_for_the_field_uses_a_scratch() {
    let simulator = run(|m| {
        m.mov_immediate(x1, 1000);
        m.add(x0, x1, 0x123456i64);
        m.sub(x2, x1, 0x123456i64);
    });
    assert_eq!(simulator.xreg(0), 1000 + 0x123456);
    assert_eq!(simulator.xreg(2), (1000i64 - 0x123456) as u64);
}

#[test]
fn a_logical_immediate_that_does_not_encode_uses_a_scratch() {
    let simulator = run(|m| {
        m.mov_immediate(x1, u64::MAX);
        m.and_(x0, x1, 0x12345i64);
        m.orr(x2, xzr, 0x12345i64);
        m.eor(x3, x1, 0x12345i64);
    });
    assert_eq!(simulator.xreg(0), 0x12345);
    assert_eq!(simulator.xreg(2), 0x12345);
    assert_eq!(simulator.xreg(3), !0x12345u64);
}

#[test]
fn the_special_cases_of_a_logical_immediate() {
    let simulator = run(|m| {
        m.mov_immediate(x1, 0xabcd);
        m.and_(x0, x1, 0i64);
        m.orr(x2, x1, 0i64);
        m.and_(x3, x1, -1i64);
        m.eor(x4, x1, -1i64);
    });
    assert_eq!(simulator.xreg(0), 0);
    assert_eq!(simulator.xreg(2), 0xabcd);
    assert_eq!(simulator.xreg(3), 0xabcd);
    assert_eq!(simulator.xreg(4), !0xabcdu64);
}

#[test]
fn a_load_with_an_offset_beyond_the_field_uses_a_scratch() {
    let simulator = run(|m| {
        m.mov_immediate(x0, 0xcafe);
        m.sub(sp, sp, 0x200i64);
        m.mov(x1, sp);
        m.str(x0, MemOperand::new(x1, 300, AddrMode::Offset));
        m.ldr(x2, MemOperand::new(x1, 300, AddrMode::Offset));
        m.add(sp, sp, 0x200i64);
    });
    assert_eq!(simulator.xreg(2), 0xcafe);
}

#[test]
fn claim_and_drop_move_the_stack_pointer() {
    let simulator = run(|m| {
        m.mov(x0, sp);
        m.claim(2, 8);
        m.mov(x1, sp);
        m.drop(2, 8);
        m.mov(x2, sp);
    });
    assert_eq!(simulator.xreg(0) - simulator.xreg(1), 16);
    assert_eq!(simulator.xreg(0), simulator.xreg(2));
}

#[test]
fn a_push_and_pop_round_trip_through_the_stack() {
    let simulator = run(|m| {
        m.mov_immediate(x0, 11);
        m.mov_immediate(x1, 22);
        m.push(&[x0, x1]);
        m.mov_immediate(x0, 0);
        m.mov_immediate(x1, 0);
        m.pop(&[x2, x3]);
    });
    assert_eq!(simulator.xreg(2), 22);
    assert_eq!(simulator.xreg(3), 11);
}

#[test]
fn a_scratch_register_is_returned_when_the_scope_closes() {
    let simulator = run(|m| {
        m.mov_immediate(x1, 1);
        m.add(x0, x1, 0x123456i64);
        m.add(x2, x1, 0x123456i64);
        m.add(x3, x1, 0x123456i64);
    });
    assert_eq!(simulator.xreg(0), 1 + 0x123456);
    assert_eq!(simulator.xreg(2), 1 + 0x123456);
    assert_eq!(simulator.xreg(3), 1 + 0x123456);
}

#[test]
fn an_extended_register_operand_is_expanded() {
    let simulator = run(|m| {
        m.mov_immediate(x1, 0xff);
        m.mov_immediate(x2, 0xabcd);
        m.and_(
            x0,
            x2,
            Operand::with_extend(w1, vx::codegen::arm64::constants_arm64::Extend::Uxtb, 0),
        );
    });
    assert_eq!(simulator.xreg(0), 0xcd);
}

#[test]
fn a_comparison_through_the_macro_assembler() {
    let simulator = run(|m| {
        m.mov_immediate(x0, 0x123456);
        m.cmp(x0, 0x123456i64);
        m.asm().cset(x1, vx::codegen::arm64::constants_arm64::Condition::Eq);
        m.tst(x0, 0x123456i64);
        m.asm().cset(x2, vx::codegen::arm64::constants_arm64::Condition::Ne);
    });
    assert_eq!(simulator.xreg(1), 1);
    assert_eq!(simulator.xreg(2), 1);
}

#[test]
fn a_test_branch_to_a_far_forward_label_is_sent_through_a_veneer() {
    let mut masm = MacroAssembler::new();
    let far = masm.asm().new_label();
    masm.tbz(x0, 3, far);
    let branch = 0;
    while masm.pc_offset() < 40 * 1024 {
        masm.asm().nop();
    }
    masm.asm().bind(far);

    let code = words(&masm);
    let tbz = Instruction::new(code[branch as usize]);
    assert!(tbz.is_test_branch());
    let veneer = branch + tbz.imm_pc_offset() as i32;
    assert!(veneer < masm.asm().label(far).pos());

    let veneer_instruction = Instruction::new(code[(veneer / 4) as usize]);
    assert!(veneer_instruction.is_uncond_branch_imm());
    assert_eq!(
        veneer + veneer_instruction.imm_pc_offset() as i32,
        masm.asm().label(far).pos()
    );
}

#[test]
fn a_far_test_branch_still_reaches_its_label_when_simulated() {
    let simulator = run(|m| {
        let far = m.asm().new_label();
        let end = m.asm().new_label();
        m.mov_immediate(x0, 0);
        m.mov_immediate(x1, 7);
        m.tbz(x1, 3, far);
        m.mov_immediate(x0, 1);
        m.asm().b(end);
        while m.pc_offset() < 40 * 1024 {
            m.asm().nop();
        }
        m.asm().bind(far);
        m.mov_immediate(x0, 42);
        m.asm().bind(end);
    });
    assert_eq!(simulator.xreg(0), 42);
}

#[test]
fn a_conditional_branch_to_a_bound_label_out_of_range_gets_an_inverted_branch() {
    let mut masm = MacroAssembler::new();
    let back = masm.asm().new_label();
    masm.asm().bind(back);
    while masm.pc_offset() < 2 * 1024 * 1024 {
        masm.asm().nop();
    }
    let at = masm.pc_offset();
    masm.b_cond(back, Condition::Eq);

    let code = words(&masm);
    let inverted = Instruction::new(code[(at / 4) as usize]);
    assert!(inverted.is_cond_branch_imm());
    assert_eq!(
        condition_from_bits(inverted.condition_branch()),
        Condition::Ne
    );
    let over = Instruction::new(code[(at / 4 + 1) as usize]);
    assert!(over.is_uncond_branch_imm());
    assert_eq!(at + 4 + over.imm_pc_offset() as i32, 0);
}

fn one(build: impl FnOnce(&mut MacroAssembler)) -> u32 {
    let mut masm = MacroAssembler::new();
    masm.asm().enable_cpu_feature(CpuFeature::CSSC);
    build(&mut masm);
    let bytes = masm.buffer_start();
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

#[test]
fn two_register_and_conditional_macros() {
    assert_eq!(one(|m| m.cls(x0, x1)), 0xdac01420);
    assert_eq!(one(|m| m.clz(w2, w3)), 0x5ac01062);
    assert_eq!(one(|m| m.rbit(x4, x5)), 0xdac000a4);
    assert_eq!(one(|m| m.rev(x6, x7)), 0xdac00ce6);
    assert_eq!(one(|m| m.rev16(w8, w9)), 0x5ac00528);
    assert_eq!(one(|m| m.sxtb(x10, w11)), 0x93401d6a);
    assert_eq!(one(|m| m.uxth(w12, w13)), 0x53003dac);
    assert_eq!(one(|m| m.cinc(x14, x15, Condition::Eq)), 0x9a8f15ee);
    assert_eq!(one(|m| m.cinv(w16, w17, Condition::Ne)), 0x5a910230);
    assert_eq!(one(|m| m.cneg(x18, x19, Condition::Lt)), 0xda93a672);
    assert_eq!(one(|m| m.cset(x20, Condition::Gt)), 0x9a9fd7f4);
    assert_eq!(one(|m| m.csetm(w21, Condition::Le)), 0x5a9fc3f5);
    assert_eq!(one(|m| m.csel(x22, x23, x24, Condition::Hs)), 0x9a9822f6);
    assert_eq!(one(|m| m.csinc(w25, w26, w27, Condition::Lo)), 0x1a9b3759);
}

#[test]
fn shift_and_multiply_macros() {
    assert_eq!(one(|m| m.asr(x0, x1, 5)), 0x9345fc20);
    assert_eq!(one(|m| m.lsl(w2, w3, 7)), 0x53196062);
    assert_eq!(one(|m| m.lsr(x4, x5, 9)), 0xd349fca4);
    assert_eq!(one(|m| m.ror(x6, x7, 11)), 0x93c72ce6);
    assert_eq!(one(|m| m.asr_register(x8, x9, x10)), 0x9aca2928);
    assert_eq!(one(|m| m.lsl_register(w11, w12, w13)), 0x1acd218b);
    assert_eq!(one(|m| m.mul(x14, x15, x16)), 0x9b107dee);
    assert_eq!(one(|m| m.mneg(w17, w18, w19)), 0x1b13fe51);
    assert_eq!(one(|m| m.sdiv(x20, x21, x22)), 0x9ad60eb4);
    assert_eq!(one(|m| m.udiv(w23, w24, w25)), 0x1ad90b17);
    assert_eq!(one(|m| m.smulh(x26, x27, x28)), 0x9b5c7f7a);
    assert_eq!(one(|m| m.umulh(x0, x1, x2)), 0x9bc27c20);
    assert_eq!(one(|m| m.madd(x3, x4, x5, x6)), 0x9b051883);
    assert_eq!(one(|m| m.msub(w7, w8, w9, w10)), 0x1b09a907);
    assert_eq!(one(|m| m.smaddl(x11, w12, w13, x14)), 0x9b2d398b);
    assert_eq!(one(|m| m.umsubl(x15, w16, w17, x18)), 0x9bb1ca0f);
}

#[test]
fn bitfield_macros() {
    assert_eq!(one(|m| m.bfi(x19, x20, 3, 8)), 0xb37d1e93);
    assert_eq!(one(|m| m.bfxil(w21, w22, 4, 9)), 0x330432d5);
    assert_eq!(one(|m| m.sbfx(x23, x24, 5, 10)), 0x93453b17);
    assert_eq!(one(|m| m.ubfx(w25, w26, 6, 11)), 0x53064359);
    assert_eq!(one(|m| m.sbfiz(x27, x28, 7, 12)), 0x93792f9b);
    assert_eq!(one(|m| m.ubfiz(w0, w1, 8, 13)), 0x53183020);
}

#[test]
fn floating_point_macros() {
    assert_eq!(one(|m| m.fabs_macro(d0, d1)), 0x1e60c020);
    assert_eq!(one(|m| m.fneg(s2, s3)), 0x1e214062);
    assert_eq!(one(|m| m.fsqrt(d4, d5)), 0x1e61c0a4);
    assert_eq!(one(|m| m.fadd(d6, d7, d8)), 0x1e6828e6);
    assert_eq!(one(|m| m.fsub(s9, s10, s11)), 0x1e2b3949);
    assert_eq!(one(|m| m.fmul(d12, d13, d14)), 0x1e6e09ac);
    assert_eq!(one(|m| m.fdiv(s15, s16, s17)), 0x1e311a0f);
    assert_eq!(one(|m| m.fmax(d18, d19, d20)), 0x1e744a72);
    assert_eq!(one(|m| m.fminnm(s21, s22, s23)), 0x1e377ad5);
    assert_eq!(one(|m| m.fmadd(d24, d25, d26, d27)), 0x1f5a6f38);
    assert_eq!(one(|m| m.fnmsub(s28, s29, s30, s31)), 0x1f3effbc);
}

#[test]
fn floating_point_conversions_and_compares() {
    assert_eq!(one(|m| m.fcvtas(x0, d1)), 0x9e640020);
    assert_eq!(one(|m| m.fcvtnu(w2, s3)), 0x1e210062);
    assert_eq!(one(|m| m.fcvtzs(x4, d5, 0)), 0x9e7800a4);
    assert_eq!(one(|m| m.fcmp(d6, d7)), 0x1e6720c0);
    assert_eq!(one(|m| m.fccmp(d8, d9, NZCVFlag, Condition::Eq)), 0x1e69050f);
    assert_eq!(one(|m| m.fcsel(d10, d11, d12, Condition::Ne)), 0x1e6c1d6a);
    assert_eq!(one(|m| m.scvtf(d13, x14, 0)), 0x9e6201cd);
    assert_eq!(one(|m| m.ucvtf(s15, w16, 0)), 0x1e23020f);
    assert_eq!(one(|m| m.extr(x17, x18, x19, 20)), 0x93d35251);
}

#[test]
fn load_store_macros() {
    assert_eq!(one(|m| m.ldrb(w0, MemOperand::new(x1, 2, AddrMode::Offset))), 0x39400820);
    assert_eq!(one(|m| m.strb(w2, MemOperand::new(x3, 4, AddrMode::Offset))), 0x39001062);
    assert_eq!(one(|m| m.ldrsb(x4, MemOperand::new(x5, 6, AddrMode::Offset))), 0x398018a4);
    assert_eq!(one(|m| m.ldrh(w6, MemOperand::new(x7, 8, AddrMode::Offset))), 0x794010e6);
    assert_eq!(one(|m| m.strh(w8, MemOperand::new(x9, 10, AddrMode::Offset))), 0x79001528);
    assert_eq!(one(|m| m.ldrsh(x10, MemOperand::new(x11, 12, AddrMode::Offset))), 0x7980196a);
    assert_eq!(one(|m| m.ldrsw(x12, MemOperand::new(x13, 16, AddrMode::Offset))), 0xb98011ac);
    assert_eq!(one(|m| m.ldp(x14, x15, MemOperand::new(x16, 32, AddrMode::Offset))), 0xa9423e0e);
    assert_eq!(one(|m| m.stp(w17, w18, MemOperand::new(x19, 8, AddrMode::Offset))), 0x29014a71);
    assert_eq!(one(|m| m.ldpsw(x20, x21, MemOperand::new(x22, 16, AddrMode::Offset))), 0x694256d4);
}

#[test]
fn acquire_release_and_atomic_macros() {
    assert_eq!(one(|m| m.ldarb(w0, x1)), 0x08dffc20);
    assert_eq!(one(|m| m.ldaxr(x2, x3)), 0xc85ffc62);
    assert_eq!(one(|m| m.stlrh(w4, x5)), 0x489ffca4);
    assert_eq!(one(|m| m.stlxr(w6, x7, x8)), 0xc806fd07);
    assert_eq!(one(|m| m.cas(x9, x10, MemOperand::new(x11, 0, AddrMode::Offset))), 0xc8a97d6a);
    assert_eq!(one(|m| m.casalb(w12, w13, MemOperand::new(x14, 0, AddrMode::Offset))), 0x08ecfdcd);
}

#[test]
fn barrier_macros() {
    assert_eq!(one(|m| m.dmb(BarrierDomain::InnerShareable, BarrierType::BarrierAll)), 0xd5033bbf);
    assert_eq!(one(|m| m.dsb(BarrierDomain::NonShareable, BarrierType::BarrierAll)), 0xd503379f);
    assert_eq!(one(|m| m.isb()), 0xd5033fdf);
}
