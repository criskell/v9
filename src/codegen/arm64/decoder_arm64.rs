#![allow(non_upper_case_globals)]

use crate::codegen::arm64::instructions_arm64::Instruction;

pub trait DecoderVisitor {
    fn visit_pc_rel_addressing(&mut self, instr: Instruction);
    fn visit_add_sub_immediate(&mut self, instr: Instruction);
    fn visit_logical_immediate(&mut self, instr: Instruction);
    fn visit_move_wide_immediate(&mut self, instr: Instruction);
    fn visit_bitfield(&mut self, instr: Instruction);
    fn visit_extract(&mut self, instr: Instruction);
    fn visit_unconditional_branch(&mut self, instr: Instruction);
    fn visit_unconditional_branch_to_register(&mut self, instr: Instruction);
    fn visit_compare_branch(&mut self, instr: Instruction);
    fn visit_test_branch(&mut self, instr: Instruction);
    fn visit_conditional_branch(&mut self, instr: Instruction);
    fn visit_system(&mut self, instr: Instruction);
    fn visit_exception(&mut self, instr: Instruction);
    fn visit_load_store_pair_post_index(&mut self, instr: Instruction);
    fn visit_load_store_pair_pre_index(&mut self, instr: Instruction);
    fn visit_load_store_pair_offset(&mut self, instr: Instruction);
    fn visit_load_literal(&mut self, instr: Instruction);
    fn visit_load_store_unscaled_offset(&mut self, instr: Instruction);
    fn visit_load_store_post_index(&mut self, instr: Instruction);
    fn visit_load_store_pre_index(&mut self, instr: Instruction);
    fn visit_load_store_register_offset(&mut self, instr: Instruction);
    fn visit_load_store_unsigned_offset(&mut self, instr: Instruction);
    fn visit_logical_shifted(&mut self, instr: Instruction);
    fn visit_add_sub_with_carry(&mut self, instr: Instruction);
    fn visit_conditional_compare_register(&mut self, instr: Instruction);
    fn visit_conditional_compare_immediate(&mut self, instr: Instruction);
    fn visit_conditional_select(&mut self, instr: Instruction);
    fn visit_data_processing_1_source(&mut self, instr: Instruction);
    fn visit_data_processing_2_source(&mut self, instr: Instruction);
    fn visit_add_sub_shifted(&mut self, instr: Instruction);
    fn visit_add_sub_extended(&mut self, instr: Instruction);
    fn visit_data_processing_3_source(&mut self, instr: Instruction);
    fn visit_fp_fixed_point_convert(&mut self, instr: Instruction);
    fn visit_fp_integer_convert(&mut self, instr: Instruction);
    fn visit_fp_data_processing_1_source(&mut self, instr: Instruction);
    fn visit_fp_data_processing_2_source(&mut self, instr: Instruction);
    fn visit_fp_data_processing_3_source(&mut self, instr: Instruction);
    fn visit_fp_compare(&mut self, instr: Instruction);
    fn visit_fp_immediate(&mut self, instr: Instruction);
    fn visit_fp_conditional_compare(&mut self, instr: Instruction);
    fn visit_fp_conditional_select(&mut self, instr: Instruction);
    fn visit_unallocated(&mut self, instr: Instruction);
    fn visit_unimplemented(&mut self, instr: Instruction);
}

pub fn decode<V: DecoderVisitor>(visitor: &mut V, instr: Instruction) {
    if instr.bits(28, 27) == 0 {
        visitor.visit_unallocated(instr);
        return;
    }
    match instr.bits(27, 24) {
        0x0 => decode_pc_rel_addressing(visitor, instr),
        0x1 => decode_data_processing_immediate(visitor, instr),
        0xA | 0xB => decode_data_processing(visitor, instr),
        0x2 => decode_logical(visitor, instr),
        0x3 => decode_bitfield_extract(visitor, instr),
        0x4 | 0x5 | 0x6 | 0x7 => decode_branch_system_exception(visitor, instr),
        0x8 | 0x9 | 0xC | 0xD => decode_load_store(visitor, instr),
        0xE | 0xF => decode_fp(visitor, instr),
        _ => unreachable!(),
    }
}

