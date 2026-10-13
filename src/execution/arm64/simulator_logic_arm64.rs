#![allow(non_upper_case_globals)]

use crate::codegen::arm64::constants_arm64::*;
use crate::codegen::arm64::instructions_arm64::rotate_right;
use crate::codegen::arm64::register_arm64::{
    lane_count_from_format, lane_size_in_bits_from_format, lane_size_in_bytes_from_format,
    max_int_from_format, max_uint_from_format, min_int_from_format,
    scalar_format_from_format, scalar_format_from_lane_size,
    vector_format_double_width, vector_format_fill_q, vector_format_half_lanes,
    vector_format_half_width, vector_format_half_width_double_lanes, VectorFormat,
};
use crate::codegen::arm64::utils_arm64::{
    count_leading_sign_bits, count_leading_zeros, double_exp, double_mantissa, double_pack,
    double_sign, float_exp, float_mantissa, float_pack, float_sign,
};
use crate::execution::arm64::simulator_arm64::{LogicVRegister, Simulator};

macro_rules! define_by_element {
    ($($name:ident, $by_element:ident, $indexform:expr;)*) => {
        impl Simulator {
            $(
                pub fn $by_element(
                    &mut self,
                    vform: VectorFormat,
                    dst: &mut LogicVRegister,
                    src1: &LogicVRegister,
                    src2: &LogicVRegister,
                    index: i32,
                ) {
                    let mut temp = LogicVRegister::default();
                    let indexform = ($indexform)(vform);
                    self.dup_element(indexform, &mut temp, src2, index);
                    let temp = temp;
                    self.$name(vform, dst, src1, &temp);
                }
            )*
        }
    };
}

define_by_element! {
    mul, mul_by_element, vector_format_fill_q;
    mla, mla_by_element, vector_format_fill_q;
    mls, mls_by_element, vector_format_fill_q;
    sqdmulh, sqdmulh_by_element, vector_format_fill_q;
    sqrdmulh, sqrdmulh_by_element, vector_format_fill_q;
    smull, smull_by_element, half_width_double_lanes;
    smull2, smull2_by_element, half_width_double_lanes;
    umull, umull_by_element, half_width_double_lanes;
    umull2, umull2_by_element, half_width_double_lanes;
    smlal, smlal_by_element, half_width_double_lanes;
    smlal2, smlal2_by_element, half_width_double_lanes;
    umlal, umlal_by_element, half_width_double_lanes;
    umlal2, umlal2_by_element, half_width_double_lanes;
    smlsl, smlsl_by_element, half_width_double_lanes;
    smlsl2, smlsl2_by_element, half_width_double_lanes;
    umlsl, umlsl_by_element, half_width_double_lanes;
    umlsl2, umlsl2_by_element, half_width_double_lanes;
    sqdmull, sqdmull_by_element, half_width_double_lanes;
    sqdmull2, sqdmull2_by_element, half_width_double_lanes;
    sqdmlal, sqdmlal_by_element, half_width_double_lanes;
    sqdmlal2, sqdmlal2_by_element, half_width_double_lanes;
    sqdmlsl, sqdmlsl_by_element, half_width_double_lanes;
    sqdmlsl2, sqdmlsl2_by_element, half_width_double_lanes;
}

fn half_width_double_lanes(vform: VectorFormat) -> VectorFormat {
    vector_format_half_width_double_lanes(vector_format_fill_q(vform))
}

impl Simulator {
    pub fn cmp(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
        cond: Condition,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let sa = src1.int(vform, i);
            let sb = src2.int(vform, i);
            let ua = src1.uint(vform, i);
            let ub = src2.uint(vform, i);
            let result = match cond {
                Condition::Eq => ua == ub,
                Condition::Ge => sa >= sb,
                Condition::Gt => sa > sb,
                Condition::Hi => ua > ub,
                Condition::Hs => ua >= ub,
                Condition::Lt => sa < sb,
                Condition::Le => sa <= sb,
                _ => unreachable!(),
            };
            dst.set_uint(
                vform,
                i,
                if result {
                    max_uint_from_format(vform)
                } else {
                    0
                },
            );
        }
    }

    pub fn cmp_immediate(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        imm: i32,
        cond: Condition,
    ) {
        let mut temp = LogicVRegister::default();
        self.dup_immediate(vform, &mut temp, imm as u64);
        let imm_reg = temp;
        self.cmp(vform, dst, src1, &imm_reg, cond);
    }

    pub fn cmptst(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let ua = src1.uint(vform, i);
            let ub = src2.uint(vform, i);
            dst.set_uint(
                vform,
                i,
                if (ua & ub) != 0 {
                    max_uint_from_format(vform)
                } else {
                    0
                },
            );
        }
    }

    pub fn add(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let lane_size = lane_size_in_bits_from_format(vform);
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let ua = src1.uint_left_justified(vform, i);
            let ub = src2.uint_left_justified(vform, i);
            let ur = ua.wrapping_add(ub);
            if ur < ua {
                dst.set_unsigned_sat(i, true);
            }

            let pos_a = (ua >> 63) == 0;
            let pos_b = (ub >> 63) == 0;
            let pos_r = (ur >> 63) == 0;
            if (pos_a == pos_b) && (pos_a != pos_r) {
                dst.set_signed_sat(i, pos_a);
            }

            dst.set_int(vform, i, (ur >> (64 - lane_size)) as i64);
        }
    }

    pub fn addp(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let mut temp1 = LogicVRegister::default();
        let mut temp2 = LogicVRegister::default();
        self.uzp1(vform, &mut temp1, src1, src2);
        self.uzp2(vform, &mut temp2, src1, src2);
        let (temp1, temp2) = (temp1, temp2);
        self.add(vform, dst, &temp1, &temp2);
    }

    pub fn mla(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let mut temp = LogicVRegister::default();
        self.mul(vform, &mut temp, src1, src2);
        let (temp, previous) = (temp, *dst);
        self.add(vform, dst, &previous, &temp);
    }

    pub fn mls(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let mut temp = LogicVRegister::default();
        self.mul(vform, &mut temp, src1, src2);
        let (temp, previous) = (temp, *dst);
        self.sub(vform, dst, &previous, &temp);
    }

    pub fn mul(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            dst.set_uint(
                vform,
                i,
                src1.uint(vform, i).wrapping_mul(src2.uint(vform, i)),
            );
        }
    }

    pub fn polynomial_mult(op1: u8, op2: u8) -> u16 {
        Simulator::polynomial_mult128(op1 as u64, op2 as u64, 8).1 as u16
    }

    pub fn polynomial_mult128(op1: u64, op2: u64, lane_size_in_bits: u32) -> (u64, u64) {
        let mut result = (0u64, 0u64);
        debug_assert!(lane_size_in_bits <= kDRegSizeInBits as u32);
        for i in 0..lane_size_in_bits {
            if ((op2 >> i) & 1) != 0 {
                let mut temp = (0u64, op1);
                if i != 0 {
                    temp.0 = temp.1 >> (64 - i);
                    temp.1 <<= i;
                }
                result.0 ^= temp.0;
                result.1 ^= temp.1;
            }
        }
        result
    }

    pub fn pmul(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            dst.set_uint(
                vform,
                i,
                Simulator::polynomial_mult(src1.uint(vform, i) as u8, src2.uint(vform, i) as u8)
                    as u64,
            );
        }
    }

    pub fn pmull(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let vform_src = vector_format_half_width(vform);
        dst.clear_for_write(vform);
        for i in (0..lane_count_from_format(vform)).rev() {
            let product = Simulator::polynomial_mult128(
                src1.uint(vform_src, i),
                src2.uint(vform_src, i),
                lane_size_in_bits_from_format(vform_src),
            );
            dst.set_uint128(vform, i, product);
        }
    }

    pub fn pmull2(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let vform_src = vector_format_half_width_double_lanes(vform);
        dst.clear_for_write(vform);
        let lane_count = lane_count_from_format(vform);
        for i in 0..lane_count {
            let product = Simulator::polynomial_mult128(
                src1.uint(vform_src, lane_count + i),
                src2.uint(vform_src, lane_count + i),
                lane_size_in_bits_from_format(vform_src),
            );
            dst.set_uint128(vform, i, product);
        }
    }

    pub fn sub(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let lane_size = lane_size_in_bits_from_format(vform);
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let ua = src1.uint_left_justified(vform, i);
            let ub = src2.uint_left_justified(vform, i);
            let ur = ua.wrapping_sub(ub);
            if ub > ua {
                dst.set_unsigned_sat(i, false);
            }

            let pos_a = (ua >> 63) == 0;
            let pos_b = (ub >> 63) == 0;
            let pos_r = (ur >> 63) == 0;
            if (pos_a != pos_b) && (pos_a != pos_r) {
                dst.set_signed_sat(i, pos_a);
            }

            dst.set_int(vform, i, (ur >> (64 - lane_size)) as i64);
        }
    }

    pub fn and_(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            dst.set_uint(vform, i, src1.uint(vform, i) & src2.uint(vform, i));
        }
    }

    pub fn orr(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            dst.set_uint(vform, i, src1.uint(vform, i) | src2.uint(vform, i));
        }
    }

    pub fn orn(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            dst.set_uint(vform, i, src1.uint(vform, i) | !src2.uint(vform, i));
        }
    }

    pub fn eor(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            dst.set_uint(vform, i, src1.uint(vform, i) ^ src2.uint(vform, i));
        }
    }

    pub fn bic(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            dst.set_uint(vform, i, src1.uint(vform, i) & !src2.uint(vform, i));
        }
    }

    pub fn bic_immediate(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        imm: u64,
    ) {
        let mut result = [0u64; 16];
        let lane_count = lane_count_from_format(vform);
        for i in 0..lane_count {
            result[i as usize] = src.uint(vform, i) & !imm;
        }
        dst.set_uint_array(vform, &result);
    }

    pub fn bif(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let operand1 = dst.uint(vform, i);
            let operand2 = !src2.uint(vform, i);
            let operand3 = src1.uint(vform, i);
            let result = operand1 ^ ((operand1 ^ operand3) & operand2);
            dst.set_uint(vform, i, result);
        }
    }

    pub fn bit(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let operand1 = dst.uint(vform, i);
            let operand2 = src2.uint(vform, i);
            let operand3 = src1.uint(vform, i);
            let result = operand1 ^ ((operand1 ^ operand3) & operand2);
            dst.set_uint(vform, i, result);
        }
    }

    pub fn bsl(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let operand1 = src2.uint(vform, i);
            let operand2 = dst.uint(vform, i);
            let operand3 = src1.uint(vform, i);
            let result = operand1 ^ ((operand1 ^ operand3) & operand2);
            dst.set_uint(vform, i, result);
        }
    }

    pub fn trn1(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let mut result = [0u64; 16];
        let lane_count = lane_count_from_format(vform);
        let pairs = lane_count / 2;
        for i in 0..pairs {
            result[(2 * i) as usize] = src1.uint(vform, 2 * i);
            result[((2 * i) + 1) as usize] = src2.uint(vform, 2 * i);
        }
        dst.set_uint_array(vform, &result);
    }

    pub fn trn2(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let mut result = [0u64; 16];
        let lane_count = lane_count_from_format(vform);
        let pairs = lane_count / 2;
        for i in 0..pairs {
            result[(2 * i) as usize] = src1.uint(vform, (2 * i) + 1);
            result[((2 * i) + 1) as usize] = src2.uint(vform, (2 * i) + 1);
        }
        dst.set_uint_array(vform, &result);
    }

    pub fn zip1(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let mut result = [0u64; 16];
        let lane_count = lane_count_from_format(vform);
        let pairs = lane_count / 2;
        for i in 0..pairs {
            result[(2 * i) as usize] = src1.uint(vform, i);
            result[((2 * i) + 1) as usize] = src2.uint(vform, i);
        }
        dst.set_uint_array(vform, &result);
    }

    pub fn zip2(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let mut result = [0u64; 16];
        let lane_count = lane_count_from_format(vform);
        let pairs = lane_count / 2;
        for i in 0..pairs {
            result[(2 * i) as usize] = src1.uint(vform, pairs + i);
            result[((2 * i) + 1) as usize] = src2.uint(vform, pairs + i);
        }
        dst.set_uint_array(vform, &result);
    }

    pub fn uzp1(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let mut result = [0u64; 32];
        let lane_count = lane_count_from_format(vform);
        for i in 0..lane_count {
            result[i as usize] = src1.uint(vform, i);
            result[(lane_count + i) as usize] = src2.uint(vform, i);
        }

        dst.clear_for_write(vform);
        for i in 0..lane_count {
            dst.set_uint(vform, i, result[(2 * i) as usize]);
        }
    }

    pub fn uzp2(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let mut result = [0u64; 32];
        let lane_count = lane_count_from_format(vform);
        for i in 0..lane_count {
            result[i as usize] = src1.uint(vform, i);
            result[(lane_count + i) as usize] = src2.uint(vform, i);
        }

        dst.clear_for_write(vform);
        for i in 0..lane_count {
            dst.set_uint(vform, i, result[((2 * i) + 1) as usize]);
        }
    }

    pub fn dup_element(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        src_index: i32,
    ) {
        let lane_count = lane_count_from_format(vform);
        let value = src.uint(vform, src_index);
        dst.clear_for_write(vform);
        for i in 0..lane_count {
            dst.set_uint(vform, i, value);
        }
    }

    pub fn dup_immediate(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, imm: u64) {
        let lane_count = lane_count_from_format(vform);
        let value = imm & max_uint_from_format(vform);
        dst.clear_for_write(vform);
        for i in 0..lane_count {
            dst.set_uint(vform, i, value);
        }
    }

    pub fn ins_element(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        dst_index: i32,
        src: &LogicVRegister,
        src_index: i32,
    ) {
        let value = src.uint(vform, src_index);
        dst.set_uint(vform, dst_index, value);
    }

    pub fn ins_immediate(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        dst_index: i32,
        imm: u64,
    ) {
        let value = imm & max_uint_from_format(vform);
        dst.set_uint(vform, dst_index, value);
    }
}

