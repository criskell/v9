use crate::objects::smi::Smi;
use crate::objects::tagged::Tagged;

pub type PropertyAttributes = u32;

pub const NONE: PropertyAttributes = 0;
pub const READ_ONLY: PropertyAttributes = 1 << 0;
pub const DONT_ENUM: PropertyAttributes = 1 << 1;
pub const DONT_DELETE: PropertyAttributes = 1 << 2;

/// `PropertyKind`
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PropertyKind {
    Data = 0,
    Accessor = 1,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PropertyLocation {
    Field = 0,
    Descriptor = 1,
}

/// `PropertyConstness`
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PropertyConstness {
    Mutable = 0,
    Const = 1,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PropertyCellType {
    Mutable = 0,      // Cell will no longer be tracked as constant.
    Undefined = 1,    // The PREMONOMORPHIC of property cells.
    Constant = 2,     // Cell has been assigned only once.
    ConstantType = 3, // Cell has been assigned only one type.
}

impl PropertyCellType {
    // Value for dictionaries not holding cells, must be 0:
    pub const NO_CELL: PropertyCellType = PropertyCellType::Mutable;
}

impl PropertyCellType {
    pub fn from_u32(value: u32) -> PropertyCellType {
        match value {
            1 => PropertyCellType::Undefined,
            2 => PropertyCellType::Constant,
            3 => PropertyCellType::ConstantType,
            _ => PropertyCellType::Mutable,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Representation {
    None = 0,
    Smi = 1,
    Double = 2,
    HeapObject = 3,
    Tagged = 4,
}

impl Representation {
    /// `Representation::IsMoreGeneralThan`
    pub fn is_more_general_than(self, other: Representation) -> bool {
        match (self, other) {
            (a, b) if a == b => false,
            (Representation::None, _) => false,
            (_, Representation::None) => true,
            (Representation::Tagged, _) => true,
            _ => false,
        }
    }

    pub fn generalize(self, other: Representation) -> Representation {
        if self == other {
            return self;
        }
        if self == Representation::None {
            return other;
        }
        if other == Representation::None {
            return self;
        }
        Representation::Tagged
    }

    pub fn for_value(value: Tagged) -> Representation {
        if value.is_smi() {
            Representation::Smi
        } else {
            Representation::HeapObject
        }
    }
}

const KIND_SHIFT: u32 = 0;
const KIND_BITS: u32 = 1;
const CONSTNESS_SHIFT: u32 = KIND_SHIFT + KIND_BITS;
const CONSTNESS_BITS: u32 = 1;
const ATTRIBUTES_SHIFT: u32 = CONSTNESS_SHIFT + CONSTNESS_BITS;
const ATTRIBUTES_BITS: u32 = 3;
const LOCATION_SHIFT: u32 = ATTRIBUTES_SHIFT + ATTRIBUTES_BITS;
const LOCATION_BITS: u32 = 1;
const REPRESENTATION_SHIFT: u32 = LOCATION_SHIFT + LOCATION_BITS;
const REPRESENTATION_BITS: u32 = 3;
const DESCRIPTOR_POINTER_SHIFT: u32 = REPRESENTATION_SHIFT + REPRESENTATION_BITS;
/// `kDescriptorIndexBitCount`
pub const K_DESCRIPTOR_INDEX_BIT_COUNT: u32 = 10;
const FIELD_INDEX_SHIFT: u32 = DESCRIPTOR_POINTER_SHIFT + K_DESCRIPTOR_INDEX_BIT_COUNT;
const FIELD_INDEX_BITS: u32 = K_DESCRIPTOR_INDEX_BIT_COUNT + 1;

const CELL_TYPE_SHIFT: u32 = ATTRIBUTES_SHIFT + ATTRIBUTES_BITS;
const CELL_TYPE_BITS: u32 = 3;
const DICTIONARY_STORAGE_SHIFT: u32 = CELL_TYPE_SHIFT + CELL_TYPE_BITS;
const DICTIONARY_STORAGE_BITS: u32 = 23;

/// `kMaxNumberOfDescriptors`
pub const K_MAX_NUMBER_OF_DESCRIPTORS: u32 = (1 << K_DESCRIPTOR_INDEX_BIT_COUNT) - 4;

#[inline]
const fn mask(bits: u32) -> u32 {
    (1u32 << bits) - 1
}

#[inline]
const fn encode(value: u32, shift: u32, bits: u32) -> u32 {
    debug_assert!(value <= mask(bits));
    (value & mask(bits)) << shift
}

#[inline]
const fn decode(word: u32, shift: u32, bits: u32) -> u32 {
    (word >> shift) & mask(bits)
}

/// `PropertyDetails`
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct PropertyDetails(u32);

impl PropertyDetails {
    pub fn new_field(
        kind: PropertyKind,
        attributes: PropertyAttributes,
        constness: PropertyConstness,
        representation: Representation,
        field_index: u32,
    ) -> PropertyDetails {
        debug_assert!(field_index <= mask(FIELD_INDEX_BITS));
        PropertyDetails(
            encode(kind as u32, KIND_SHIFT, KIND_BITS)
                | encode(constness as u32, CONSTNESS_SHIFT, CONSTNESS_BITS)
                | encode(attributes, ATTRIBUTES_SHIFT, ATTRIBUTES_BITS)
                | encode(
                    PropertyLocation::Field as u32,
                    LOCATION_SHIFT,
                    LOCATION_BITS,
                )
                | encode(
                    representation as u32,
                    REPRESENTATION_SHIFT,
                    REPRESENTATION_BITS,
                )
                | encode(field_index, FIELD_INDEX_SHIFT, FIELD_INDEX_BITS),
        )
    }

    // Property details for dictionary mode properties/elements.
    pub fn new_dictionary(
        kind: PropertyKind,
        attributes: PropertyAttributes,
        constness: PropertyConstness,
        dictionary_index: u32,
    ) -> PropertyDetails {
        PropertyDetails(
            encode(kind as u32, KIND_SHIFT, KIND_BITS)
                | encode(
                    PropertyLocation::Field as u32,
                    LOCATION_SHIFT,
                    LOCATION_BITS,
                )
                | encode(attributes, ATTRIBUTES_SHIFT, ATTRIBUTES_BITS)
                | encode(constness as u32, CONSTNESS_SHIFT, CONSTNESS_BITS)
                | encode(
                    dictionary_index,
                    DICTIONARY_STORAGE_SHIFT,
                    DICTIONARY_STORAGE_BITS,
                )
                | encode(
                    PropertyCellType::NO_CELL as u32,
                    CELL_TYPE_SHIFT,
                    CELL_TYPE_BITS,
                ),
        )
    }

    #[inline]
    pub const fn from_raw(value: u32) -> PropertyDetails {
        PropertyDetails(value)
    }

    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// `PropertyDetails::AsSmi`
    #[inline]
    pub fn as_smi(self) -> Tagged {
        Smi::from_int(self.0 as i32)
    }

    #[inline]
    pub fn from_smi(value: Tagged) -> PropertyDetails {
        PropertyDetails(Smi::to_int(value) as u32)
    }

    #[inline]
    pub fn kind(self) -> PropertyKind {
        match decode(self.0, KIND_SHIFT, KIND_BITS) {
            0 => PropertyKind::Data,
            _ => PropertyKind::Accessor,
        }
    }

    #[inline]
    pub fn constness(self) -> PropertyConstness {
        match decode(self.0, CONSTNESS_SHIFT, CONSTNESS_BITS) {
            0 => PropertyConstness::Mutable,
            _ => PropertyConstness::Const,
        }
    }

    #[inline]
    pub fn attributes(self) -> PropertyAttributes {
        decode(self.0, ATTRIBUTES_SHIFT, ATTRIBUTES_BITS)
    }

    #[inline]
    pub fn location(self) -> PropertyLocation {
        match decode(self.0, LOCATION_SHIFT, LOCATION_BITS) {
            0 => PropertyLocation::Field,
            _ => PropertyLocation::Descriptor,
        }
    }

    #[inline]
    pub fn representation(self) -> Representation {
        match decode(self.0, REPRESENTATION_SHIFT, REPRESENTATION_BITS) {
            0 => Representation::None,
            1 => Representation::Smi,
            2 => Representation::Double,
            3 => Representation::HeapObject,
            _ => Representation::Tagged,
        }
    }

    #[inline]
    pub fn field_index(self) -> u32 {
        decode(self.0, FIELD_INDEX_SHIFT, FIELD_INDEX_BITS)
    }

    #[inline]
    pub fn is_read_only(self) -> bool {
        self.attributes() & READ_ONLY != 0
    }

    #[inline]
    pub fn is_enumerable(self) -> bool {
        self.attributes() & DONT_ENUM == 0
    }

    #[inline]
    pub fn is_configurable(self) -> bool {
        self.attributes() & DONT_DELETE == 0
    }

    /// `PropertyDetails::CopyWithRepresentation`
    pub fn with_representation(self, representation: Representation) -> PropertyDetails {
        let cleared = self.0 & !(mask(REPRESENTATION_BITS) << REPRESENTATION_SHIFT);
        PropertyDetails(
            cleared
                | encode(
                    representation as u32,
                    REPRESENTATION_SHIFT,
                    REPRESENTATION_BITS,
                ),
        )
    }

    pub fn new_cell(
        kind: PropertyKind,
        attributes: PropertyAttributes,
        cell_type: PropertyCellType,
    ) -> PropertyDetails {
        PropertyDetails(
            encode(kind as u32, KIND_SHIFT, KIND_BITS)
                | encode(attributes, ATTRIBUTES_SHIFT, ATTRIBUTES_BITS)
                | encode(cell_type as u32, CELL_TYPE_SHIFT, CELL_TYPE_BITS),
        )
    }

    #[inline]
    pub fn cell_type(self) -> PropertyCellType {
        PropertyCellType::from_u32(decode(self.0, CELL_TYPE_SHIFT, CELL_TYPE_BITS))
    }

    pub fn with_cell_type(self, cell_type: PropertyCellType) -> PropertyDetails {
        let cleared = self.0 & !(mask(CELL_TYPE_BITS) << CELL_TYPE_SHIFT);
        PropertyDetails(cleared | encode(cell_type as u32, CELL_TYPE_SHIFT, CELL_TYPE_BITS))
    }

    /// `PropertyDetails::dictionary_index`
    #[inline]
    pub fn dictionary_index(self) -> u32 {
        decode(self.0, DICTIONARY_STORAGE_SHIFT, DICTIONARY_STORAGE_BITS)
    }

    /// `PropertyDetails::set_index`
    pub fn set_dictionary_index(self, index: u32) -> PropertyDetails {
        let cleared = self.0 & !(mask(DICTIONARY_STORAGE_BITS) << DICTIONARY_STORAGE_SHIFT);
        PropertyDetails(cleared | encode(index, DICTIONARY_STORAGE_SHIFT, DICTIONARY_STORAGE_BITS))
    }

    pub fn with_constness(self, constness: PropertyConstness) -> PropertyDetails {
        let cleared = self.0 & !(mask(CONSTNESS_BITS) << CONSTNESS_SHIFT);
        PropertyDetails(cleared | encode(constness as u32, CONSTNESS_SHIFT, CONSTNESS_BITS))
    }
}

/// `PropertyDetails::kInitialIndex`
pub const K_FIRST_INDEX: i32 = 1;

impl std::fmt::Debug for PropertyDetails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PropertyDetails {{ {:?} {:?} {:?} {:?} index: {}, attrs: {}{}{} }}",
            self.kind(),
            self.location(),
            self.constness(),
            self.representation(),
            self.field_index(),
            if self.is_read_only() { "R" } else { "-" },
            if self.is_enumerable() { "-" } else { "E" },
            if self.is_configurable() { "-" } else { "D" },
        )
    }
}

pub fn default_data_field(field_index: u32, representation: Representation) -> PropertyDetails {
    data_field_with_attributes(field_index, representation, NONE)
}

pub fn data_field_with_attributes(
    field_index: u32,
    representation: Representation,
    attributes: PropertyAttributes,
) -> PropertyDetails {
    PropertyDetails::new_field(
        PropertyKind::Data,
        attributes,
        PropertyConstness::Mutable,
        representation,
        field_index,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitfields_do_not_overlap() {
        let d = PropertyDetails::new_field(
            PropertyKind::Accessor,
            READ_ONLY | DONT_ENUM | DONT_DELETE,
            PropertyConstness::Const,
            Representation::Tagged,
            mask(FIELD_INDEX_BITS),
        );
        assert_eq!(d.kind(), PropertyKind::Accessor);
        assert_eq!(d.constness(), PropertyConstness::Const);
        assert_eq!(d.attributes(), READ_ONLY | DONT_ENUM | DONT_DELETE);
        assert_eq!(d.location(), PropertyLocation::Field);
        assert_eq!(d.representation(), Representation::Tagged);
        assert_eq!(d.field_index(), mask(FIELD_INDEX_BITS));
    }

    #[test]
    fn field_index_survives_a_trip_through_a_smi() {
        for index in [0u32, 1, 3, 255, 1023] {
            let d = default_data_field(index, Representation::Smi);
            let round_tripped = PropertyDetails::from_smi(d.as_smi());
            assert_eq!(round_tripped.field_index(), index);
            assert_eq!(round_tripped.representation(), Representation::Smi);
            assert_eq!(round_tripped.kind(), PropertyKind::Data);
        }
    }

    #[test]
    fn a_plain_assignment_produces_a_writable_enumerable_field() {
        let d = default_data_field(0, Representation::Smi);
        assert!(!d.is_read_only());
        assert!(d.is_enumerable());
        assert!(d.is_configurable());
        assert_eq!(d.location(), PropertyLocation::Field);
        assert_eq!(d.constness(), PropertyConstness::Mutable);
    }

    #[test]
    fn attributes_are_independent() {
        let ro = PropertyDetails::new_field(
            PropertyKind::Data,
            READ_ONLY,
            PropertyConstness::Mutable,
            Representation::Tagged,
            7,
        );
        assert!(ro.is_read_only());
        assert!(ro.is_enumerable());
        assert!(ro.is_configurable());
        assert_eq!(ro.field_index(), 7);
    }

    #[test]
    fn widening_a_representation_only_changes_that_field() {
        let d = default_data_field(5, Representation::Smi);
        let widened = d.with_representation(Representation::Tagged);
        assert_eq!(widened.representation(), Representation::Tagged);
        assert_eq!(widened.field_index(), 5, "index must survive widening");
        assert_eq!(widened.attributes(), d.attributes());
        assert_eq!(widened.kind(), d.kind());
    }

    #[test]
    fn generalizing_representations() {
        use Representation::*;
        assert_eq!(Smi.generalize(Smi), Smi);
        assert_eq!(None.generalize(Smi), Smi);
        assert_eq!(Smi.generalize(None), Smi);
        assert_eq!(Smi.generalize(HeapObject), Tagged);
        assert_eq!(Double.generalize(Smi), Tagged);
        assert!(Tagged.is_more_general_than(Smi));
        assert!(!Smi.is_more_general_than(Tagged));
        assert!(!Smi.is_more_general_than(Smi));
    }

    #[test]
    fn the_cell_layout_overlays_the_fast_layout() {
        let cell = PropertyDetails::new_cell(PropertyKind::Data, NONE, PropertyCellType::Constant);
        assert_eq!(cell.cell_type(), PropertyCellType::Constant);
        assert_eq!(cell.kind(), PropertyKind::Data);
        assert_eq!(cell.attributes(), NONE);

        let fast = default_data_field(0, Representation::Smi);
        assert_ne!(
            fast.cell_type(),
            PropertyCellType::Mutable,
            "reading a fast property's details as a cell's is meaningless, \
             which is exactly why the two are never mixed"
        );
    }

    #[test]
    fn the_cell_type_lattice_round_trips() {
        for cell_type in [
            PropertyCellType::Mutable,
            PropertyCellType::Undefined,
            PropertyCellType::Constant,
            PropertyCellType::ConstantType,
        ] {
            let details = PropertyDetails::new_cell(PropertyKind::Data, READ_ONLY, cell_type);
            assert_eq!(details.cell_type(), cell_type);
            assert!(details.is_read_only(), "the attributes are untouched");
            let updated = details.with_cell_type(PropertyCellType::Mutable);
            assert_eq!(updated.cell_type(), PropertyCellType::Mutable);
            assert!(updated.is_read_only());
        }
    }

    #[test]
    fn representation_of_a_stored_value() {
        assert_eq!(
            Representation::for_value(Smi::from_int(1)),
            Representation::Smi
        );
        assert_eq!(
            Representation::for_value(Tagged(0x1001)),
            Representation::HeapObject
        );
    }
}