fn decode_pc_rel_addressing<V: DecoderVisitor>(visitor: &mut V, instr: Instruction) {
    debug_assert_eq!(0x0, instr.bits(27, 24));
    debug_assert_eq!(0x1, instr.bit(28));
    visitor.visit_pc_rel_addressing(instr);
}

fn decode_data_processing_immediate<V: DecoderVisitor>(visitor: &mut V, instr: Instruction) {
    debug_assert_eq!(instr.bit(28), 1);
    debug_assert_eq!(0x1, instr.bits(27, 24));
    if instr.bits(23, 22) == 2 {
        visitor.visit_unallocated(instr);
    } else if instr.bit(23) == 1 {
        if instr.bits(30, 29) != 0 || instr.bits(21, 20) != 0 {
            visitor.visit_unallocated(instr);
        } else {
            visitor.visit_unimplemented(instr);
        }
    } else {
        visitor.visit_add_sub_immediate(instr);
    }
}

fn decode_logical<V: DecoderVisitor>(visitor: &mut V, instr: Instruction) {
    debug_assert_eq!(0x2, instr.bits(27, 24));
    if instr.mask(0x80400000) == 0x00400000 {
        visitor.visit_unallocated(instr);
    } else if instr.bit(23) == 0 {
        visitor.visit_logical_immediate(instr);
    } else if instr.bits(30, 29) == 0x1 {
        visitor.visit_unallocated(instr);
    } else {
        visitor.visit_move_wide_immediate(instr);
    }
}

fn decode_bitfield_extract<V: DecoderVisitor>(visitor: &mut V, instr: Instruction) {
    debug_assert_eq!(0x3, instr.bits(27, 24));
    if (instr.mask(0x80400000) == 0x80000000)
        || (instr.mask(0x80400000) == 0x00400000)
        || (instr.mask(0x80008000) == 0x00008000)
    {
        visitor.visit_unallocated(instr);
    } else if instr.bit(23) == 0 {
        if (instr.mask(0x80200000) == 0x00200000) || (instr.mask(0x60000000) == 0x60000000) {
            visitor.visit_unallocated(instr);
        } else {
            visitor.visit_bitfield(instr);
        }
    } else if (instr.mask(0x60200000) == 0x00200000) || (instr.mask(0x60000000) != 0x00000000) {
        visitor.visit_unallocated(instr);
    } else {
        visitor.visit_extract(instr);
    }
}

fn decode_branch_system_exception<V: DecoderVisitor>(visitor: &mut V, instr: Instruction) {
    debug_assert_eq!(0x4, instr.bits(27, 24) & 0xC);
    match instr.bits(31, 29) {
        0 | 4 => visitor.visit_unconditional_branch(instr),
        1 | 5 => {
            if instr.bit(25) == 0 {
                visitor.visit_compare_branch(instr);
            } else {
                visitor.visit_test_branch(instr);
            }
        }
        2 => {
            if instr.bit(25) == 0 {
                if instr.bit(24) == 0x1 {
                    visitor.visit_unallocated(instr);
                } else {
                    visitor.visit_conditional_branch(instr);
                }
            } else {
                visitor.visit_unallocated(instr);
            }
        }
        6 => {
            if instr.bit(25) == 0 {
                if instr.bit(24) == 0 {
                    if (instr.bits(4, 2) != 0)
                        || (instr.mask(0x00E0001D) == 0x00200001)
                        || (instr.mask(0x00E0001D) == 0x00400001)
                        || (instr.mask(0x00E0001E) == 0x00200002)
                        || (instr.mask(0x00E0001E) == 0x00400002)
                        || (instr.mask(0x00E0001C) == 0x00600000)
                        || (instr.mask(0x00E0001C) == 0x00800000)
                        || (instr.mask(0x00E0001F) == 0x00A00000)
                        || (instr.mask(0x00C0001C) == 0x00C00000)
                    {
                        visitor.visit_unallocated(instr);
                    } else {
                        visitor.visit_exception(instr);
                    }
                } else if instr.bits(23, 22) == 0 {
                    let masked_003ff0e0 = instr.mask(0x003FF0E0);
                    if (instr.bits(21, 19) == 0x4)
                        || (masked_003ff0e0 == 0x00033000)
                        || (masked_003ff0e0 == 0x003FF020)
                        || (masked_003ff0e0 == 0x003FF060)
                        || (masked_003ff0e0 == 0x003FF0E0)
                        || (instr.mask(0x00388000) == 0x00008000)
                        || (instr.mask(0x0038E000) == 0x00000000)
                        || (instr.mask(0x0039E000) == 0x00002000)
                        || (instr.mask(0x003AE000) == 0x00002000)
                        || (instr.mask(0x003CE000) == 0x00042000)
                        || (instr.mask(0x0038F000) == 0x00005000)
                        || (instr.mask(0x0038E000) == 0x00006000)
                    {
                        visitor.visit_unallocated(instr);
                    } else {
                        visitor.visit_system(instr);
                    }
                } else {
                    visitor.visit_unallocated(instr);
                }
            } else if (instr.bit(24) == 0x1)
                || (instr.bits(20, 16) != 0x1F)
                || (instr.bits(15, 10) != 0)
                || (instr.bits(4, 0) != 0)
                || (instr.bits(24, 21) == 0x3)
                || (instr.bits(24, 22) == 0x3)
            {
                visitor.visit_unallocated(instr);
            } else {
                visitor.visit_unconditional_branch_to_register(instr);
            }
        }
        3 | 7 => visitor.visit_unallocated(instr),
        _ => unreachable!(),
    }
}

