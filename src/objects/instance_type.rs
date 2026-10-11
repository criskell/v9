//! Ported from `src/objects/instance-type.h`.
//!
//! The instance type is a 16-bit field in every `Map`. Its bit layout is not
//! arbitrary: the encoding is chosen so that the hot predicates are a single
//! mask-and-compare, and so that subclass tests are range checks.
//!
//! The string half of the encoding is reproduced here bit for bit, because
//! those bits are load-bearing -- `IsString`, the representation and the
//! encoding are all read straight out of the instance type. The non-string
//! types are assigned by Torque in V8 (`TORQUE_ASSIGNED_INSTANCE_TYPES`);
//! `vx` assigns them here instead, preserving the ordering constraints that
//! the range checks depend on.

// ---------------------------------------------------------------------------
// String bit encoding. All of bits 7-15 are clear iff the object is a string.
// ---------------------------------------------------------------------------

/// All high-order bits; set for anything that is not a string.
pub const K_IS_NOT_STRING_MASK: u16 = !((1 << 7) - 1);
pub const K_STRING_TAG: u16 = 0x0;

/// Bits 0-2: how the string's characters are stored.
pub const K_STRING_REPRESENTATION_MASK: u16 = (1 << 3) - 1;

pub const K_SEQ_STRING_TAG: u16 = 0x0;
pub const K_CONS_STRING_TAG: u16 = 0x1;
pub const K_EXTERNAL_STRING_TAG: u16 = 0x2;
pub const K_SLICED_STRING_TAG: u16 = 0x3;
pub const K_THIN_STRING_TAG: u16 = 0x5;

/// Bit 0 alone says whether the string is indirect (cons/sliced/thin), which
/// is the check that string reads have to make first.
pub const K_IS_INDIRECT_STRING_MASK: u16 = 1 << 0;
pub const K_IS_INDIRECT_STRING_TAG: u16 = 1 << 0;

/// Bit 3: one-byte (Latin-1) or two-byte (UTF-16) characters.
pub const K_STRING_ENCODING_MASK: u16 = 1 << 3;
pub const K_TWO_BYTE_STRING_TAG: u16 = 0;
pub const K_ONE_BYTE_STRING_TAG: u16 = 1 << 3;

pub const K_STRING_REPRESENTATION_AND_ENCODING_MASK: u16 =
    K_STRING_REPRESENTATION_MASK | K_STRING_ENCODING_MASK;
pub const K_SEQ_ONE_BYTE_STRING_TAG: u16 = K_SEQ_STRING_TAG | K_ONE_BYTE_STRING_TAG;
pub const K_SEQ_TWO_BYTE_STRING_TAG: u16 = K_SEQ_STRING_TAG | K_TWO_BYTE_STRING_TAG;

/// Bit 6: *not* internalized. Internalized is the zero state so that
/// "not an internalized string" is one contiguous mask together with
/// `kIsNotStringMask`.
pub const K_IS_NOT_INTERNALIZED_MASK: u16 = 1 << 6;
pub const K_NOT_INTERNALIZED_TAG: u16 = 1 << 6;
pub const K_INTERNALIZED_TAG: u16 = 0;

// ---------------------------------------------------------------------------
// InstanceType
// ---------------------------------------------------------------------------

/// `InstanceType`. Represented as a plain `u16` rather than a Rust enum: the
/// values are a bit encoding that gets masked and range-checked, which an
/// exhaustive enum would fight rather than help. The named constants below
/// are the whole vocabulary.
pub type InstanceType = u16;

// -- String types (bit-encoded, matching V8 exactly) ------------------------

pub const INTERNALIZED_TWO_BYTE_STRING_TYPE: InstanceType =
    K_TWO_BYTE_STRING_TAG | K_SEQ_STRING_TAG | K_INTERNALIZED_TAG;
pub const INTERNALIZED_ONE_BYTE_STRING_TYPE: InstanceType =
    K_ONE_BYTE_STRING_TAG | K_SEQ_STRING_TAG | K_INTERNALIZED_TAG;
