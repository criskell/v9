/// `ElementsKind`
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
#[repr(u8)]
pub enum ElementsKind {
    // The "fast" kind for elements that only contain SMI values. Must be
    // first to make it possible to efficiently check maps for this kind.
    PackedSmiElements = 0,
    HoleySmiElements = 1,

    // The "fast" kind for tagged values. Must be second to make it possible
    // to efficiently check maps for this and the PACKED_SMI_ELEMENTS kind
    // together at once.
    PackedElements = 2,
    HoleyElements = 3,

    // The "fast" kind for unwrapped, non-tagged double values.
    PackedDoubleElements = 4,
    HoleyDoubleElements = 5,

    // The "slow" kind.
    DictionaryElements = 6,

    // Sentinel ElementsKind for objects with no elements.
    NoElements = 7,
}

use ElementsKind::*;

// Derived constants from ElementsKind.
pub const FIRST_ELEMENTS_KIND: ElementsKind = PackedSmiElements;
pub const FIRST_FAST_ELEMENTS_KIND: ElementsKind = PackedSmiElements;
pub const LAST_FAST_ELEMENTS_KIND: ElementsKind = HoleyDoubleElements;
pub const TERMINAL_FAST_ELEMENTS_KIND: ElementsKind = HoleyElements;

/// `kFastElementsKindCount`
pub const FAST_ELEMENTS_KIND_COUNT: usize = 6;

/// `GetInitialFastElementsKind`
#[inline]
pub fn get_initial_fast_elements_kind() -> ElementsKind {
    PackedSmiElements
}

/// `IsDictionaryElementsKind`
#[inline]
pub fn is_dictionary_elements_kind(kind: ElementsKind) -> bool {
    kind == DictionaryElements
}

/// `IsTerminalElementsKind`
#[inline]
pub fn is_terminal_elements_kind(kind: ElementsKind) -> bool {
    kind == TERMINAL_FAST_ELEMENTS_KIND
}

/// `IsFastElementsKind`
#[inline]
pub fn is_fast_elements_kind(kind: ElementsKind) -> bool {
    debug_assert_eq!(FIRST_FAST_ELEMENTS_KIND as u8, 0);
    kind as u8 <= LAST_FAST_ELEMENTS_KIND as u8
}

/// `IsSmiElementsKind`
#[inline]
pub fn is_smi_elements_kind(kind: ElementsKind) -> bool {
    matches!(kind, PackedSmiElements | HoleySmiElements)
}

/// `IsObjectElementsKind`
#[inline]
pub fn is_object_elements_kind(kind: ElementsKind) -> bool {
    matches!(kind, PackedElements | HoleyElements)
}

/// `IsDoubleElementsKind`
#[inline]
pub fn is_double_elements_kind(kind: ElementsKind) -> bool {
    matches!(kind, PackedDoubleElements | HoleyDoubleElements)
}

/// `IsSmiOrObjectElementsKind`
#[inline]
pub fn is_smi_or_object_elements_kind(kind: ElementsKind) -> bool {
    (PackedSmiElements as u8..=HoleyElements as u8).contains(&(kind as u8))
}

/// `IsHoleyElementsKindForRead`
#[inline]
pub fn is_holey_elements_kind_for_read(kind: ElementsKind) -> bool {
    kind as u8 % 2 == 1 && kind as u8 <= HoleyDoubleElements as u8
}

/// `IsHoleyElementsKind`
#[inline]
pub fn is_holey_elements_kind(kind: ElementsKind) -> bool {
    is_holey_elements_kind_for_read(kind)
}

/// `IsHoleyOrDictionaryElementsKind`
#[inline]
pub fn is_holey_or_dictionary_elements_kind(kind: ElementsKind) -> bool {
    is_holey_elements_kind_for_read(kind) || kind == DictionaryElements
}

/// `IsFastPackedElementsKind`
#[inline]
pub fn is_fast_packed_elements_kind(kind: ElementsKind) -> bool {
    kind as u8 % 2 == 0 && kind as u8 <= PackedDoubleElements as u8
}

/// `GetPackedElementsKind`
#[inline]
pub fn get_packed_elements_kind(holey_kind: ElementsKind) -> ElementsKind {
    match holey_kind {
        HoleySmiElements => PackedSmiElements,
        HoleyDoubleElements => PackedDoubleElements,
        HoleyElements => PackedElements,
        other => other,
    }
}