fn decode_data_processing<V: DecoderVisitor>(visitor: &mut V, instr: Instruction) {
    debug_assert!((instr.bits(27, 24) == 0xA) || (instr.bits(27, 24) == 0xB));

    if instr.bit(24) == 0 {
        if instr.bit(28) == 0 {
            if instr.mask(0x80008000) == 0x00008000 {
                visitor.visit_unallocated(instr);
            } else {
                visitor.visit_logical_shifted(instr);
            }
            return;
        }
        match instr.bits(23, 21) {
            0 => {
                if instr.mask(0x0000FC00) != 0 {
                    visitor.visit_unallocated(instr);
                } else {
                    visitor.visit_add_sub_with_carry(instr);
                }
            }
            2 => {
                if (instr.bit(29) == 0) || (instr.mask(0x00000410) != 0) {
                    visitor.visit_unallocated(instr);
                } else if instr.bit(11) == 0 {
                    visitor.visit_conditional_compare_register(instr);
                } else {
                    visitor.visit_conditional_compare_immediate(instr);
                }
            }
            4 => {
                if instr.mask(0x20000800) != 0x00000000 {
                    visitor.visit_unallocated(instr);
                } else {
                    visitor.visit_conditional_select(instr);
                }
            }
            6 => {
                if instr.bit(29) == 0x1 {
                    visitor.visit_unallocated(instr);
                } else if instr.bit(30) == 0 {
                    if (instr.bits(15, 11) == 0)
                        || (instr.bits(15, 12) == 0x1)
                        || ((instr.bits(15, 12) > 2) && (instr.bits(15, 12) < 6))
                        || (instr.bits(15, 10) > 0x1B)
                    {
                        visitor.visit_unallocated(instr);
                    } else {
                        visitor.visit_data_processing_2_source(instr);
                    }
                } else if (instr.bits(20, 16) != 0)
                    || (instr.bits(15, 10) > 8)
                    || (instr.mask(0xFFFFFC00) == 0x5AC00C00)
                {
                    visitor.visit_unallocated(instr);
                } else {
                    visitor.visit_data_processing_1_source(instr);
                }
            }
            _ => visitor.visit_unallocated(instr),
        }
        return;
    }

    if instr.bit(28) == 0 {
        if instr.bit(21) == 0 {
            if (instr.bits(23, 22) == 0x3) || (instr.mask(0x80008000) == 0x00008000) {
                visitor.visit_unallocated(instr);
            } else {
                visitor.visit_add_sub_shifted(instr);
            }
        } else if (instr.mask(0x00C00000) != 0x00000000)
            || (instr.mask(0x00001400) == 0x00001400)
            || (instr.mask(0x00001800) == 0x00001800)
        {
            visitor.visit_unallocated(instr);
        } else {
            visitor.visit_add_sub_extended(instr);
        }
        return;
    }

    if (instr.bit(30) == 0x1)
        || (instr.bits(30, 29) == 0x1)
        || (instr.mask(0xE0600000) == 0x00200000)
        || (instr.mask(0xE0608000) == 0x00400000)
        || (instr.mask(0x60608000) == 0x00408000)
        || (instr.mask(0x60E00000) == 0x00E00000)
        || (instr.mask(0x60E00000) == 0x00800000)
        || (instr.mask(0x60E00000) == 0x00600000)
    {
        visitor.visit_unallocated(instr);
    } else {
        visitor.visit_data_processing_3_source(instr);
    }
}