impl Simulator {
    fn s_min_max(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
        max: bool,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let src1_val = src1.int(vform, i);
            let src2_val = src2.int(vform, i);
            let dst_val = if max {
                if src1_val > src2_val {
                    src1_val
                } else {
                    src2_val
                }
            } else if src1_val < src2_val {
                src1_val
            } else {
                src2_val
            };
            dst.set_int(vform, i, dst_val);
        }
    }

    pub fn smax(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        self.s_min_max(vform, dst, src1, src2, true);
    }

    pub fn smin(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        self.s_min_max(vform, dst, src1, src2, false);
    }

    fn s_min_max_p(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
        max: bool,
    ) {
        let lanes = lane_count_from_format(vform);
        let mut result = [0i64; kMaxLanesPerVector as usize];
        for j in 0..2 {
            let src = if j == 0 { src1 } else { src2 };
            let mut i = 0;
            while i < lanes {
                let first_val = src.int(vform, i);
                let second_val = src.int(vform, i + 1);
                let dst_val = if max {
                    if first_val > second_val {
                        first_val
                    } else {
                        second_val
                    }
                } else if first_val < second_val {
                    first_val
                } else {
                    second_val
                };
                debug_assert!((i >> 1) + (j * lanes / 2) < kMaxLanesPerVector);
                result[((i >> 1) + (j * lanes / 2)) as usize] = dst_val;
                i += 2;
            }
        }
        dst.set_int_array(vform, &result);
    }

    pub fn smaxp(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        self.s_min_max_p(vform, dst, src1, src2, true);
    }

    pub fn sminp(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        self.s_min_max_p(vform, dst, src1, src2, false);
    }

    pub fn addp_scalar(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        debug_assert_eq!(vform, VectorFormat::FormatD);

        let dst_val = src
            .uint(VectorFormat::Format2D, 0)
            .wrapping_add(src.uint(VectorFormat::Format2D, 1));
        dst.clear_for_write(vform);
        dst.set_uint(vform, 0, dst_val);
    }

    pub fn addv(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        let vform_dst = scalar_format_from_lane_size(lane_size_in_bits_from_format(vform));

        let mut dst_val: i64 = 0;
        for i in 0..lane_count_from_format(vform) {
            dst_val = dst_val.wrapping_add(src.int(vform, i));
        }

        dst.clear_for_write(vform_dst);
        dst.set_int(vform_dst, 0, dst_val);
    }

    pub fn saddlv(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        let vform_dst = scalar_format_from_lane_size(lane_size_in_bits_from_format(vform) * 2);

        let mut dst_val: i64 = 0;
        for i in 0..lane_count_from_format(vform) {
            dst_val = dst_val.wrapping_add(src.int(vform, i));
        }

        dst.clear_for_write(vform_dst);
        dst.set_int(vform_dst, 0, dst_val);
    }

    pub fn uaddlv(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        let vform_dst = scalar_format_from_lane_size(lane_size_in_bits_from_format(vform) * 2);

        let mut dst_val: u64 = 0;
        for i in 0..lane_count_from_format(vform) {
            dst_val = dst_val.wrapping_add(src.uint(vform, i));
        }

        dst.clear_for_write(vform_dst);
        dst.set_uint(vform_dst, 0, dst_val);
    }

    fn s_min_max_v(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        max: bool,
    ) {
        let mut dst_val = if max { i64::MIN } else { i64::MAX };
        for i in 0..lane_count_from_format(vform) {
            let src_val = src.int(vform, i);
            if max {
                dst_val = if src_val > dst_val { src_val } else { dst_val };
            } else {
                dst_val = if src_val < dst_val { src_val } else { dst_val };
            }
        }
        dst.clear_for_write(scalar_format_from_format(vform));
        dst.set_int(vform, 0, dst_val);
    }

    pub fn smaxv(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        self.s_min_max_v(vform, dst, src, true);
    }

    pub fn sminv(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        self.s_min_max_v(vform, dst, src, false);
    }

    fn u_min_max(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
        max: bool,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let src1_val = src1.uint(vform, i);
            let src2_val = src2.uint(vform, i);
            let dst_val = if max {
                if src1_val > src2_val {
                    src1_val
                } else {
                    src2_val
                }
            } else if src1_val < src2_val {
                src1_val
            } else {
                src2_val
            };
            dst.set_uint(vform, i, dst_val);
        }
    }

    pub fn umax(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        self.u_min_max(vform, dst, src1, src2, true);
    }

    pub fn umin(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        self.u_min_max(vform, dst, src1, src2, false);
    }

    fn u_min_max_p(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
        max: bool,
    ) {
        let lanes = lane_count_from_format(vform);
        let mut result = [0u64; kMaxLanesPerVector as usize];
        for j in 0..2 {
            let src = if j == 0 { src1 } else { src2 };
            let mut i = 0;
            while i < lane_count_from_format(vform) {
                let first_val = src.uint(vform, i);
                let second_val = src.uint(vform, i + 1);
                let dst_val = if max {
                    if first_val > second_val {
                        first_val
                    } else {
                        second_val
                    }
                } else if first_val < second_val {
                    first_val
                } else {
                    second_val
                };
                debug_assert!((i >> 1) + (j * lanes / 2) < kMaxLanesPerVector);
                result[((i >> 1) + (j * lanes / 2)) as usize] = dst_val;
                i += 2;
            }
        }
        dst.set_uint_array(vform, &result);
    }

    pub fn umaxp(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        self.u_min_max_p(vform, dst, src1, src2, true);
    }

    pub fn uminp(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        self.u_min_max_p(vform, dst, src1, src2, false);
    }

    fn u_min_max_v(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        max: bool,
    ) {
        let mut dst_val = if max { 0 } else { u64::MAX };
        for i in 0..lane_count_from_format(vform) {
            let src_val = src.uint(vform, i);
            if max {
                dst_val = if src_val > dst_val { src_val } else { dst_val };
            } else {
                dst_val = if src_val < dst_val { src_val } else { dst_val };
            }
        }
        dst.clear_for_write(scalar_format_from_format(vform));
        dst.set_uint(vform, 0, dst_val);
    }

    pub fn umaxv(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        self.u_min_max_v(vform, dst, src, true);
    }

    pub fn uminv(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        self.u_min_max_v(vform, dst, src, false);
    }
}

macro_rules! define_extend_long {
    ($($name:ident, $extend:ident, $op:ident;)*) => {
        impl Simulator {
            $(
                pub fn $name(
                    &mut self,
                    vform: VectorFormat,
                    dst: &mut LogicVRegister,
                    src1: &LogicVRegister,
                    src2: &LogicVRegister,
                ) {
                    let mut temp1 = LogicVRegister::default();
                    let mut temp2 = LogicVRegister::default();
                    self.$extend(vform, &mut temp1, src1);
                    self.$extend(vform, &mut temp2, src2);
                    let (temp1, temp2) = (temp1, temp2);
                    self.$op(vform, dst, &temp1, &temp2);
                }
            )*
        }
    };
}

macro_rules! define_extend_wide {
    ($($name:ident, $extend:ident, $op:ident;)*) => {
        impl Simulator {
            $(
                pub fn $name(
                    &mut self,
                    vform: VectorFormat,
                    dst: &mut LogicVRegister,
                    src1: &LogicVRegister,
                    src2: &LogicVRegister,
                ) {
                    let mut temp = LogicVRegister::default();
                    self.$extend(vform, &mut temp, src2);
                    let temp = temp;
                    self.$op(vform, dst, src1, &temp);
                }
            )*
        }
    };
}

macro_rules! define_abs_diff_long {
    ($($name:ident, $extend:ident, $issigned:expr;)*) => {
        impl Simulator {
            $(
                pub fn $name(
                    &mut self,
                    vform: VectorFormat,
                    dst: &mut LogicVRegister,
                    src1: &LogicVRegister,
                    src2: &LogicVRegister,
                ) {
                    let mut temp1 = LogicVRegister::default();
                    let mut temp2 = LogicVRegister::default();
                    self.$extend(vform, &mut temp1, src1);
                    self.$extend(vform, &mut temp2, src2);
                    let (temp1, temp2) = (temp1, temp2);
                    self.abs_diff(vform, dst, &temp1, &temp2, $issigned);
                }
            )*
        }
    };
}

define_extend_long! {
    uaddl, uxtl, add;
    uaddl2, uxtl2, add;
    saddl, sxtl, add;
    saddl2, sxtl2, add;
    usubl, uxtl, sub;
    usubl2, uxtl2, sub;
    ssubl, sxtl, sub;
    ssubl2, sxtl2, sub;
    uabal, uxtl, uaba;
    uabal2, uxtl2, uaba;
    sabal, sxtl, saba;
    sabal2, sxtl2, saba;
    umull, uxtl, mul;
    umull2, uxtl2, mul;
    smull, sxtl, mul;
    smull2, sxtl2, mul;
    umlsl, uxtl, mls;
    umlsl2, uxtl2, mls;
    smlsl, sxtl, mls;
    smlsl2, sxtl2, mls;
    umlal, uxtl, mla;
    umlal2, uxtl2, mla;
    smlal, sxtl, mla;
    smlal2, sxtl2, mla;
}

define_extend_wide! {
    uaddw, uxtl, add;
    uaddw2, uxtl2, add;
    saddw, sxtl, add;
    saddw2, sxtl2, add;
    usubw, uxtl, sub;
    usubw2, uxtl2, sub;
    ssubw, sxtl, sub;
    ssubw2, sxtl2, sub;
}

define_abs_diff_long! {
    uabdl, uxtl, false;
    uabdl2, uxtl2, false;
    sabdl, sxtl, true;
    sabdl2, sxtl2, true;
}

impl Simulator {
    pub fn shl(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        debug_assert!(shift >= 0);
        let mut temp = LogicVRegister::default();
        self.dup_immediate(vform, &mut temp, shift as u64);
        let shiftreg = temp;
        self.ushl(vform, dst, src, &shiftreg);
    }