/// `GetHoleyElementsKind`
#[inline]
pub fn get_holey_elements_kind(packed_kind: ElementsKind) -> ElementsKind {
    match packed_kind {
        PackedSmiElements => HoleySmiElements,
        PackedDoubleElements => HoleyDoubleElements,
        PackedElements => HoleyElements,
        other => other,
    }
}

/// `FastSmiToObjectElementsKind`
#[inline]
pub fn fast_smi_to_object_elements_kind(from_kind: ElementsKind) -> ElementsKind {
    debug_assert!(is_smi_elements_kind(from_kind));
    if from_kind == PackedSmiElements {
        PackedElements
    } else {
        HoleyElements
    }
}

/// `IsSimpleMapChangeTransition`
#[inline]
pub fn is_simple_map_change_transition(from_kind: ElementsKind, to_kind: ElementsKind) -> bool {
    get_holey_elements_kind(from_kind) == to_kind
        || (is_smi_elements_kind(from_kind) && is_object_elements_kind(to_kind))
}

/// `IsMoreGeneralElementsKindTransition`
pub fn is_more_general_elements_kind_transition(
    from_kind: ElementsKind,
    to_kind: ElementsKind,
) -> bool {
    if !is_fast_elements_kind(from_kind) {
        return false;
    }
    if !is_fast_elements_kind(to_kind) {
        return false;
    }
    match from_kind {
        PackedSmiElements => to_kind != PackedSmiElements,
        HoleySmiElements => to_kind != PackedSmiElements && to_kind != HoleySmiElements,
        PackedDoubleElements => {
            to_kind != PackedSmiElements
                && to_kind != HoleySmiElements
                && to_kind != PackedDoubleElements
        }
        HoleyDoubleElements => to_kind == PackedElements || to_kind == HoleyElements,
        PackedElements => to_kind == HoleyElements,
        HoleyElements => false,
        _ => false,
    }
}

/// `GetMoreGeneralElementsKind`
#[inline]
pub fn get_more_general_elements_kind(
    from_kind: ElementsKind,
    to_kind: ElementsKind,
) -> ElementsKind {
    if is_more_general_elements_kind_transition(from_kind, to_kind) {
        to_kind
    } else {
        from_kind
    }
}

/// `IsTransitionableFastElementsKind`
#[inline]
pub fn is_transitionable_fast_elements_kind(from_kind: ElementsKind) -> bool {
    is_fast_elements_kind(from_kind) && from_kind != TERMINAL_FAST_ELEMENTS_KIND
}

/// `kFastElementsKindSequence`
pub const FAST_ELEMENTS_KIND_SEQUENCE: [ElementsKind; FAST_ELEMENTS_KIND_COUNT] = [
    PackedSmiElements,
    HoleySmiElements,
    PackedDoubleElements,
    HoleyDoubleElements,
    PackedElements,
    HoleyElements,
];

/// `GetSequenceIndexFromFastElementsKind`
pub fn get_sequence_index_from_fast_elements_kind(kind: ElementsKind) -> usize {
    FAST_ELEMENTS_KIND_SEQUENCE
        .iter()
        .position(|candidate| *candidate == kind)
        .expect("a fast elements kind")
}

/// `GetFastElementsKindFromSequenceIndex`
pub fn get_fast_elements_kind_from_sequence_index(index: usize) -> ElementsKind {
    FAST_ELEMENTS_KIND_SEQUENCE[index]
}

/// `GetNextTransitionElementsKind`
pub fn get_next_transition_elements_kind(kind: ElementsKind) -> ElementsKind {
    let index = get_sequence_index_from_fast_elements_kind(kind);
    debug_assert!(index + 1 < FAST_ELEMENTS_KIND_COUNT);
    get_fast_elements_kind_from_sequence_index(index + 1)
}

/// `Object::OptimalElementsKind`
pub fn optimal_elements_kind(
    heap: &crate::heap::heap::Heap,
    value: crate::objects::tagged::Tagged,
) -> ElementsKind {
    if value.is_smi() {
        return PackedSmiElements;
    }
    if crate::objects::heap_number::is_number(heap, value) {
        return PackedDoubleElements;
    }
    PackedElements
}