pub const SEQ_TWO_BYTE_STRING_TYPE: InstanceType =
    INTERNALIZED_TWO_BYTE_STRING_TYPE | K_NOT_INTERNALIZED_TAG;
pub const SEQ_ONE_BYTE_STRING_TYPE: InstanceType =
    INTERNALIZED_ONE_BYTE_STRING_TYPE | K_NOT_INTERNALIZED_TAG;
pub const CONS_TWO_BYTE_STRING_TYPE: InstanceType =
    K_TWO_BYTE_STRING_TAG | K_CONS_STRING_TAG | K_NOT_INTERNALIZED_TAG;
pub const CONS_ONE_BYTE_STRING_TYPE: InstanceType =
    K_ONE_BYTE_STRING_TAG | K_CONS_STRING_TAG | K_NOT_INTERNALIZED_TAG;
pub const SLICED_TWO_BYTE_STRING_TYPE: InstanceType =
    K_TWO_BYTE_STRING_TAG | K_SLICED_STRING_TAG | K_NOT_INTERNALIZED_TAG;
pub const SLICED_ONE_BYTE_STRING_TYPE: InstanceType =
    K_ONE_BYTE_STRING_TAG | K_SLICED_STRING_TAG | K_NOT_INTERNALIZED_TAG;
pub const THIN_TWO_BYTE_STRING_TYPE: InstanceType =
    K_TWO_BYTE_STRING_TAG | K_THIN_STRING_TAG | K_NOT_INTERNALIZED_TAG;
pub const THIN_ONE_BYTE_STRING_TYPE: InstanceType =
    K_ONE_BYTE_STRING_TAG | K_THIN_STRING_TAG | K_NOT_INTERNALIZED_TAG;

pub const LAST_STRING_TYPE: InstanceType = 127;

// -- Non-string types -------------------------------------------------------
//
// Torque assigns these in V8. The ordering here is what the range checks
// below rely on, so new types go inside the right run, never at the end:
//
//   SYMBOL_TYPE .. FIRST_JS_RECEIVER_TYPE-1   non-receivers
//   FIRST_JS_RECEIVER_TYPE ..                 everything JS code can hold a
//                                             reference to as an object
//   FIRST_JS_OBJECT_TYPE ..                   ordinary objects
//   FIRST_JS_FUNCTION_TYPE ..                 callables

pub const SYMBOL_TYPE: InstanceType = 128;
pub const FIRST_NONSTRING_TYPE: InstanceType = SYMBOL_TYPE;
pub const FIRST_UNIQUE_NAME_TYPE: InstanceType = INTERNALIZED_TWO_BYTE_STRING_TYPE;
pub const LAST_UNIQUE_NAME_TYPE: InstanceType = SYMBOL_TYPE;

pub const HEAP_NUMBER_TYPE: InstanceType = 129;
pub const ODDBALL_TYPE: InstanceType = 130;
pub const MAP_TYPE: InstanceType = 131;
pub const FIXED_ARRAY_TYPE: InstanceType = 132;
pub const PROPERTY_ARRAY_TYPE: InstanceType = 133;
pub const DESCRIPTOR_ARRAY_TYPE: InstanceType = 134;
pub const TRANSITION_ARRAY_TYPE: InstanceType = 135;
pub const FEEDBACK_VECTOR_TYPE: InstanceType = 136;
pub const SHARED_FUNCTION_INFO_TYPE: InstanceType = 137;
pub const BYTECODE_ARRAY_TYPE: InstanceType = 138;
pub const FILLER_TYPE: InstanceType = 139;

pub const JS_OBJECT_TYPE: InstanceType = 1058;
pub const FIRST_JS_RECEIVER_TYPE: InstanceType = JS_OBJECT_TYPE;
pub const FIRST_JS_OBJECT_TYPE: InstanceType = JS_OBJECT_TYPE;
pub const JS_GLOBAL_OBJECT_TYPE: InstanceType = 1059;
pub const JS_ARRAY_TYPE: InstanceType = 1060;
pub const JS_FUNCTION_TYPE: InstanceType = 1061;
pub const FIRST_JS_FUNCTION_TYPE: InstanceType = JS_FUNCTION_TYPE;
pub const JS_NATIVE_FUNCTION_TYPE: InstanceType = 1062;
pub const LAST_JS_FUNCTION_TYPE: InstanceType = JS_NATIVE_FUNCTION_TYPE;
pub const LAST_JS_OBJECT_TYPE: InstanceType = LAST_JS_FUNCTION_TYPE;
pub const LAST_JS_RECEIVER_TYPE: InstanceType = LAST_JS_OBJECT_TYPE;
pub const LAST_TYPE: InstanceType = LAST_JS_RECEIVER_TYPE;