fn decode_load_store<V: DecoderVisitor>(visitor: &mut V, instr: Instruction) {
    debug_assert_eq!(0x8, instr.bits(27, 24) & 0xA);

    if (instr.bit(28) == 0) && (instr.bit(29) == 0) && (instr.bit(26) == 1) {
        visitor.visit_unimplemented(instr);
        return;
    }

    if instr.bit(24) == 0 {
        if instr.bit(28) == 0 {
            if instr.bit(29) == 0 {
                visitor.visit_unimplemented(instr);
            } else if (instr.bits(31, 30) == 0x3) || (instr.mask(0xC4400000) == 0x40000000) {
                visitor.visit_unallocated(instr);
            } else if instr.bit(23) == 0 {
                visitor.visit_unimplemented(instr);
            } else {
                visitor.visit_load_store_pair_post_index(instr);
            }
            return;
        }
        if instr.bit(29) == 0 {
            if instr.mask(0xC4000000) == 0xC4000000 {
                visitor.visit_unallocated(instr);
            } else {
                visitor.visit_load_literal(instr);
            }
            return;
        }
        if (instr.mask(0x44800000) == 0x44800000) || (instr.mask(0x84800000) == 0x84800000) {
            visitor.visit_unallocated(instr);
            return;
        }
        if instr.bit(21) == 0 {
            match instr.bits(11, 10) {
                0 => visitor.visit_load_store_unscaled_offset(instr),
                1 => {
                    if instr.mask(0xC4C00000) == 0xC0800000 {
                        visitor.visit_unallocated(instr);
                    } else {
                        visitor.visit_load_store_post_index(instr);
                    }
                }
                2 => visitor.visit_unimplemented(instr),
                3 => {
                    if instr.mask(0xC4C00000) == 0xC0800000 {
                        visitor.visit_unallocated(instr);
                    } else {
                        visitor.visit_load_store_pre_index(instr);
                    }
                }
                _ => unreachable!(),
            }
            return;
        }
        if instr.bits(11, 10) == 0x2 {
            if instr.bit(14) == 0 {
                visitor.visit_unallocated(instr);
            } else {
                visitor.visit_load_store_register_offset(instr);
            }
        } else {
            visitor.visit_unimplemented(instr);
        }
        return;
    }

    if instr.bit(28) == 0 {
        if instr.bit(29) == 0 {
            visitor.visit_unallocated(instr);
        } else if (instr.bits(31, 30) == 0x3) || (instr.mask(0xC4400000) == 0x40000000) {
            visitor.visit_unallocated(instr);
        } else if instr.bit(23) == 0 {
            visitor.visit_load_store_pair_offset(instr);
        } else {
            visitor.visit_load_store_pair_pre_index(instr);
        }
        return;
    }

    if instr.bit(29) == 0 {
        visitor.visit_unallocated(instr);
    } else if (instr.mask(0x84C00000) == 0x80C00000)
        || (instr.mask(0x44800000) == 0x44800000)
        || (instr.mask(0x84800000) == 0x84800000)
    {
        visitor.visit_unallocated(instr);
    } else {
        visitor.visit_load_store_unsigned_offset(instr);
    }
}