impl ElementsKind {
    /// `ElementsKindToString`
    pub fn to_string(self) -> &'static str {
        match self {
            PackedSmiElements => "PACKED_SMI_ELEMENTS",
            HoleySmiElements => "HOLEY_SMI_ELEMENTS",
            PackedElements => "PACKED_ELEMENTS",
            HoleyElements => "HOLEY_ELEMENTS",
            PackedDoubleElements => "PACKED_DOUBLE_ELEMENTS",
            HoleyDoubleElements => "HOLEY_DOUBLE_ELEMENTS",
            DictionaryElements => "DICTIONARY_ELEMENTS",
            NoElements => "NO_ELEMENTS",
        }
    }

    pub fn from_bits(bits: u8) -> ElementsKind {
        match bits {
            0 => PackedSmiElements,
            1 => HoleySmiElements,
            2 => PackedElements,
            3 => HoleyElements,
            4 => PackedDoubleElements,
            5 => HoleyDoubleElements,
            6 => DictionaryElements,
            7 => NoElements,
            other => panic!("not an elements kind: {other}"),
        }
    }
}

impl std::fmt::Display for ElementsKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(ElementsKind::to_string(*self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fast_kinds_keep_v8s_numbering() {
        assert_eq!(PackedSmiElements as u8, 0);
        assert_eq!(HoleySmiElements as u8, 1);
        assert_eq!(PackedElements as u8, 2);
        assert_eq!(HoleyElements as u8, 3);
        assert_eq!(PackedDoubleElements as u8, 4);
        assert_eq!(HoleyDoubleElements as u8, 5);
        assert!(is_smi_or_object_elements_kind(PackedSmiElements));
        assert!(is_smi_or_object_elements_kind(HoleyElements));
        assert!(!is_smi_or_object_elements_kind(PackedDoubleElements));
    }

    #[test]
    fn generalisation_is_antisymmetric() {
        for &from in &FAST_ELEMENTS_KIND_SEQUENCE {
            assert!(!is_more_general_elements_kind_transition(from, from));
            for &to in &FAST_ELEMENTS_KIND_SEQUENCE {
                if from == to {
                    continue;
                }
                assert!(
                    !(is_more_general_elements_kind_transition(from, to)
                        && is_more_general_elements_kind_transition(to, from)),
                    "{from} and {to} cannot both be more general"
                );
            }
        }
    }

    #[test]
    fn every_kind_reaches_the_terminal_kind() {
        for &from in &FAST_ELEMENTS_KIND_SEQUENCE {
            assert!(
                is_more_general_elements_kind_transition(from, HoleyElements)
                    || from == HoleyElements,
                "{from} should reach HOLEY_ELEMENTS"
            );
        }
        assert!(is_terminal_elements_kind(HoleyElements));
        assert!(!is_transitionable_fast_elements_kind(HoleyElements));
    }

    #[test]
    fn a_double_kind_does_not_generalise_back_to_smi() {
        assert_eq!(
            get_more_general_elements_kind(PackedDoubleElements, PackedSmiElements),
            PackedDoubleElements
        );
        assert_eq!(
            get_more_general_elements_kind(PackedSmiElements, PackedDoubleElements),
            PackedDoubleElements
        );
    }

    #[test]
    fn simple_map_change_transitions_need_no_new_backing_store() {
        assert!(is_simple_map_change_transition(
            PackedSmiElements,
            HoleySmiElements
        ));
        assert!(is_simple_map_change_transition(
            PackedSmiElements,
            PackedElements
        ));
        assert!(!is_simple_map_change_transition(
            PackedSmiElements,
            PackedDoubleElements
        ));
        assert!(!is_simple_map_change_transition(
            PackedDoubleElements,
            PackedElements
        ));
    }

    #[test]
    fn the_sequence_is_generalisation_order_not_numeric_order() {
        assert_eq!(
            get_next_transition_elements_kind(PackedSmiElements),
            HoleySmiElements
        );
        assert_eq!(
            get_next_transition_elements_kind(HoleySmiElements),
            PackedDoubleElements
        );
        assert_eq!(
            get_next_transition_elements_kind(PackedElements),
            HoleyElements
        );
    }
}