// ---------------------------------------------------------------------------
// Predicates. Each one is the single mask-and-compare or range check that V8
// uses, not a match over every type.
// ---------------------------------------------------------------------------

#[inline]
pub const fn is_string(t: InstanceType) -> bool {
    t & K_IS_NOT_STRING_MASK == K_STRING_TAG
}

#[inline]
pub const fn is_internalized_string(t: InstanceType) -> bool {
    // One mask covers both questions, which is the point of putting the
    // "not internalized" bit directly below the non-string bits.
    t & (K_IS_NOT_STRING_MASK | K_IS_NOT_INTERNALIZED_MASK) == K_STRING_TAG | K_INTERNALIZED_TAG
}

#[inline]
pub const fn is_one_byte_string(t: InstanceType) -> bool {
    debug_assert!(is_string(t));
    t & K_STRING_ENCODING_MASK == K_ONE_BYTE_STRING_TAG
}

#[inline]
pub const fn is_seq_string(t: InstanceType) -> bool {
    is_string(t) && (t & K_STRING_REPRESENTATION_MASK == K_SEQ_STRING_TAG)
}

#[inline]
pub const fn is_cons_string(t: InstanceType) -> bool {
    is_string(t) && (t & K_STRING_REPRESENTATION_MASK == K_CONS_STRING_TAG)
}

#[inline]
pub const fn is_indirect_string(t: InstanceType) -> bool {
    debug_assert!(is_string(t));
    t & K_IS_INDIRECT_STRING_MASK == K_IS_INDIRECT_STRING_TAG
}

#[inline]
pub const fn is_unique_name(t: InstanceType) -> bool {
    is_internalized_string(t) || t == SYMBOL_TYPE
}

/// `IsName`: a string or a symbol -- the things that can be property keys.
#[inline]
pub const fn is_name(t: InstanceType) -> bool {
    is_string(t) || t == SYMBOL_TYPE
}

#[inline]
pub const fn is_js_receiver(t: InstanceType) -> bool {
    t >= FIRST_JS_RECEIVER_TYPE && t <= LAST_JS_RECEIVER_TYPE
}

#[inline]
pub const fn is_js_object(t: InstanceType) -> bool {
    t >= FIRST_JS_OBJECT_TYPE && t <= LAST_JS_OBJECT_TYPE
}

#[inline]
pub const fn is_callable(t: InstanceType) -> bool {
    t >= FIRST_JS_FUNCTION_TYPE && t <= LAST_JS_FUNCTION_TYPE
}

#[inline]
pub const fn is_js_array(t: InstanceType) -> bool {
    t == JS_ARRAY_TYPE
}

#[inline]
pub const fn is_heap_number(t: InstanceType) -> bool {
    t == HEAP_NUMBER_TYPE
}

#[inline]
pub const fn is_map(t: InstanceType) -> bool {
    t == MAP_TYPE
}

#[inline]
pub const fn is_oddball(t: InstanceType) -> bool {
    t == ODDBALL_TYPE
}