    pub fn sshll(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        debug_assert!(shift >= 0);
        let mut temp1 = LogicVRegister::default();
        let mut temp2 = LogicVRegister::default();
        self.dup_immediate(vform, &mut temp1, shift as u64);
        self.sxtl(vform, &mut temp2, src);
        let (shiftreg, extendedreg) = (temp1, temp2);
        self.sshl(vform, dst, &extendedreg, &shiftreg);
    }

    pub fn sshll2(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        debug_assert!(shift >= 0);
        let mut temp1 = LogicVRegister::default();
        let mut temp2 = LogicVRegister::default();
        self.dup_immediate(vform, &mut temp1, shift as u64);
        self.sxtl2(vform, &mut temp2, src);
        let (shiftreg, extendedreg) = (temp1, temp2);
        self.sshl(vform, dst, &extendedreg, &shiftreg);
    }

    pub fn shll(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        let shift = lane_size_in_bits_from_format(vform) / 2;
        self.sshll(vform, dst, src, shift as i32);
    }

    pub fn shll2(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        let shift = lane_size_in_bits_from_format(vform) / 2;
        self.sshll2(vform, dst, src, shift as i32);
    }

    pub fn ushll(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        debug_assert!(shift >= 0);
        let mut temp1 = LogicVRegister::default();
        let mut temp2 = LogicVRegister::default();
        self.dup_immediate(vform, &mut temp1, shift as u64);
        self.uxtl(vform, &mut temp2, src);
        let (shiftreg, extendedreg) = (temp1, temp2);
        self.ushl(vform, dst, &extendedreg, &shiftreg);
    }

    pub fn ushll2(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        debug_assert!(shift >= 0);
        let mut temp1 = LogicVRegister::default();
        let mut temp2 = LogicVRegister::default();
        self.dup_immediate(vform, &mut temp1, shift as u64);
        self.uxtl2(vform, &mut temp2, src);
        let (shiftreg, extendedreg) = (temp1, temp2);
        self.ushl(vform, dst, &extendedreg, &shiftreg);
    }

    pub fn sli(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        dst.clear_for_write(vform);
        let lane_count = lane_count_from_format(vform);
        for i in 0..lane_count {
            let src_lane = src.uint(vform, i);
            let dst_lane = dst.uint(vform, i);
            let shifted = src_lane << shift;
            let mask = max_uint_from_format(vform) << shift;
            dst.set_uint(vform, i, (dst_lane & !mask) | shifted);
        }
    }

    pub fn sqshl(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        debug_assert!(shift >= 0);
        let mut temp = LogicVRegister::default();
        self.dup_immediate(vform, &mut temp, shift as u64);
        let shiftreg = temp;
        self.sshl(vform, dst, src, &shiftreg);
        dst.signed_saturate(vform);
    }

    pub fn uqshl(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        debug_assert!(shift >= 0);
        let mut temp = LogicVRegister::default();
        self.dup_immediate(vform, &mut temp, shift as u64);
        let shiftreg = temp;
        self.ushl(vform, dst, src, &shiftreg);
        dst.unsigned_saturate(vform);
    }

    pub fn sqshlu(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        debug_assert!(shift >= 0);
        let mut temp = LogicVRegister::default();
        self.dup_immediate(vform, &mut temp, shift as u64);
        let shiftreg = temp;
        self.sshl(vform, dst, src, &shiftreg);
        dst.unsigned_saturate(vform);
    }

    pub fn sri(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        dst.clear_for_write(vform);
        let lane_count = lane_count_from_format(vform);
        debug_assert!((shift > 0) && (shift <= lane_size_in_bits_from_format(vform) as i32));
        for i in 0..lane_count {
            let src_lane = src.uint(vform, i);
            let dst_lane = dst.uint(vform, i);
            let shifted;
            let mask;
            if shift == 64 {
                shifted = 0;
                mask = 0;
            } else {
                shifted = src_lane >> shift;
                mask = max_uint_from_format(vform) >> shift;
            }
            dst.set_uint(vform, i, (dst_lane & !mask) | shifted);
        }
    }

    pub fn ushr(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        debug_assert!(shift >= 0);
        let mut temp = LogicVRegister::default();
        self.dup_immediate(vform, &mut temp, (-shift) as i64 as u64);
        let shiftreg = temp;
        self.ushl(vform, dst, src, &shiftreg);
    }

    pub fn sshr(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        debug_assert!(shift >= 0);
        let mut temp = LogicVRegister::default();
        self.dup_immediate(vform, &mut temp, (-shift) as i64 as u64);
        let shiftreg = temp;
        self.sshl(vform, dst, src, &shiftreg);
    }

    pub fn ssra(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        let mut temp = LogicVRegister::default();
        self.sshr(vform, &mut temp, src, shift);
        let (shifted_reg, previous) = (temp, *dst);
        self.add(vform, dst, &previous, &shifted_reg);
    }

    pub fn usra(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        let mut temp = LogicVRegister::default();
        self.ushr(vform, &mut temp, src, shift);
        let (shifted_reg, previous) = (temp, *dst);
        self.add(vform, dst, &previous, &shifted_reg);
    }

    pub fn srsra(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        let mut temp = LogicVRegister::default();
        self.sshr(vform, &mut temp, src, shift);
        temp.round(vform);
        let (shifted_reg, previous) = (temp, *dst);
        self.add(vform, dst, &previous, &shifted_reg);
    }

    pub fn ursra(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        let mut temp = LogicVRegister::default();
        self.ushr(vform, &mut temp, src, shift);
        temp.round(vform);
        let (shifted_reg, previous) = (temp, *dst);
        self.add(vform, dst, &previous, &shifted_reg);
    }

    pub fn cls(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        let mut result = [0u64; 16];
        let lane_size_in_bits = lane_size_in_bits_from_format(vform);
        let lane_count = lane_count_from_format(vform);
        for i in 0..lane_count {
            result[i as usize] =
                count_leading_sign_bits(src.int(vform, i), lane_size_in_bits as i32) as u64;
        }
        dst.set_uint_array(vform, &result);
    }

    pub fn clz(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        let mut result = [0u64; 16];
        let lane_size_in_bits = lane_size_in_bits_from_format(vform);
        let lane_count = lane_count_from_format(vform);
        for i in 0..lane_count {
            result[i as usize] = count_leading_zeros(src.uint(vform, i), lane_size_in_bits as i32) as u64;
        }
        dst.set_uint_array(vform, &result);
    }

    pub fn cnt(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        let mut result = [0u64; 16];
        let lane_size_in_bits = lane_size_in_bits_from_format(vform);
        let lane_count = lane_count_from_format(vform);
        for i in 0..lane_count {
            let mut value = src.uint(vform, i);
            result[i as usize] = 0;
            for _ in 0..lane_size_in_bits {
                result[i as usize] += value & 1;
                value >>= 1;
            }
        }
        dst.set_uint_array(vform, &result);
    }