fn decode_fp<V: DecoderVisitor>(visitor: &mut V, instr: Instruction) {
    debug_assert!((instr.bits(27, 24) == 0xE) || (instr.bits(27, 24) == 0xF));

    if instr.bit(28) == 0 {
        visitor.visit_unimplemented(instr);
        return;
    }
    if instr.bits(31, 30) == 0x3 {
        visitor.visit_unallocated(instr);
        return;
    }
    if instr.bits(31, 30) == 0x1 {
        visitor.visit_unimplemented(instr);
        return;
    }
    if instr.bit(29) != 0 {
        visitor.visit_unallocated(instr);
        return;
    }
    if instr.bit(24) != 0 {
        debug_assert_eq!(0, instr.bit(30));
        if instr.mask(0xA0800000) != 0 {
            visitor.visit_unallocated(instr);
        } else {
            visitor.visit_fp_data_processing_3_source(instr);
        }
        return;
    }
    if instr.bit(21) == 0 {
        if (instr.bit(23) == 1)
            || (instr.bit(18) == 1)
            || (instr.mask(0x80008000) == 0x00000000)
            || (instr.mask(0x000E0000) == 0x00000000)
            || (instr.mask(0x000E0000) == 0x000A0000)
            || (instr.mask(0x00160000) == 0x00000000)
            || (instr.mask(0x00160000) == 0x00120000)
        {
            visitor.visit_unallocated(instr);
        } else {
            visitor.visit_fp_fixed_point_convert(instr);
        }
        return;
    }
    if instr.bits(15, 10) == 32 {
        visitor.visit_unallocated(instr);
    } else if instr.bits(15, 10) == 0 {
        if (instr.bits(23, 22) == 0x3)
            || (instr.mask(0x000E0000) == 0x000A0000)
            || (instr.mask(0x000E0000) == 0x000C0000)
            || (instr.mask(0x00160000) == 0x00120000)
            || (instr.mask(0x00160000) == 0x00140000)
            || (instr.mask(0x20C40000) == 0x00800000)
            || (instr.mask(0x20C60000) == 0x00840000)
            || (instr.mask(0xA0C60000) == 0x80060000)
            || (instr.mask(0xA0C60000) == 0x00860000)
            || (instr.mask(0xA0CE0000) == 0x80860000)
            || (instr.mask(0xA0CE0000) == 0x804E0000)
            || (instr.mask(0xA0CE0000) == 0x000E0000)
            || (instr.mask(0xA0D60000) == 0x00160000)
            || (instr.mask(0xA0D60000) == 0x80560000)
            || (instr.mask(0xA0D60000) == 0x80960000)
        {
            visitor.visit_unallocated(instr);
        } else {
            visitor.visit_fp_integer_convert(instr);
        }
    } else if instr.bits(14, 10) == 16 {
        let masked_a0df8000 = instr.mask(0xA0DF8000);
        if (instr.mask(0x80180000) != 0)
            || (masked_a0df8000 == 0x00020000)
            || (masked_a0df8000 == 0x00030000)
            || (masked_a0df8000 == 0x00068000)
            || (masked_a0df8000 == 0x00428000)
            || (masked_a0df8000 == 0x00430000)
            || (masked_a0df8000 == 0x00468000)
            || (instr.mask(0xA0D80000) == 0x00800000)
            || (instr.mask(0xA0DE0000) == 0x00C00000)
            || (instr.mask(0xA0DF0000) == 0x00C30000)
            || (instr.mask(0xA0DC0000) == 0x00C40000)
        {
            visitor.visit_unallocated(instr);
        } else {
            visitor.visit_fp_data_processing_1_source(instr);
        }
    } else if instr.bits(13, 10) == 8 {
        if (instr.bits(15, 14) != 0)
            || (instr.bits(2, 0) != 0)
            || (instr.mask(0x80800000) != 0x00000000)
        {
            visitor.visit_unallocated(instr);
        } else {
            visitor.visit_fp_compare(instr);
        }
    } else if instr.bits(12, 10) == 4 {
        if (instr.bits(9, 5) != 0) || (instr.mask(0x80800000) != 0x00000000) {
            visitor.visit_unallocated(instr);
        } else {
            visitor.visit_fp_immediate(instr);
        }
    } else if instr.mask(0x80800000) != 0x00000000 {
        visitor.visit_unallocated(instr);
    } else {
        match instr.bits(11, 10) {
            1 => visitor.visit_fp_conditional_compare(instr),
            2 => {
                if (instr.bits(15, 14) == 0x3)
                    || (instr.mask(0x00009000) == 0x00009000)
                    || (instr.mask(0x0000A000) == 0x0000A000)
                {
                    visitor.visit_unallocated(instr);
                } else {
                    visitor.visit_fp_data_processing_2_source(instr);
                }
            }
            3 => visitor.visit_fp_conditional_select(instr),
            _ => unreachable!(),
        }
    }
}