/// A human-readable name, for `--print-heap` and assertion messages.
pub fn instance_type_name(t: InstanceType) -> &'static str {
    match t {
        INTERNALIZED_TWO_BYTE_STRING_TYPE => "INTERNALIZED_TWO_BYTE_STRING",
        INTERNALIZED_ONE_BYTE_STRING_TYPE => "INTERNALIZED_ONE_BYTE_STRING",
        SEQ_TWO_BYTE_STRING_TYPE => "SEQ_TWO_BYTE_STRING",
        SEQ_ONE_BYTE_STRING_TYPE => "SEQ_ONE_BYTE_STRING",
        CONS_TWO_BYTE_STRING_TYPE => "CONS_TWO_BYTE_STRING",
        CONS_ONE_BYTE_STRING_TYPE => "CONS_ONE_BYTE_STRING",
        SLICED_TWO_BYTE_STRING_TYPE => "SLICED_TWO_BYTE_STRING",
        SLICED_ONE_BYTE_STRING_TYPE => "SLICED_ONE_BYTE_STRING",
        THIN_TWO_BYTE_STRING_TYPE => "THIN_TWO_BYTE_STRING",
        THIN_ONE_BYTE_STRING_TYPE => "THIN_ONE_BYTE_STRING",
        SYMBOL_TYPE => "SYMBOL",
        HEAP_NUMBER_TYPE => "HEAP_NUMBER",
        ODDBALL_TYPE => "ODDBALL",
        MAP_TYPE => "MAP",
        FIXED_ARRAY_TYPE => "FIXED_ARRAY",
        PROPERTY_ARRAY_TYPE => "PROPERTY_ARRAY",
        DESCRIPTOR_ARRAY_TYPE => "DESCRIPTOR_ARRAY",
        TRANSITION_ARRAY_TYPE => "TRANSITION_ARRAY",
        FEEDBACK_VECTOR_TYPE => "FEEDBACK_VECTOR",
        BYTECODE_ARRAY_TYPE => "BYTECODE_ARRAY",
        SHARED_FUNCTION_INFO_TYPE => "SHARED_FUNCTION_INFO",
        FILLER_TYPE => "FILLER",
        JS_OBJECT_TYPE => "JS_OBJECT",
        JS_GLOBAL_OBJECT_TYPE => "JS_GLOBAL_OBJECT",
        JS_ARRAY_TYPE => "JS_ARRAY",
        JS_FUNCTION_TYPE => "JS_FUNCTION",
        JS_NATIVE_FUNCTION_TYPE => "JS_NATIVE_FUNCTION",
        _ => "<unknown>",
    }
}