    pub fn sshl(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let mut shift_val = src2.int(vform, i) as i8 as i32;
            let lj_src_val = src1.int_left_justified(vform, i);

            if (shift_val > count_leading_sign_bits(lj_src_val, 64)) && (lj_src_val != 0) {
                dst.set_signed_sat(i, lj_src_val >= 0);
            }

            if lj_src_val < 0 {
                dst.set_unsigned_sat(i, false);
            } else if (shift_val > count_leading_zeros(lj_src_val as u64, 64))
                && (lj_src_val != 0)
            {
                dst.set_unsigned_sat(i, true);
            }

            let src_val = src1.int(vform, i);
            let src_is_negative = src_val < 0;
            if shift_val > 63 {
                dst.set_int(vform, i, 0);
            } else if shift_val < -63 {
                dst.set_rounding(i, src_is_negative);
                dst.set_int(vform, i, if src_is_negative { -1 } else { 0 });
            } else {
                let mut usrc_val = src_val as u64;

                if shift_val < 0 {
                    shift_val = -shift_val;

                    if ((usrc_val >> (shift_val - 1)) & 1) == 1 {
                        dst.set_rounding(i, true);
                    }

                    usrc_val >>= shift_val;

                    if src_is_negative {
                        usrc_val |= !0u64 << (64 - shift_val);
                    }
                } else {
                    usrc_val <<= shift_val;
                }
                dst.set_uint(vform, i, usrc_val);
            }
        }
    }

    pub fn ushl(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let shift_val = src2.int(vform, i) as i8 as i32;
            let lj_src_val = src1.uint_left_justified(vform, i);

            if (shift_val > count_leading_zeros(lj_src_val, 64)) && (lj_src_val != 0) {
                dst.set_unsigned_sat(i, true);
            }

            let mut src_val = src1.uint(vform, i);
            if (shift_val > 63) || (shift_val < -64) {
                dst.set_uint(vform, i, 0);
            } else {
                if shift_val < 0 {
                    if ((src_val >> (-shift_val - 1)) & 1) == 1 {
                        dst.set_rounding(i, true);
                    }

                    if shift_val == -64 {
                        src_val = 0;
                    } else {
                        src_val >>= -shift_val;
                    }
                } else {
                    src_val <<= shift_val;
                }
                dst.set_uint(vform, i, src_val);
            }
        }
    }

    pub fn neg(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let sa = src.int(vform, i);
            if sa == min_int_from_format(vform) {
                dst.set_signed_sat(i, true);
            }
            dst.set_int(vform, i, if sa == i64::MIN { sa } else { -sa });
        }
    }

    pub fn suqadd(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let sa = dst.int_left_justified(vform, i);
            let ub = src.uint_left_justified(vform, i);
            let ur = (sa as u64).wrapping_add(ub);

            let sr = ur as i64;
            if sr < sa {
                dst.set_int(vform, i, max_int_from_format(vform));
            } else {
                let value = (dst.int(vform, i) as u64).wrapping_add(src.uint(vform, i));
                dst.set_uint(vform, i, value);
            }
        }
    }

    pub fn usqadd(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let ua = dst.uint_left_justified(vform, i);
            let sb = src.int_left_justified(vform, i);
            let ur = ua.wrapping_add(sb as u64);

            if (sb > 0) && (ur <= ua) {
                dst.set_uint(vform, i, max_uint_from_format(vform));
            } else if (sb < 0) && (ur >= ua) {
                dst.set_uint(vform, i, 0);
            } else {
                let value = dst.uint(vform, i).wrapping_add(src.int(vform, i) as u64);
                dst.set_uint(vform, i, value);
            }
        }
    }

    pub fn abs(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let sa = src.int(vform, i);
            if sa == min_int_from_format(vform) {
                dst.set_signed_sat(i, true);
            }
            if sa < 0 {
                dst.set_int(vform, i, if sa == i64::MIN { sa } else { -sa });
            } else {
                dst.set_int(vform, i, sa);
            }
        }
    }

    fn extract_narrow(
        &mut self,
        dstform: VectorFormat,
        dst: &mut LogicVRegister,
        dst_is_signed: bool,
        src: &LogicVRegister,
        src_is_signed: bool,
    ) {
        let upperhalf;
        let srcform;
        let mut ssrc = [0i64; 8];
        let mut usrc = [0u64; 8];

        match dstform {
            VectorFormat::Format8B => {
                upperhalf = false;
                srcform = VectorFormat::Format8H;
            }
            VectorFormat::Format16B => {
                upperhalf = true;
                srcform = VectorFormat::Format8H;
            }
            VectorFormat::Format4H => {
                upperhalf = false;
                srcform = VectorFormat::Format4S;
            }
            VectorFormat::Format8H => {
                upperhalf = true;
                srcform = VectorFormat::Format4S;
            }
            VectorFormat::Format2S => {
                upperhalf = false;
                srcform = VectorFormat::Format2D;
            }
            VectorFormat::Format4S => {
                upperhalf = true;
                srcform = VectorFormat::Format2D;
            }
            VectorFormat::FormatB => {
                upperhalf = false;
                srcform = VectorFormat::FormatH;
            }
            VectorFormat::FormatH => {
                upperhalf = false;
                srcform = VectorFormat::FormatS;
            }
            VectorFormat::FormatS => {
                upperhalf = false;
                srcform = VectorFormat::FormatD;
            }
            _ => unreachable!(),
        }

        for i in 0..lane_count_from_format(srcform) {
            ssrc[i as usize] = src.int(srcform, i);
            usrc[i as usize] = src.uint(srcform, i);
        }

        let offset;
        if upperhalf {
            offset = lane_count_from_format(dstform) / 2;
        } else {
            offset = 0;
            dst.clear_for_write(dstform);
        }

        for i in 0..lane_count_from_format(srcform) {
            if ssrc[i as usize] > max_int_from_format(dstform) {
                dst.set_signed_sat(offset + i, true);
            } else if ssrc[i as usize] < min_int_from_format(dstform) {
                dst.set_signed_sat(offset + i, false);
            }

            if src_is_signed {
                if ssrc[i as usize] > max_uint_from_format(dstform) as i64 {
                    dst.set_unsigned_sat(offset + i, true);
                } else if ssrc[i as usize] < 0 {
                    dst.set_unsigned_sat(offset + i, false);
                }
            } else if usrc[i as usize] > max_uint_from_format(dstform) {
                dst.set_unsigned_sat(offset + i, true);
            }

            let result = if src_is_signed {
                ssrc[i as usize] as u64 & max_uint_from_format(dstform)
            } else {
                usrc[i as usize] & max_uint_from_format(dstform)
            };

            if dst_is_signed {
                dst.set_int(dstform, offset + i, result as i64);
            } else {
                dst.set_uint(dstform, offset + i, result);
            }
        }
    }

    pub fn xtn(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        self.extract_narrow(vform, dst, true, src, true);
    }

    pub fn sqxtn(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        self.extract_narrow(vform, dst, true, src, true);
        dst.signed_saturate(vform);
    }

    pub fn sqxtun(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        self.extract_narrow(vform, dst, false, src, true);
        dst.unsigned_saturate(vform);
    }

    pub fn uqxtn(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        self.extract_narrow(vform, dst, false, src, false);
        dst.unsigned_saturate(vform);
    }

    pub fn abs_diff(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
        issigned: bool,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            if issigned {
                let sr = src1.int(vform, i).wrapping_sub(src2.int(vform, i));
                let sr = if sr > 0 { sr } else { sr.wrapping_neg() };
                dst.set_int(vform, i, sr);
            } else {
                let sr = src1.uint(vform, i).wrapping_sub(src2.uint(vform, i)) as i64;
                let sr = if sr > 0 { sr } else { sr.wrapping_neg() };
                dst.set_uint(vform, i, sr as u64);
            }
        }
    }

    pub fn saba(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let mut temp = LogicVRegister::default();
        dst.clear_for_write(vform);
        self.abs_diff(vform, &mut temp, src1, src2, true);
        let (temp, previous) = (temp, *dst);
        self.add(vform, dst, &previous, &temp);
    }

    pub fn uaba(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let mut temp = LogicVRegister::default();
        dst.clear_for_write(vform);
        self.abs_diff(vform, &mut temp, src1, src2, false);
        let (temp, previous) = (temp, *dst);
        self.add(vform, dst, &previous, &temp);
    }

    pub fn not_(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            dst.set_uint(vform, i, !src.uint(vform, i));
        }
    }

    pub fn rbit(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        let mut result = [0u64; 16];
        let lane_count = lane_count_from_format(vform);
        let lane_size_in_bits = lane_size_in_bits_from_format(vform);
        for i in 0..lane_count {
            let mut value = src.uint(vform, i);
            let mut reversed_value = 0;
            for _ in 0..lane_size_in_bits {
                reversed_value = (reversed_value << 1) | (value & 1);
                value >>= 1;
            }
            result[i as usize] = reversed_value;
        }
        dst.set_uint_array(vform, &result);
    }

    fn rev(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        rev_size: i32,
    ) {
        let mut result = [0u64; 16];
        let lane_count = lane_count_from_format(vform);
        let lane_size = lane_size_in_bytes_from_format(vform) as i32;
        let lanes_per_loop = rev_size / lane_size;
        let mut i = 0;
        while i < lane_count {
            for j in 0..lanes_per_loop {
                result[(i + lanes_per_loop - 1 - j) as usize] = src.uint(vform, i + j);
            }
            i += lanes_per_loop;
        }
        dst.set_uint_array(vform, &result);
    }

    pub fn rev16(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        self.rev(vform, dst, src, 2);
    }

    pub fn rev32(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        self.rev(vform, dst, src, 4);
    }

    pub fn rev64(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        self.rev(vform, dst, src, 8);
    }

    fn addlp(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        is_signed: bool,
        do_accumulate: bool,
    ) {
        let vformsrc = vector_format_half_width_double_lanes(vform);
        debug_assert!(lane_size_in_bits_from_format(vformsrc) <= 32);
        debug_assert!(lane_count_from_format(vform) <= 8);

        let mut result = [0u64; 8];
        let lane_count = lane_count_from_format(vform);
        for i in 0..lane_count {
            if is_signed {
                result[i as usize] = src
                    .int(vformsrc, 2 * i)
                    .wrapping_add(src.int(vformsrc, 2 * i + 1)) as u64;
            } else {
                result[i as usize] = src
                    .uint(vformsrc, 2 * i)
                    .wrapping_add(src.uint(vformsrc, 2 * i + 1));
            }
        }

        dst.clear_for_write(vform);
        for i in 0..lane_count {
            if do_accumulate {
                result[i as usize] = result[i as usize].wrapping_add(dst.uint(vform, i));
            }
            dst.set_uint(vform, i, result[i as usize]);
        }
    }

    pub fn saddlp(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        self.addlp(vform, dst, src, true, false);
    }

    pub fn uaddlp(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        self.addlp(vform, dst, src, false, false);
    }

    pub fn sadalp(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        self.addlp(vform, dst, src, true, true);
    }

    pub fn uadalp(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        self.addlp(vform, dst, src, false, true);
    }

    pub fn ror(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        rotation: i32,
    ) {
        dst.clear_for_write(vform);
        let width = lane_size_in_bits_from_format(vform);
        for i in 0..lane_count_from_format(vform) {
            let value = src.uint(vform, i);
            dst.set_uint(vform, i, rotate_right(value, rotation as u32, width));
        }
    }

    pub fn ext(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
        index: i32,
    ) {
        let mut result = [0u8; 16];
        let lane_count = lane_count_from_format(vform);
        for i in 0..lane_count - index {
            result[i as usize] = src1.uint(vform, i + index) as u8;
        }
        for i in 0..index {
            result[(lane_count - index + i) as usize] = src2.uint(vform, i) as u8;
        }
        dst.clear_for_write(vform);
        for i in 0..lane_count {
            dst.set_uint(vform, i, result[i as usize] as u64);
        }
    }

    pub fn movi(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, imm: u64) {
        let lane_count = lane_count_from_format(vform);
        dst.clear_for_write(vform);
        for i in 0..lane_count {
            dst.set_uint(vform, i, imm);
        }
    }

    pub fn mvni(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, imm: u64) {
        let lane_count = lane_count_from_format(vform);
        dst.clear_for_write(vform);
        for i in 0..lane_count {
            dst.set_uint(vform, i, !imm);
        }
    }

    pub fn orr_immediate(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        imm: u64,
    ) {
        let mut result = [0u64; 16];
        let lane_count = lane_count_from_format(vform);
        for i in 0..lane_count {
            result[i as usize] = src.uint(vform, i) | imm;
        }
        dst.set_uint_array(vform, &result);
    }

    pub fn uxtl(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        let vform_half = vector_format_half_width(vform);

        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            dst.set_uint(vform, i, src.uint(vform_half, i));
        }
    }

    pub fn sxtl(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        let vform_half = vector_format_half_width(vform);

        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            dst.set_int(vform, i, src.int(vform_half, i));
        }
    }

    pub fn uxtl2(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        let vform_half = vector_format_half_width(vform);
        let lane_count = lane_count_from_format(vform);

        dst.clear_for_write(vform);
        for i in 0..lane_count {
            dst.set_uint(vform, i, src.uint(vform_half, lane_count + i));
        }
    }

    pub fn sxtl2(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        let vform_half = vector_format_half_width(vform);
        let lane_count = lane_count_from_format(vform);

        dst.clear_for_write(vform);
        for i in 0..lane_count {
            dst.set_int(vform, i, src.int(vform_half, lane_count + i));
        }
    }

    pub fn shrn(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        let mut temp = LogicVRegister::default();
        let vform_src = vector_format_double_width(vform);
        let vform_dst = vform;
        self.ushr(vform_src, &mut temp, src, shift);
        let shifted_src = temp;
        self.extract_narrow(vform_dst, dst, false, &shifted_src, false);
    }

    pub fn shrn2(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        let mut temp = LogicVRegister::default();
        let vformsrc = vector_format_double_width(vector_format_half_lanes(vform));
        let vformdst = vform;
        self.ushr(vformsrc, &mut temp, src, shift);
        let shifted_src = temp;
        self.extract_narrow(vformdst, dst, false, &shifted_src, false);
    }

    pub fn rshrn(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        let mut temp = LogicVRegister::default();
        let vformsrc = vector_format_double_width(vform);
        let vformdst = vform;
        self.ushr(vformsrc, &mut temp, src, shift);
        temp.round(vformsrc);
        let shifted_src = temp;
        self.extract_narrow(vformdst, dst, false, &shifted_src, false);
    }

    pub fn rshrn2(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        let mut temp = LogicVRegister::default();
        let vformsrc = vector_format_double_width(vector_format_half_lanes(vform));
        let vformdst = vform;
        self.ushr(vformsrc, &mut temp, src, shift);
        temp.round(vformsrc);
        let shifted_src = temp;
        self.extract_narrow(vformdst, dst, false, &shifted_src, false);
    }

    fn table(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        ind: &LogicVRegister,
        zero_out_of_bounds: bool,
        tab: &[&LogicVRegister],
    ) {
        debug_assert!(!tab.is_empty());
        let mut result = [0u64; kMaxLanesPerVector as usize];
        for i in 0..lane_count_from_format(vform) {
            result[i as usize] = if zero_out_of_bounds {
                0
            } else {
                dst.uint(VectorFormat::Format16B, i)
            };
        }
        for i in 0..lane_count_from_format(vform) {
            let j = ind.uint(vform, i);
            let tab_idx = (j >> 4) as usize;
            let j_idx = (j & 15) as i32;
            if tab_idx < tab.len() {
                result[i as usize] = tab[tab_idx].uint(VectorFormat::Format16B, j_idx);
            }
        }
        dst.set_uint_array(vform, &result);
    }

    pub fn tbl(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        tab: &[&LogicVRegister],
        ind: &LogicVRegister,
    ) {
        self.table(vform, dst, ind, true, tab);
    }

    pub fn tbx(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        tab: &[&LogicVRegister],
        ind: &LogicVRegister,
    ) {
        self.table(vform, dst, ind, false, tab);
    }

    pub fn uqshrn(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        self.shrn(vform, dst, src, shift);
        dst.unsigned_saturate(vform);
    }

    pub fn uqshrn2(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        self.shrn2(vform, dst, src, shift);
        dst.unsigned_saturate(vform);
    }

    pub fn uqrshrn(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        self.rshrn(vform, dst, src, shift);
        dst.unsigned_saturate(vform);
    }

    pub fn uqrshrn2(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        self.rshrn2(vform, dst, src, shift);
        dst.unsigned_saturate(vform);
    }

    pub fn sqshrn(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        let mut temp = LogicVRegister::default();
        let vformsrc = vector_format_double_width(vform);
        let vformdst = vform;
        self.sshr(vformsrc, &mut temp, src, shift);
        let shifted_src = temp;
        self.sqxtn(vformdst, dst, &shifted_src);
    }

    pub fn sqshrn2(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        let mut temp = LogicVRegister::default();
        let vformsrc = vector_format_double_width(vector_format_half_lanes(vform));
        let vformdst = vform;
        self.sshr(vformsrc, &mut temp, src, shift);
        let shifted_src = temp;
        self.sqxtn(vformdst, dst, &shifted_src);
    }

    pub fn sqrshrn(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        let mut temp = LogicVRegister::default();
        let vformsrc = vector_format_double_width(vform);
        let vformdst = vform;
        self.sshr(vformsrc, &mut temp, src, shift);
        temp.round(vformsrc);
        let shifted_src = temp;
        self.sqxtn(vformdst, dst, &shifted_src);
    }

    pub fn sqrshrn2(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        let mut temp = LogicVRegister::default();
        let vformsrc = vector_format_double_width(vector_format_half_lanes(vform));
        let vformdst = vform;
        self.sshr(vformsrc, &mut temp, src, shift);
        temp.round(vformsrc);
        let shifted_src = temp;
        self.sqxtn(vformdst, dst, &shifted_src);
    }

    pub fn sqshrun(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        let mut temp = LogicVRegister::default();
        let vformsrc = vector_format_double_width(vform);
        let vformdst = vform;
        self.sshr(vformsrc, &mut temp, src, shift);
        let shifted_src = temp;
        self.sqxtun(vformdst, dst, &shifted_src);
    }

    pub fn sqshrun2(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        let mut temp = LogicVRegister::default();
        let vformsrc = vector_format_double_width(vector_format_half_lanes(vform));
        let vformdst = vform;
        self.sshr(vformsrc, &mut temp, src, shift);
        let shifted_src = temp;
        self.sqxtun(vformdst, dst, &shifted_src);
    }

    pub fn sqrshrun(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        let mut temp = LogicVRegister::default();
        let vformsrc = vector_format_double_width(vform);
        let vformdst = vform;
        self.sshr(vformsrc, &mut temp, src, shift);
        temp.round(vformsrc);
        let shifted_src = temp;
        self.sqxtun(vformdst, dst, &shifted_src);
    }

    pub fn sqrshrun2(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        shift: i32,
    ) {
        let mut temp = LogicVRegister::default();
        let vformsrc = vector_format_double_width(vector_format_half_lanes(vform));
        let vformdst = vform;
        self.sshr(vformsrc, &mut temp, src, shift);
        temp.round(vformsrc);
        let shifted_src = temp;
        self.sqxtun(vformdst, dst, &shifted_src);
    }

    pub fn sqdmull(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let mut temp = LogicVRegister::default();
        self.smull(vform, &mut temp, src1, src2);
        let product = temp;
        self.add(vform, dst, &product, &product);
        dst.signed_saturate(vform);
    }

    pub fn sqdmull2(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let mut temp = LogicVRegister::default();
        self.smull2(vform, &mut temp, src1, src2);
        let product = temp;
        self.add(vform, dst, &product, &product);
        dst.signed_saturate(vform);
    }

    fn sqrdmulh_rounded(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
        round: bool,
    ) {
        let esize = lane_size_in_bits_from_format(vform) as i32;
        let round_const: i64 = if round { 1 << (esize - 2) } else { 0 };

        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let mut product = src1.int(vform, i).wrapping_mul(src2.int(vform, i));
            product = product.wrapping_add(round_const);
            product >>= esize - 1;

            if product > max_int_from_format(vform) {
                product = max_int_from_format(vform);
            } else if product < min_int_from_format(vform) {
                product = min_int_from_format(vform);
            }
            dst.set_int(vform, i, product);
        }
    }

    pub fn sqrdmulh(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        self.sqrdmulh_rounded(vform, dst, src1, src2, true);
    }

    pub fn sqdmulh(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        self.sqrdmulh_rounded(vform, dst, src1, src2, false);
    }
}

