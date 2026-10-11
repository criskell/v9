use crate::common::globals::K_TAGGED_SIZE;
use crate::heap::heap::Heap;
use crate::objects::property_details::{PropertyCellType, PropertyDetails};
use crate::objects::tagged::{Address, Tagged};

object_type!(
    /// `PropertyCell`
    PropertyCell
);

impl PropertyCell {
    pub const K_NAME_OFFSET: i32 = K_TAGGED_SIZE;
    pub const K_PROPERTY_DETAILS_RAW_OFFSET: i32 = Self::K_NAME_OFFSET + K_TAGGED_SIZE;
    pub const K_VALUE_OFFSET: i32 = Self::K_PROPERTY_DETAILS_RAW_OFFSET + K_TAGGED_SIZE;
    pub const K_DEPENDENT_CODE_OFFSET: i32 = Self::K_VALUE_OFFSET + K_TAGGED_SIZE;
    /// `PropertyCell::kSize`
    pub const K_SIZE: i32 = Self::K_DEPENDENT_CODE_OFFSET + K_TAGGED_SIZE;

    #[inline]
    pub fn name(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_NAME_OFFSET))
    }

    #[inline]
    pub fn set_name(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_NAME_OFFSET), value);
    }

    #[inline]
    pub fn name_address(self) -> Address {
        self.field(Self::K_NAME_OFFSET)
    }

    /// `PropertyCell::property_details`
    #[inline]
    pub fn property_details(self, heap: &Heap) -> PropertyDetails {
        PropertyDetails::from_smi(heap.read_tagged(self.field(Self::K_PROPERTY_DETAILS_RAW_OFFSET)))
    }

    #[inline]
    pub fn set_property_details(self, heap: &mut Heap, details: PropertyDetails) {
        heap.write_tagged(
            self.field(Self::K_PROPERTY_DETAILS_RAW_OFFSET),
            details.as_smi(),
        );
    }

    #[inline]
    pub fn value(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_VALUE_OFFSET))
    }

    #[inline]
    pub fn set_value(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_VALUE_OFFSET), value);
    }

    #[inline]
    pub fn value_address(self) -> Address {
        self.field(Self::K_VALUE_OFFSET)
    }

    #[inline]
    pub fn dependent_code(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_DEPENDENT_CODE_OFFSET))
    }

    #[inline]
    pub fn set_dependent_code(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_DEPENDENT_CODE_OFFSET), value);
    }

    #[inline]
    pub fn dependent_code_address(self) -> Address {
        self.field(Self::K_DEPENDENT_CODE_OFFSET)
    }

    #[inline]
    pub fn cell_type(self, heap: &Heap) -> PropertyCellType {
        self.property_details(heap).cell_type()
    }
}

pub fn initial_cell_type(value: Tagged, undefined: Tagged) -> PropertyCellType {
    if value == undefined {
        PropertyCellType::Undefined
    } else {
        PropertyCellType::Constant
    }
}

pub fn remains_constant_type(heap: &Heap, cell: PropertyCell, value: Tagged) -> bool {
    let current = cell.value(heap);
    if current.is_smi() && value.is_smi() {
        return true;
    }
    match (
        current.to_strong_heap_object(),
        value.to_strong_heap_object(),
    ) {
        (Some(current), Some(value)) => {
            let current_map = heap.map(current);
            let value_map = heap.map(value);
            current_map == value_map
                && !crate::objects::map::Map::unchecked_cast(value_map).is_unstable(heap)
        }
        _ => false,
    }
}

pub fn updated_cell_type(heap: &Heap, cell: PropertyCell, value: Tagged) -> PropertyCellType {
    match cell.cell_type(heap) {
        PropertyCellType::Undefined => PropertyCellType::Constant,
        PropertyCellType::Constant => {
            if value == cell.value(heap) {
                return PropertyCellType::Constant;
            }
            if remains_constant_type(heap, cell, value) {
                PropertyCellType::ConstantType
            } else {
                PropertyCellType::Mutable
            }
        }
        PropertyCellType::ConstantType => {
            if remains_constant_type(heap, cell, value) {
                PropertyCellType::ConstantType
            } else {
                PropertyCellType::Mutable
            }
        }
        // Terminal.
        PropertyCellType::Mutable => PropertyCellType::Mutable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout() {
        assert_eq!(PropertyCell::K_NAME_OFFSET, 8);
        assert_eq!(PropertyCell::K_PROPERTY_DETAILS_RAW_OFFSET, 16);
        assert_eq!(PropertyCell::K_VALUE_OFFSET, 24);
        assert_eq!(PropertyCell::K_DEPENDENT_CODE_OFFSET, 32);
        assert_eq!(PropertyCell::K_SIZE, 40);
    }

    #[test]
    fn a_cell_holding_undefined_has_not_really_been_assigned() {
        let undefined = Tagged(0x11);
        assert_eq!(
            initial_cell_type(undefined, undefined),
            PropertyCellType::Undefined
        );
        assert_eq!(
            initial_cell_type(crate::objects::smi::Smi::from_int(1), undefined),
            PropertyCellType::Constant
        );
    }
}
