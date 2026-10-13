#![allow(non_upper_case_globals)]

pub const kContinueShift: u32 = 7;
pub const kContinueBit: u32 = 1 << kContinueShift;
pub const kDataMask: u32 = kContinueBit - 1;

pub fn vlq_encode_unsigned(process_byte: &mut impl FnMut(u8), value: u32) {
    let mut value = value;
    while value >= kContinueBit {
        process_byte((value | kContinueBit) as u8);
        value >>= kContinueShift;
    }
    process_byte(value as u8);
}

pub fn vlq_convert_to_unsigned(value: i32) -> u32 {
    debug_assert_ne!(value, i32::MIN);
    let is_negative = value < 0;
    ((if is_negative { -value } else { value } as u32) << 1) | is_negative as u32
}

pub fn vlq_encode(process_byte: &mut impl FnMut(u8), value: i32) {
    vlq_encode_unsigned(process_byte, vlq_convert_to_unsigned(value));
}

pub fn vlq_decode_unsigned(get_next: &mut impl FnMut() -> u8) -> u32 {
    let mut cur_byte = get_next();
    if (cur_byte as u32) <= kDataMask {
        return cur_byte as u32;
    }
    let mut bits = cur_byte as u32 & kDataMask;
    let mut shift = kContinueShift;
    while shift <= 32 {
        cur_byte = get_next();
        bits |= (cur_byte as u32 & kDataMask) << shift;
        if (cur_byte as u32) <= kDataMask {
            break;
        }
        shift += kContinueShift;
    }
    bits
}

pub fn vlq_decode(get_next: &mut impl FnMut() -> u8) -> i32 {
    let bits = vlq_decode_unsigned(get_next);
    let is_negative = (bits & 1) == 1;
    let result = (bits >> 1) as i32;
    if is_negative {
        -result
    } else {
        result
    }
}