macro_rules! define_halving_narrow {
    ($($name:ident, $op:ident, $narrow:ident, $halve:expr;)*) => {
        impl Simulator {
            $(
                pub fn $name(
                    &mut self,
                    vform: VectorFormat,
                    dst: &mut LogicVRegister,
                    src1: &LogicVRegister,
                    src2: &LogicVRegister,
                ) {
                    let mut temp = LogicVRegister::default();
                    let wide = if $halve {
                        vector_format_double_width(vector_format_half_lanes(vform))
                    } else {
                        vector_format_double_width(vform)
                    };
                    self.$op(wide, &mut temp, src1, src2);
                    let temp = temp;
                    let shift = lane_size_in_bits_from_format(vform) as i32;
                    self.$narrow(vform, dst, &temp, shift);
                }
            )*
        }
    };
}

define_halving_narrow! {
    addhn, add, shrn, false;
    addhn2, add, shrn2, true;
    raddhn, add, rshrn, false;
    raddhn2, add, rshrn2, true;
    subhn, sub, shrn, false;
    subhn2, sub, shrn2, true;
    rsubhn, sub, rshrn, false;
    rsubhn2, sub, rshrn2, true;
}

macro_rules! define_sqd_accumulate {
    ($($name:ident, $product:ident, $op:ident;)*) => {
        impl Simulator {
            $(
                pub fn $name(
                    &mut self,
                    vform: VectorFormat,
                    dst: &mut LogicVRegister,
                    src1: &LogicVRegister,
                    src2: &LogicVRegister,
                ) {
                    let mut temp = LogicVRegister::default();
                    self.$product(vform, &mut temp, src1, src2);
                    let (temp, previous) = (temp, *dst);
                    self.$op(vform, dst, &previous, &temp);
                    dst.signed_saturate(vform);
                }
            )*
        }
    };
}

define_sqd_accumulate! {
    sqdmlal, sqdmull, add;
    sqdmlal2, sqdmull2, add;
    sqdmlsl, sqdmull, sub;
    sqdmlsl2, sqdmull2, sub;
}

macro_rules! define_fp_scalar_helpers {
    ($float:ty, $fp_add:ident, $fp_sub:ident, $fp_mul:ident, $fp_mulx:ident,
     $fp_mul_add:ident, $fp_div:ident, $fp_sqrt:ident, $fp_max:ident,
     $fp_max_nm:ident, $fp_min:ident, $fp_min_nm:ident,
     $fp_recip_step_fused:ident, $fp_rsqrt_step_fused:ident,
     $fp_process_nan:ident, $fp_process_nans:ident, $fp_process_nans3:ident,
     $fp_default_nan:ident, $is_quiet_nan:ident, $is_signalling_nan:ident,
     $mantissa_msb:expr;) => {
        pub fn $is_quiet_nan(value: $float) -> bool {
            value.is_nan() && ((value.to_bits() >> $mantissa_msb) & 1) != 0
        }

        pub fn $is_signalling_nan(value: $float) -> bool {
            value.is_nan() && ((value.to_bits() >> $mantissa_msb) & 1) == 0
        }

        impl Simulator {
            pub fn $fp_default_nan() -> $float {
                <$float>::NAN
            }

            pub fn $fp_process_nan(op: $float) -> $float {
                debug_assert!(op.is_nan());
                if $is_signalling_nan(op) {
                    Simulator::$fp_default_nan()
                } else {
                    op
                }
            }

            pub fn $fp_process_nans(op1: $float, op2: $float) -> $float {
                if $is_signalling_nan(op1) {
                    Simulator::$fp_process_nan(op1)
                } else if $is_signalling_nan(op2) {
                    Simulator::$fp_process_nan(op2)
                } else if op1.is_nan() {
                    debug_assert!($is_quiet_nan(op1));
                    Simulator::$fp_process_nan(op1)
                } else if op2.is_nan() {
                    debug_assert!($is_quiet_nan(op2));
                    Simulator::$fp_process_nan(op2)
                } else {
                    0.0
                }
            }

            pub fn $fp_process_nans3(a: $float, op1: $float, op2: $float) -> $float {
                if a.is_nan() || op1.is_nan() || op2.is_nan() {
                    if $is_signalling_nan(a) {
                        Simulator::$fp_process_nan(a)
                    } else if $is_signalling_nan(op1) {
                        Simulator::$fp_process_nan(op1)
                    } else if $is_signalling_nan(op2) {
                        Simulator::$fp_process_nan(op2)
                    } else if a.is_nan() {
                        Simulator::$fp_process_nan(a)
                    } else if op1.is_nan() {
                        Simulator::$fp_process_nan(op1)
                    } else {
                        Simulator::$fp_process_nan(op2)
                    }
                } else {
                    0.0
                }
            }

            pub fn $fp_add(op1: $float, op2: $float) -> $float {
                let result = Simulator::$fp_process_nans(op1, op2);
                if result.is_nan() {
                    return result;
                }

                if op1.is_infinite() && op2.is_infinite() && (op1 != op2) {
                    Simulator::$fp_default_nan()
                } else {
                    op1 + op2
                }
            }

            pub fn $fp_sub(op1: $float, op2: $float) -> $float {
                debug_assert!(!op1.is_nan() && !op2.is_nan());

                if op1.is_infinite() && op2.is_infinite() && (op1 == op2) {
                    Simulator::$fp_default_nan()
                } else {
                    op1 - op2
                }
            }

            pub fn $fp_mul(op1: $float, op2: $float) -> $float {
                debug_assert!(!op1.is_nan() && !op2.is_nan());

                if (op1.is_infinite() && (op2 == 0.0)) || (op2.is_infinite() && (op1 == 0.0)) {
                    Simulator::$fp_default_nan()
                } else {
                    op1 * op2
                }
            }

            pub fn $fp_mulx(op1: $float, op2: $float) -> $float {
                if (op1.is_infinite() && (op2 == 0.0)) || (op2.is_infinite() && (op1 == 0.0)) {
                    let two: $float = 2.0;
                    return (1.0 as $float).copysign(op1) * (1.0 as $float).copysign(op2) * two;
                }
                Simulator::$fp_mul(op1, op2)
            }

            pub fn $fp_mul_add(a: $float, op1: $float, op2: $float) -> $float {
                let result = Simulator::$fp_process_nans3(a, op1, op2);

                let sign_a = (1.0 as $float).copysign(a);
                let sign_prod = (1.0 as $float).copysign(op1) * (1.0 as $float).copysign(op2);
                let isinf_prod = op1.is_infinite() || op2.is_infinite();
                let operation_generates_nan = (op1.is_infinite() && (op2 == 0.0))
                    || (op2.is_infinite() && (op1 == 0.0))
                    || (a.is_infinite() && isinf_prod && (sign_a != sign_prod));

                if result.is_nan() {
                    if operation_generates_nan && $is_quiet_nan(a) {
                        return Simulator::$fp_default_nan();
                    }
                    return result;
                }

                if operation_generates_nan {
                    return Simulator::$fp_default_nan();
                }

                if ((op1 == 0.0) || (op2 == 0.0)) && (a == 0.0) {
                    return if (sign_a < 0.0) && (sign_prod < 0.0) {
                        -0.0
                    } else {
                        0.0
                    };
                }

                let result = op1.mul_add(op2, a);
                debug_assert!(!result.is_nan());

                if (a == 0.0) && (result == 0.0) {
                    return (0.0 as $float).copysign(sign_prod);
                }

                result
            }

            pub fn $fp_div(op1: $float, op2: $float) -> $float {
                debug_assert!(!op1.is_nan() && !op2.is_nan());

                if (op1.is_infinite() && op2.is_infinite()) || ((op1 == 0.0) && (op2 == 0.0)) {
                    Simulator::$fp_default_nan()
                } else {
                    if op2 == 0.0 && !op1.is_nan() {
                        let op1_sign = (1.0 as $float).copysign(op1);
                        let op2_sign = (1.0 as $float).copysign(op2);
                        return op1_sign * op2_sign * <$float>::INFINITY;
                    }
                    op1 / op2
                }
            }

            pub fn $fp_sqrt(op: $float) -> $float {
                if op.is_nan() {
                    Simulator::$fp_process_nan(op)
                } else if op < 0.0 {
                    Simulator::$fp_default_nan()
                } else {
                    op.sqrt()
                }
            }

            pub fn $fp_max(a: $float, b: $float) -> $float {
                let result = Simulator::$fp_process_nans(a, b);
                if result.is_nan() {
                    return result;
                }

                if (a == 0.0)
                    && (b == 0.0)
                    && ((1.0 as $float).copysign(a) != (1.0 as $float).copysign(b))
                {
                    0.0
                } else if a > b {
                    a
                } else {
                    b
                }
            }

            pub fn $fp_max_nm(a: $float, b: $float) -> $float {
                let mut a = a;
                let mut b = b;
                if $is_quiet_nan(a) && !$is_quiet_nan(b) {
                    a = <$float>::NEG_INFINITY;
                } else if !$is_quiet_nan(a) && $is_quiet_nan(b) {
                    b = <$float>::NEG_INFINITY;
                }

                let result = Simulator::$fp_process_nans(a, b);
                if result.is_nan() {
                    result
                } else {
                    Simulator::$fp_max(a, b)
                }
            }

            pub fn $fp_min(a: $float, b: $float) -> $float {
                let result = Simulator::$fp_process_nans(a, b);
                if result.is_nan() {
                    return result;
                }

                if (a == 0.0)
                    && (b == 0.0)
                    && ((1.0 as $float).copysign(a) != (1.0 as $float).copysign(b))
                {
                    -0.0
                } else if a < b {
                    a
                } else {
                    b
                }
            }

            pub fn $fp_min_nm(a: $float, b: $float) -> $float {
                let mut a = a;
                let mut b = b;
                if $is_quiet_nan(a) && !$is_quiet_nan(b) {
                    a = <$float>::INFINITY;
                } else if !$is_quiet_nan(a) && $is_quiet_nan(b) {
                    b = <$float>::INFINITY;
                }

                let result = Simulator::$fp_process_nans(a, b);
                if result.is_nan() {
                    result
                } else {
                    Simulator::$fp_min(a, b)
                }
            }

            pub fn $fp_recip_step_fused(op1: $float, op2: $float) -> $float {
                let two: $float = 2.0;
                if (op1.is_infinite() && (op2 == 0.0)) || ((op1 == 0.0) && op2.is_infinite()) {
                    two
                } else if op1.is_infinite() || op2.is_infinite() {
                    if (op1 >= 0.0) == (op2 >= 0.0) {
                        <$float>::INFINITY
                    } else {
                        <$float>::NEG_INFINITY
                    }
                } else {
                    op1.mul_add(op2, two)
                }
            }

            pub fn $fp_rsqrt_step_fused(op1: $float, op2: $float) -> $float {
                let one_point_five: $float = 1.5;
                let two: $float = 2.0;

                if (op1.is_infinite() && (op2 == 0.0)) || ((op1 == 0.0) && op2.is_infinite()) {
                    one_point_five
                } else if op1.is_infinite() || op2.is_infinite() {
                    if (op1 >= 0.0) == (op2 >= 0.0) {
                        <$float>::INFINITY
                    } else {
                        <$float>::NEG_INFINITY
                    }
                } else if (op1 / two).is_normal() {
                    (op1 / two).mul_add(op2, one_point_five)
                } else if (op2 / two).is_normal() {
                    op1.mul_add(op2 / two, one_point_five)
                } else {
                    one_point_five
                }
            }
        }
    };
}