// The static assertions V8 makes about the encoding, kept as compile-time
// checks so a careless edit to the constants above cannot land.
const _: () = assert!(K_SEQ_STRING_TAG & K_IS_INDIRECT_STRING_MASK == 0);
const _: () = assert!(K_EXTERNAL_STRING_TAG & K_IS_INDIRECT_STRING_MASK == 0);
const _: () = assert!(K_CONS_STRING_TAG & K_IS_INDIRECT_STRING_MASK == K_IS_INDIRECT_STRING_TAG);
const _: () = assert!(K_SLICED_STRING_TAG & K_IS_INDIRECT_STRING_MASK == K_IS_INDIRECT_STRING_TAG);
const _: () = assert!(K_THIN_STRING_TAG & K_IS_INDIRECT_STRING_MASK == K_IS_INDIRECT_STRING_TAG);
const _: () = assert!(FIRST_NONSTRING_TYPE & K_IS_NOT_STRING_MASK != K_STRING_TAG);
const _: () =
    assert!(K_IS_NOT_STRING_MASK | K_IS_NOT_INTERNALIZED_MASK == !(K_IS_NOT_INTERNALIZED_MASK - 1));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_string_type_is_a_string_and_nothing_else_is() {
        let strings = [
            INTERNALIZED_TWO_BYTE_STRING_TYPE,
            INTERNALIZED_ONE_BYTE_STRING_TYPE,
            SEQ_TWO_BYTE_STRING_TYPE,
            SEQ_ONE_BYTE_STRING_TYPE,
            CONS_TWO_BYTE_STRING_TYPE,
            CONS_ONE_BYTE_STRING_TYPE,
            SLICED_TWO_BYTE_STRING_TYPE,
            SLICED_ONE_BYTE_STRING_TYPE,
            THIN_TWO_BYTE_STRING_TYPE,
            THIN_ONE_BYTE_STRING_TYPE,
        ];
        for t in strings {
            assert!(is_string(t), "{} should be a string", instance_type_name(t));
        }
        for t in [
            SYMBOL_TYPE,
            HEAP_NUMBER_TYPE,
            ODDBALL_TYPE,
            MAP_TYPE,
            FIXED_ARRAY_TYPE,
            JS_OBJECT_TYPE,
            JS_ARRAY_TYPE,
            JS_FUNCTION_TYPE,
        ] {
            assert!(
                !is_string(t),
                "{} should not be a string",
                instance_type_name(t)
            );
        }
    }

    #[test]
    fn internalized_bit() {
        assert!(is_internalized_string(INTERNALIZED_ONE_BYTE_STRING_TYPE));
        assert!(is_internalized_string(INTERNALIZED_TWO_BYTE_STRING_TYPE));
        assert!(!is_internalized_string(SEQ_ONE_BYTE_STRING_TYPE));
        assert!(!is_internalized_string(CONS_ONE_BYTE_STRING_TYPE));
        assert!(!is_internalized_string(JS_OBJECT_TYPE));
    }

    #[test]
    fn encoding_and_representation_bits() {
        assert!(is_one_byte_string(SEQ_ONE_BYTE_STRING_TYPE));
        assert!(!is_one_byte_string(SEQ_TWO_BYTE_STRING_TYPE));

        assert!(is_seq_string(SEQ_ONE_BYTE_STRING_TYPE));
        assert!(is_seq_string(INTERNALIZED_ONE_BYTE_STRING_TYPE));
        assert!(!is_seq_string(CONS_ONE_BYTE_STRING_TYPE));

        assert!(is_cons_string(CONS_TWO_BYTE_STRING_TYPE));
        assert!(!is_indirect_string(SEQ_ONE_BYTE_STRING_TYPE));
        assert!(is_indirect_string(CONS_ONE_BYTE_STRING_TYPE));
        assert!(is_indirect_string(SLICED_ONE_BYTE_STRING_TYPE));
        assert!(is_indirect_string(THIN_ONE_BYTE_STRING_TYPE));
    }

    #[test]
    fn the_known_bit_values_match_v8() {
        // Spot-check the actual numbers, so a refactor of the tag constants
        // cannot silently renumber the encoding.
        assert_eq!(INTERNALIZED_TWO_BYTE_STRING_TYPE, 0);
        assert_eq!(INTERNALIZED_ONE_BYTE_STRING_TYPE, 8);
        assert_eq!(SEQ_TWO_BYTE_STRING_TYPE, 64);
        assert_eq!(SEQ_ONE_BYTE_STRING_TYPE, 72);
        assert_eq!(CONS_TWO_BYTE_STRING_TYPE, 65);
        assert_eq!(CONS_ONE_BYTE_STRING_TYPE, 73);
        assert_eq!(THIN_ONE_BYTE_STRING_TYPE, 77);
    }

    #[test]
    fn receiver_ranges_are_range_checks() {
        assert!(is_js_receiver(JS_OBJECT_TYPE));
        assert!(is_js_receiver(JS_ARRAY_TYPE));
        assert!(is_js_receiver(JS_FUNCTION_TYPE));
        assert!(is_js_object(JS_FUNCTION_TYPE));
        assert!(!is_js_receiver(MAP_TYPE));
        assert!(!is_js_receiver(SEQ_ONE_BYTE_STRING_TYPE));

        assert!(is_callable(JS_FUNCTION_TYPE));
        assert!(is_callable(JS_NATIVE_FUNCTION_TYPE));
        assert!(!is_callable(JS_OBJECT_TYPE));
        assert!(!is_callable(JS_ARRAY_TYPE));
    }

    #[test]
    fn names_are_property_keys() {
        assert!(is_name(INTERNALIZED_ONE_BYTE_STRING_TYPE));
        assert!(is_name(SYMBOL_TYPE));
        assert!(!is_name(HEAP_NUMBER_TYPE));
        assert!(is_unique_name(INTERNALIZED_ONE_BYTE_STRING_TYPE));
        assert!(is_unique_name(SYMBOL_TYPE));
        assert!(!is_unique_name(SEQ_ONE_BYTE_STRING_TYPE));
    }
}