define_fp_scalar_helpers!(f32, fp_add_f32, fp_sub_f32, fp_mul_f32, fp_mulx_f32,
    fp_mul_add_f32, fp_div_f32, fp_sqrt_f32, fp_max_f32, fp_max_nm_f32, fp_min_f32,
    fp_min_nm_f32, fp_recip_step_fused_f32, fp_rsqrt_step_fused_f32, fp_process_nan_f32,
    fp_process_nans_f32, fp_process_nans3_f32, fp_default_nan_f32, is_quiet_nan_f32,
    is_signalling_nan_f32, 22;);

define_fp_scalar_helpers!(f64, fp_add_f64, fp_sub_f64, fp_mul_f64, fp_mulx_f64,
    fp_mul_add_f64, fp_div_f64, fp_sqrt_f64, fp_max_f64, fp_max_nm_f64, fp_min_f64,
    fp_min_nm_f64, fp_recip_step_fused_f64, fp_rsqrt_step_fused_f64, fp_process_nan_f64,
    fp_process_nans_f64, fp_process_nans3_f64, fp_default_nan_f64, is_quiet_nan_f64,
    is_signalling_nan_f64, 51;);

macro_rules! define_neon_fp_vector_op {
    ($($name:ident, $name_f32:ident, $name_f64:ident, $op_f32:ident, $op_f64:ident,
       $procnan:expr;)*) => {
        impl Simulator {
            $(
                fn $name_f32(
                    &mut self,
                    vform: VectorFormat,
                    dst: &mut LogicVRegister,
                    src1: &LogicVRegister,
                    src2: &LogicVRegister,
                ) {
                    dst.clear_for_write(vform);
                    for i in 0..lane_count_from_format(vform) {
                        let op1 = src1.float32(i);
                        let op2 = src2.float32(i);
                        let mut result;
                        if $procnan {
                            result = Simulator::fp_process_nans_f32(op1, op2);
                            if !result.is_nan() {
                                result = Simulator::$op_f32(op1, op2);
                            }
                        } else {
                            result = Simulator::$op_f32(op1, op2);
                        }
                        dst.set_float32(i, result);
                    }
                }

                fn $name_f64(
                    &mut self,
                    vform: VectorFormat,
                    dst: &mut LogicVRegister,
                    src1: &LogicVRegister,
                    src2: &LogicVRegister,
                ) {
                    dst.clear_for_write(vform);
                    for i in 0..lane_count_from_format(vform) {
                        let op1 = src1.float64(i);
                        let op2 = src2.float64(i);
                        let mut result;
                        if $procnan {
                            result = Simulator::fp_process_nans_f64(op1, op2);
                            if !result.is_nan() {
                                result = Simulator::$op_f64(op1, op2);
                            }
                        } else {
                            result = Simulator::$op_f64(op1, op2);
                        }
                        dst.set_float64(i, result);
                    }
                }

                pub fn $name(
                    &mut self,
                    vform: VectorFormat,
                    dst: &mut LogicVRegister,
                    src1: &LogicVRegister,
                    src2: &LogicVRegister,
                ) {
                    if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                        self.$name_f32(vform, dst, src1, src2);
                    } else {
                        debug_assert_eq!(
                            lane_size_in_bytes_from_format(vform),
                            kDRegSize as u32
                        );
                        self.$name_f64(vform, dst, src1, src2);
                    }
                }
            )*
        }
    };
}

define_neon_fp_vector_op! {
    fadd, fadd_f32, fadd_f64, fp_add_f32, fp_add_f64, false;
    fsub, fsub_f32, fsub_f64, fp_sub_f32, fp_sub_f64, true;
    fmul, fmul_f32, fmul_f64, fp_mul_f32, fp_mul_f64, true;
    fmulx, fmulx_f32, fmulx_f64, fp_mulx_f32, fp_mulx_f64, true;
    fdiv, fdiv_f32, fdiv_f64, fp_div_f32, fp_div_f64, true;
    fmax, fmax_f32, fmax_f64, fp_max_f32, fp_max_f64, false;
    fmin, fmin_f32, fmin_f64, fp_min_f32, fp_min_f64, false;
    fmaxnm, fmaxnm_f32, fmaxnm_f64, fp_max_nm_f32, fp_max_nm_f64, false;
    fminnm, fminnm_f32, fminnm_f64, fp_min_nm_f32, fp_min_nm_f64, false;
}

macro_rules! define_neon_fp_pair_op {
    ($($pair:ident, $pair_scalar:ident, $name:ident, $op_f32:ident, $op_f64:ident;)*) => {
        impl Simulator {
            $(
                pub fn $pair(
                    &mut self,
                    vform: VectorFormat,
                    dst: &mut LogicVRegister,
                    src1: &LogicVRegister,
                    src2: &LogicVRegister,
                ) {
                    let mut temp1 = LogicVRegister::default();
                    let mut temp2 = LogicVRegister::default();
                    self.uzp1(vform, &mut temp1, src1, src2);
                    self.uzp2(vform, &mut temp2, src1, src2);
                    let (temp1, temp2) = (temp1, temp2);
                    self.$name(vform, dst, &temp1, &temp2);
                }

                pub fn $pair_scalar(
                    &mut self,
                    vform: VectorFormat,
                    dst: &mut LogicVRegister,
                    src: &LogicVRegister,
                ) {
                    if vform == VectorFormat::FormatS {
                        let result = Simulator::$op_f32(src.float32(0), src.float32(1));
                        dst.set_float32(0, result);
                    } else {
                        debug_assert_eq!(vform, VectorFormat::FormatD);
                        let result = Simulator::$op_f64(src.float64(0), src.float64(1));
                        dst.set_float64(0, result);
                    }
                    dst.clear_for_write(vform);
                }
            )*
        }
    };
}

define_neon_fp_pair_op! {
    faddp, faddp_scalar, fadd, fp_add_f32, fp_add_f64;
    fmaxp, fmaxp_scalar, fmax, fp_max_f32, fp_max_f64;
    fmaxnmp, fmaxnmp_scalar, fmaxnm, fp_max_nm_f32, fp_max_nm_f64;
    fminp, fminp_scalar, fmin, fp_min_f32, fp_min_f64;
    fminnmp, fminnmp_scalar, fminnm, fp_min_nm_f32, fp_min_nm_f64;
}

macro_rules! define_fp_min_max_v {
    ($($name:ident, $op:ident;)*) => {
        impl Simulator {
            $(
                pub fn $name(
                    &mut self,
                    vform: VectorFormat,
                    dst: &mut LogicVRegister,
                    src: &LogicVRegister,
                ) {
                    debug_assert_eq!(vform, VectorFormat::Format4S);
                    let result1 = Simulator::$op(src.float32(0), src.float32(1));
                    let result2 = Simulator::$op(src.float32(2), src.float32(3));
                    let result = Simulator::$op(result1, result2);
                    dst.clear_for_write(VectorFormat::FormatS);
                    dst.set_float32(0, result);
                }
            )*
        }
    };
}

define_fp_min_max_v! {
    fmaxv, fp_max_f32;
    fminv, fp_min_f32;
    fmaxnmv, fp_max_nm_f32;
    fminnmv, fp_min_nm_f32;
}

macro_rules! define_fp_by_element {
    ($($by_element:ident, $name:ident;)*) => {
        impl Simulator {
            $(
                pub fn $by_element(
                    &mut self,
                    vform: VectorFormat,
                    dst: &mut LogicVRegister,
                    src1: &LogicVRegister,
                    src2: &LogicVRegister,
                    index: i32,
                ) {
                    dst.clear_for_write(vform);
                    let mut temp = LogicVRegister::default();
                    let indexform = if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                        VectorFormat::Format4S
                    } else {
                        VectorFormat::Format2D
                    };
                    self.dup_element(indexform, &mut temp, src2, index);
                    let index_reg = temp;
                    self.$name(vform, dst, src1, &index_reg);
                }
            )*
        }
    };
}

define_fp_by_element! {
    fmul_by_element, fmul;
    fmla_by_element, fmla;
    fmls_by_element, fmls;
    fmulx_by_element, fmulx;
}

impl Simulator {
    pub fn fnmul(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let mut temp = LogicVRegister::default();
        self.fmul(vform, &mut temp, src1, src2);
        let product = temp;
        self.fneg(vform, dst, &product);
    }

    pub fn frecps(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                let op1 = -src1.float32(i);
                let op2 = src2.float32(i);
                let result = Simulator::fp_process_nans_f32(op1, op2);
                dst.set_float32(
                    i,
                    if result.is_nan() {
                        result
                    } else {
                        Simulator::fp_recip_step_fused_f32(op1, op2)
                    },
                );
            } else {
                let op1 = -src1.float64(i);
                let op2 = src2.float64(i);
                let result = Simulator::fp_process_nans_f64(op1, op2);
                dst.set_float64(
                    i,
                    if result.is_nan() {
                        result
                    } else {
                        Simulator::fp_recip_step_fused_f64(op1, op2)
                    },
                );
            }
        }
    }

    pub fn frsqrts(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                let op1 = -src1.float32(i);
                let op2 = src2.float32(i);
                let result = Simulator::fp_process_nans_f32(op1, op2);
                dst.set_float32(
                    i,
                    if result.is_nan() {
                        result
                    } else {
                        Simulator::fp_rsqrt_step_fused_f32(op1, op2)
                    },
                );
            } else {
                let op1 = -src1.float64(i);
                let op2 = src2.float64(i);
                let result = Simulator::fp_process_nans_f64(op1, op2);
                dst.set_float64(
                    i,
                    if result.is_nan() {
                        result
                    } else {
                        Simulator::fp_rsqrt_step_fused_f64(op1, op2)
                    },
                );
            }
        }
    }

    pub fn fcmp(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
        cond: Condition,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let mut result = false;
            if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                let op1 = src1.float32(i);
                let op2 = src2.float32(i);
                let nan_result = Simulator::fp_process_nans_f32(op1, op2);
                if !nan_result.is_nan() {
                    result = match cond {
                        Condition::Eq => op1 == op2,
                        Condition::Ge => op1 >= op2,
                        Condition::Gt => op1 > op2,
                        Condition::Le => op1 <= op2,
                        Condition::Lt => op1 < op2,
                        _ => unreachable!(),
                    };
                }
            } else {
                let op1 = src1.float64(i);
                let op2 = src2.float64(i);
                let nan_result = Simulator::fp_process_nans_f64(op1, op2);
                if !nan_result.is_nan() {
                    result = match cond {
                        Condition::Eq => op1 == op2,
                        Condition::Ge => op1 >= op2,
                        Condition::Gt => op1 > op2,
                        Condition::Le => op1 <= op2,
                        Condition::Lt => op1 < op2,
                        _ => unreachable!(),
                    };
                }
            }
            dst.set_uint(
                vform,
                i,
                if result {
                    max_uint_from_format(vform)
                } else {
                    0
                },
            );
        }
    }

    pub fn fcmp_zero(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        cond: Condition,
    ) {
        let mut temp = LogicVRegister::default();
        self.dup_immediate(vform, &mut temp, 0);
        let zero_reg = temp;
        self.fcmp(vform, dst, src, &zero_reg, cond);
    }

    pub fn fabscmp(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
        cond: Condition,
    ) {
        let mut temp1 = LogicVRegister::default();
        let mut temp2 = LogicVRegister::default();
        self.fabs_(vform, &mut temp1, src1);
        self.fabs_(vform, &mut temp2, src2);
        let (abs_src1, abs_src2) = (temp1, temp2);
        self.fcmp(vform, dst, &abs_src1, &abs_src2, cond);
    }

    pub fn fmla(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                let op1 = src1.float32(i);
                let op2 = src2.float32(i);
                let acc = dst.float32(i);
                let result = Simulator::fp_mul_add_f32(acc, op1, op2);
                dst.set_float32(i, result);
            } else {
                let op1 = src1.float64(i);
                let op2 = src2.float64(i);
                let acc = dst.float64(i);
                let result = Simulator::fp_mul_add_f64(acc, op1, op2);
                dst.set_float64(i, result);
            }
        }
    }

    pub fn fmls(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                let op1 = -src1.float32(i);
                let op2 = src2.float32(i);
                let acc = dst.float32(i);
                let result = Simulator::fp_mul_add_f32(acc, op1, op2);
                dst.set_float32(i, result);
            } else {
                let op1 = -src1.float64(i);
                let op2 = src2.float64(i);
                let acc = dst.float64(i);
                let result = Simulator::fp_mul_add_f64(acc, op1, op2);
                dst.set_float64(i, result);
            }
        }
    }

    pub fn fneg(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                dst.set_float32(i, -src.float32(i));
            } else {
                dst.set_float64(i, -src.float64(i));
            }
        }
    }

    pub fn fabs_(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                let op = src.float32(i);
                dst.set_float32(i, if (1.0f32).copysign(op) < 0.0 { -op } else { op });
            } else {
                let op = src.float64(i);
                dst.set_float64(i, if (1.0f64).copysign(op) < 0.0 { -op } else { op });
            }
        }
    }

    pub fn fabd(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        let mut temp = LogicVRegister::default();
        self.fsub(vform, &mut temp, src1, src2);
        let temp = temp;
        self.fabs_(vform, dst, &temp);
    }

    pub fn fsqrt(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                let result = Simulator::fp_sqrt_f32(src.float32(i));
                dst.set_float32(i, result);
            } else {
                let result = Simulator::fp_sqrt_f64(src.float64(i));
                dst.set_float64(i, result);
            }
        }
    }

    pub fn frint(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        rounding_mode: FPRounding,
        _inexact_exception: bool,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                let input = src.float32(i);
                let rounded = Simulator::fp_round_int(input as f64, rounding_mode) as f32;
                dst.set_float32(i, rounded);
            } else {
                let input = src.float64(i);
                let rounded = Simulator::fp_round_int(input, rounding_mode);
                dst.set_float64(i, rounded);
            }
        }
    }

    pub fn fcvts(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        rounding_mode: FPRounding,
        fbits: i32,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                let op = src.float32(i) * (2.0f32).powi(fbits);
                dst.set_int(
                    vform,
                    i,
                    Simulator::fp_to_int32(op as f64, rounding_mode) as i64,
                );
            } else {
                let op = src.float64(i) * (2.0f64).powi(fbits);
                dst.set_int(vform, i, Simulator::fp_to_int64(op, rounding_mode));
            }
        }
    }

    pub fn fcvtu(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        rounding_mode: FPRounding,
        fbits: i32,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                let op = src.float32(i) * (2.0f32).powi(fbits);
                dst.set_uint(
                    vform,
                    i,
                    Simulator::fp_to_uint32(op as f64, rounding_mode) as u64,
                );
            } else {
                let op = src.float64(i) * (2.0f64).powi(fbits);
                dst.set_uint(vform, i, Simulator::fp_to_uint64(op, rounding_mode));
            }
        }
    }

    pub fn fcvtl(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        debug_assert_eq!(lane_size_in_bytes_from_format(vform), kDRegSize as u32);
        for i in (0..lane_count_from_format(vform)).rev() {
            dst.set_float64(i, src.float32(i) as f64);
        }
    }

    pub fn fcvtl2(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        debug_assert_eq!(lane_size_in_bytes_from_format(vform), kDRegSize as u32);
        let lane_count = lane_count_from_format(vform);
        for i in 0..lane_count {
            dst.set_float64(i, src.float32(i + lane_count) as f64);
        }
    }

    pub fn fcvtn(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        debug_assert_eq!(lane_size_in_bytes_from_format(vform), kSRegSize as u32);
        for i in 0..lane_count_from_format(vform) {
            dst.set_float32(i, src.float64(i) as f32);
        }
        dst.clear_for_write(vform);
    }

    pub fn fcvtn2(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        debug_assert_eq!(lane_size_in_bytes_from_format(vform), kSRegSize as u32);
        let lane_count = lane_count_from_format(vform) / 2;
        for i in (0..lane_count).rev() {
            dst.set_float32(i + lane_count, src.float64(i) as f32);
        }
    }

    pub fn fcvtxn(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        dst.clear_for_write(vform);
        debug_assert_eq!(lane_size_in_bytes_from_format(vform), kSRegSize as u32);
        for i in 0..lane_count_from_format(vform) {
            dst.set_float32(i, fp_to_float_round_odd(src.float64(i)));
        }
    }

    pub fn fcvtxn2(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        debug_assert_eq!(lane_size_in_bytes_from_format(vform), kSRegSize as u32);
        let lane_count = lane_count_from_format(vform) / 2;
        for i in (0..lane_count).rev() {
            dst.set_float32(i + lane_count, fp_to_float_round_odd(src.float64(i)));
        }
    }

    pub fn recip_sqrt_estimate(a: f64) -> f64 {
        let r;
        if a < 0.5 {
            let q0 = (a * 512.0) as i32;
            r = 1.0 / (((q0 as f64) + 0.5) / 512.0).sqrt();
        } else {
            let q1 = (a * 256.0) as i32;
            r = 1.0 / (((q1 as f64) + 0.5) / 256.0).sqrt();
        }
        let s = (256.0 * r + 0.5) as i32;
        (s as f64) / 256.0
    }

    pub fn recip_estimate(a: f64) -> f64 {
        let q = (a * 512.0) as i32;
        let r = 1.0 / (((q as f64) + 0.5) / 512.0);
        let s = (256.0 * r + 0.5) as i32;
        (s as f64) / 256.0
    }

    pub fn frsqrte(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                let input = src.float32(i);
                dst.set_float32(i, fp_recip_sqrt_estimate_f32(input));
            } else {
                let input = src.float64(i);
                dst.set_float64(i, fp_recip_sqrt_estimate_f64(input));
            }
        }
    }

    pub fn frecpe(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        round: FPRounding,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                let input = src.float32(i);
                dst.set_float32(i, fp_recip_estimate_f32(input, round));
            } else {
                let input = src.float64(i);
                dst.set_float64(i, fp_recip_estimate_f64(input, round));
            }
        }
    }

    pub fn ursqrte(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let operand = src.uint(vform, i);
            let result;
            if operand <= 0x3FFFFFFF {
                result = 0xFFFFFFFF;
            } else {
                let dp_operand = operand as f64 * (2.0f64).powi(-32);
                let dp_result =
                    Simulator::recip_sqrt_estimate(dp_operand) * (2.0f64).powi(31);
                result = dp_result as u32;
            }
            dst.set_uint(vform, i, result as u64);
        }
    }

    pub fn urecpe(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            let operand = src.uint(vform, i);
            let result;
            if operand <= 0x7FFFFFFF {
                result = 0xFFFFFFFF;
            } else {
                let dp_operand = operand as f64 * (2.0f64).powi(-32);
                let dp_result = Simulator::recip_estimate(dp_operand) * (2.0f64).powi(31);
                result = dp_result as u32;
            }
            dst.set_uint(vform, i, result as u64);
        }
    }

    pub fn frecpx(&mut self, vform: VectorFormat, dst: &mut LogicVRegister, src: &LogicVRegister) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                let op = src.float32(i);
                let result = if op.is_nan() {
                    Simulator::fp_process_nan_f32(op)
                } else {
                    let sign = float_sign(op);
                    let exp = float_exp(op) as i32;
                    let exp = if exp == 0 { 0xFF - 1 } else { (!exp) & 0xFF };
                    float_pack(sign, exp as u32, 0)
                };
                dst.set_float32(i, result);
            } else {
                let op = src.float64(i);
                let result = if op.is_nan() {
                    Simulator::fp_process_nan_f64(op)
                } else {
                    let sign = double_sign(op);
                    let exp = double_exp(op) as i32;
                    let exp = if exp == 0 { 0x7FF - 1 } else { (!exp) & 0x7FF };
                    double_pack(sign as u64, exp as u64, 0)
                };
                dst.set_float64(i, result);
            }
        }
    }

    pub fn scvtf(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        fbits: i32,
        _round: FPRounding,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                let result = src.int(VectorFormat::FormatS, i) as f32 / (2.0f32).powi(fbits);
                dst.set_float32(i, result);
            } else {
                let result = src.int(VectorFormat::FormatD, i) as f64 / (2.0f64).powi(fbits);
                dst.set_float64(i, result);
            }
        }
    }

    pub fn ucvtf(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src: &LogicVRegister,
        fbits: i32,
        _round: FPRounding,
    ) {
        dst.clear_for_write(vform);
        for i in 0..lane_count_from_format(vform) {
            if lane_size_in_bytes_from_format(vform) == kSRegSize as u32 {
                let result = src.uint(VectorFormat::FormatS, i) as f32 / (2.0f32).powi(fbits);
                dst.set_float32(i, result);
            } else {
                let result = src.uint(VectorFormat::FormatD, i) as f64 / (2.0f64).powi(fbits);
                dst.set_float64(i, result);
            }
        }
    }

    fn dot(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
        is_src1_signed: bool,
        is_src2_signed: bool,
    ) {
        let quarter_vform = vector_format_half_width_double_lanes(
            vector_format_half_width_double_lanes(vform),
        );

        dst.clear_for_write(vform);
        for e in 0..lane_count_from_format(vform) {
            let mut result: u64 = 0;
            for i in 0..4 {
                let index = 4 * e + i;
                let element1 = if is_src1_signed {
                    src1.int(quarter_vform, index)
                } else {
                    src1.uint(quarter_vform, index) as i64
                };
                let element2 = if is_src2_signed {
                    src2.int(quarter_vform, index)
                } else {
                    src2.uint(quarter_vform, index) as i64
                };
                result = result.wrapping_add(element1.wrapping_mul(element2) as u64);
            }
            let value = result.wrapping_add(dst.uint(vform, e));
            dst.set_uint(vform, e, value);
        }
    }

    pub fn sdot(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
    ) {
        self.dot(vform, dst, src1, src2, true, true);
    }

    pub fn bgrp(
        &mut self,
        vform: VectorFormat,
        dst: &mut LogicVRegister,
        src1: &LogicVRegister,
        src2: &LogicVRegister,
        do_bext: bool,
    ) {
        for i in 0..lane_count_from_format(vform) {
            let mut value = src1.uint(vform, i);
            let mut mask = src2.uint(vform, i);
            let mut high_pos = 0;
            let mut low_pos = 0;
            let mut result_high: u64 = 0;
            let mut result_low: u64 = 0;
            for _ in 0..lane_size_in_bits_from_format(vform) {
                if (mask & 1) == 0 {
                    result_high |= (value & 1) << high_pos;
                    high_pos += 1;
                } else {
                    result_low |= (value & 1) << low_pos;
                    low_pos += 1;
                }
                mask >>= 1;
                value >>= 1;
            }

            if !do_bext {
                result_low |= result_high << low_pos;
            }

            dst.set_uint(vform, i, result_low);
        }
    }
}

fn fp_to_float_round_odd(value: f64) -> f32 {
    let narrowed = value as f32;
    if (narrowed as f64) == value || narrowed.is_nan() || narrowed.is_infinite() {
        return narrowed;
    }
    let bits = narrowed.to_bits();
    f32::from_bits(bits | 1)
}

fn fp_recip_sqrt_estimate_f32(op: f32) -> f32 {
    if op.is_nan() {
        return Simulator::fp_process_nan_f32(op);
    }
    if op == 0.0 {
        return if (1.0f32).copysign(op) < 0.0 {
            f32::NEG_INFINITY
        } else {
            f32::INFINITY
        };
    }
    if (1.0f32).copysign(op) < 0.0 {
        return Simulator::fp_default_nan_f32();
    }
    if op.is_infinite() {
        return 0.0;
    }

    let mut exp = float_exp(op) as i32;
    let mut fraction = (float_mantissa(op) as u64) << 29;

    if exp == 0 {
        while ((fraction >> 51) & 1) == 0 {
            fraction = (fraction & ((1 << 51) - 1)) << 1;
            exp -= 1;
        }
        fraction = (fraction & ((1 << 51) - 1)) << 1;
    }

    let scaled = if (exp & 1) == 0 {
        double_pack(0, 1022, ((fraction >> 44) & 0xff) << 44)
    } else {
        double_pack(0, 1021, ((fraction >> 44) & 0xff) << 44)
    };

    let result_exp = (380 - exp) / 2;
    let estimate = Simulator::recip_sqrt_estimate(scaled).to_bits();

    float_pack(
        0,
        (result_exp as u32) & 0xff,
        ((estimate >> 29) & 0x7fffff) as u32,
    )
}

fn fp_recip_sqrt_estimate_f64(op: f64) -> f64 {
    if op.is_nan() {
        return Simulator::fp_process_nan_f64(op);
    }
    if op == 0.0 {
        return if (1.0f64).copysign(op) < 0.0 {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    if (1.0f64).copysign(op) < 0.0 {
        return Simulator::fp_default_nan_f64();
    }
    if op.is_infinite() {
        return 0.0;
    }

    let mut exp = double_exp(op) as i32;
    let mut fraction = double_mantissa(op);

    if exp == 0 {
        while ((fraction >> 51) & 1) == 0 {
            fraction = (fraction & ((1 << 51) - 1)) << 1;
            exp -= 1;
        }
        fraction = (fraction & ((1 << 51) - 1)) << 1;
    }

    let scaled = if (exp & 1) == 0 {
        double_pack(0, 1022, ((fraction >> 44) & 0xff) << 44)
    } else {
        double_pack(0, 1021, ((fraction >> 44) & 0xff) << 44)
    };

    let result_exp = (3068 - exp) / 2;
    let estimate = Simulator::recip_sqrt_estimate(scaled).to_bits();

    double_pack(
        0,
        (result_exp as u64) & 0x7ff,
        estimate & 0xfffffffffffff,
    )
}

fn fp_recip_estimate_f32(op: f32, rounding: FPRounding) -> f32 {
    let sign = float_sign(op);

    if op.is_nan() {
        return Simulator::fp_process_nan_f32(op);
    }
    if op.is_infinite() {
        return if sign == 1 { -0.0 } else { 0.0 };
    }
    if op == 0.0 {
        return if sign == 1 {
            f32::NEG_INFINITY
        } else {
            f32::INFINITY
        };
    }
    if (op.abs() as f64) < (2.0f64).powi(-128) {
        let overflow_to_inf = match rounding {
            FPRounding::FPTieEven => true,
            FPRounding::FPPositiveInfinity => sign == 0,
            FPRounding::FPNegativeInfinity => sign == 1,
            FPRounding::FPZero => false,
            _ => false,
        };
        if overflow_to_inf {
            return if sign == 1 {
                f32::NEG_INFINITY
            } else {
                f32::INFINITY
            };
        }
        return float_pack(sign, 0xFE, 0x07FFFFF);
    }

    let mut exp = float_exp(op) as i32;
    let mut fraction = (float_mantissa(op) as u64) << 29;

    if exp == 0 {
        if ((fraction >> 51) & 1) == 0 {
            exp -= 1;
            fraction = (fraction & ((1 << 50) - 1)) << 2;
        } else {
            fraction = (fraction & ((1 << 51) - 1)) << 1;
        }
    }

    let scaled = double_pack(0, 1022, ((fraction >> 44) & 0xff) << 44);
    let mut result_exp = 253 - exp;

    let estimate = Simulator::recip_estimate(scaled);
    let mut fraction = double_mantissa(estimate);
    if result_exp == 0 {
        fraction = (1u64 << 51) | ((fraction >> 1) & ((1 << 51) - 1));
    } else if result_exp == -1 {
        fraction = (1u64 << 50) | ((fraction >> 2) & ((1 << 50) - 1));
        result_exp = 0;
    }

    float_pack(
        sign,
        (result_exp as u32) & 0xff,
        ((fraction >> 29) & 0x7fffff) as u32,
    )
}

fn fp_recip_estimate_f64(op: f64, rounding: FPRounding) -> f64 {
    let sign = double_sign(op);

    if op.is_nan() {
        return Simulator::fp_process_nan_f64(op);
    }
    if op.is_infinite() {
        return if sign == 1 { -0.0 } else { 0.0 };
    }
    if op == 0.0 {
        return if sign == 1 {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    if op.abs() < (2.0f64).powi(-1024) {
        let overflow_to_inf = match rounding {
            FPRounding::FPTieEven => true,
            FPRounding::FPPositiveInfinity => sign == 0,
            FPRounding::FPNegativeInfinity => sign == 1,
            FPRounding::FPZero => false,
            _ => false,
        };
        if overflow_to_inf {
            return if sign == 1 {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            };
        }
        return double_pack(sign as u64, 0x7FE, 0x0FFFFFFFFFFFFF);
    }

    let mut exp = double_exp(op) as i32;
    let mut fraction = double_mantissa(op);

    if exp == 0 {
        if ((fraction >> 51) & 1) == 0 {
            exp -= 1;
            fraction = (fraction & ((1 << 50) - 1)) << 2;
        } else {
            fraction = (fraction & ((1 << 51) - 1)) << 1;
        }
    }

    let scaled = double_pack(0, 1022, ((fraction >> 44) & 0xff) << 44);
    let mut result_exp = 2045 - exp;

    let estimate = Simulator::recip_estimate(scaled);
    let mut fraction = double_mantissa(estimate);
    if result_exp == 0 {
        fraction = (1u64 << 51) | ((fraction >> 1) & ((1 << 51) - 1));
    } else if result_exp == -1 {
        fraction = (1u64 << 50) | ((fraction >> 2) & ((1 << 50) - 1));
        result_exp = 0;
    }

    double_pack(
        sign as u64,
        (result_exp as u64) & 0x7ff,
        fraction & 0xfffffffffffff,
    )
}

macro_rules! define_neon_load_store_struct {
    ($($name:ident, $name_index:ident, $name_replicate:ident, $count:expr, $load:expr;)*) => {
        impl Simulator {
            $(
                pub fn $name(
                    &mut self,
                    vform: VectorFormat,
                    regs: &mut [LogicVRegister; $count],
                    addr: u64,
                ) {
                    let esize = lane_size_in_bytes_from_format(vform) as u64;
                    let mut addrs = [0u64; $count];
                    for j in 0..$count {
                        addrs[j] = addr + (j as u64) * esize;
                        if $load {
                            regs[j].clear_for_write(vform);
                        }
                    }
                    for i in 0..lane_count_from_format(vform) {
                        for j in 0..$count {
                            if $load {
                                self.read_uint_from_mem(&mut regs[j], vform, i, addrs[j]);
                            } else {
                                self.write_uint_to_mem(&regs[j], vform, i, addrs[j]);
                            }
                            addrs[j] += ($count as u64) * esize;
                        }
                    }
                }

                pub fn $name_index(
                    &mut self,
                    vform: VectorFormat,
                    regs: &mut [LogicVRegister; $count],
                    index: i32,
                    addr: u64,
                ) {
                    let esize = lane_size_in_bytes_from_format(vform) as u64;
                    for j in 0..$count {
                        if $load {
                            if $count > 1 {
                                regs[j].clear_for_write(vform);
                            }
                            self.read_uint_from_mem(
                                &mut regs[j],
                                vform,
                                index,
                                addr + (j as u64) * esize,
                            );
                        } else {
                            self.write_uint_to_mem(
                                &regs[j],
                                vform,
                                index,
                                addr + (j as u64) * esize,
                            );
                        }
                    }
                }

                pub fn $name_replicate(
                    &mut self,
                    vform: VectorFormat,
                    regs: &mut [LogicVRegister; $count],
                    addr: u64,
                ) {
                    debug_assert!($load);
                    let esize = lane_size_in_bytes_from_format(vform) as u64;
                    for j in 0..$count {
                        regs[j].clear_for_write(vform);
                    }
                    for i in 0..lane_count_from_format(vform) {
                        for j in 0..$count {
                            self.read_uint_from_mem(
                                &mut regs[j],
                                vform,
                                i,
                                addr + (j as u64) * esize,
                            );
                        }
                    }
                }
            )*
        }
    };
}

define_neon_load_store_struct! {
    ld1, ld1_index, ld1r, 1, true;
    ld2, ld2_index, ld2r, 2, true;
    ld3, ld3_index, ld3r, 3, true;
    ld4, ld4_index, ld4r, 4, true;
    st1, st1_index, st1_replicate, 1, false;
    st2, st2_index, st2_replicate, 2, false;
    st3, st3_index, st3_replicate, 3, false;
    st4, st4_index, st4_replicate, 4, false;
}

impl Simulator {
    pub fn read_uint_from_mem(
        &self,
        dst: &mut LogicVRegister,
        vform: VectorFormat,
        index: i32,
        addr: u64,
    ) {
        let size = lane_size_in_bytes_from_format(vform) as usize;
        let value = self.memory_read(addr, size);
        dst.set_uint(vform, index, value);
    }

    pub fn write_uint_to_mem(
        &mut self,
        src: &LogicVRegister,
        vform: VectorFormat,
        index: i32,
        addr: u64,
    ) {
        let size = lane_size_in_bytes_from_format(vform) as usize;
        let value = src.uint(vform, index);
        self.memory_write(addr, value, size);
    }
}
