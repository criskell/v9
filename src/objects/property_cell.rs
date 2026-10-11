//! Ported from `src/objects/property-cell.h` and `property-cell.tq`.
//!
//! A box holding one global's value.
//!
//! ```text
//!   0  map
//!   8  name                  Name
//!  16  property_details_raw  Smi
//!  24  value                 Object
//!  32  dependent_code        DependentCode
//!  40  == PropertyCell::kSize
//! ```
//!
//! Why a box and not just a property
//! ---------------------------------
//! A global access cannot be cached the way `o.x` is. An ordinary inline
//! cache keys on the receiver's map, but *declaring another global* adds a
//! property to the global object and so changes its map -- which would
//! invalidate every cached global access in the program, including in code
//! that has nothing to do with the new name.
//!
//! So the storage is indirected: the global object's property holds a cell,
//! and a `LoadGlobalIC` caches a weak reference to **the cell itself**. The
//! load becomes "read this cell", with no map check at all, and it survives
//! any number of later declarations.
//!
//! The second job is optimisation. [`PropertyCellType`] records how much is
//! known about the value: a global assigned exactly once is `Constant`, and
//! an optimizing compiler may inline it, registering itself in
//! `dependent_code` so that a later assignment can throw the code away.
//!
//! Deviation, tracked in `PORTING.md`: `dependent_code` is present for
//! layout fidelity but always empty -- there is no optimized code to
//! invalidate. V8 also keeps the cells in a `GlobalDictionary` on
//! `JSGlobalObject`, so adding a global does not reshape it at all; `vx`
//! stores each cell as an ordinary fast property of the global object, which
//! does reshape it. That costs nothing here *because* the caches hold cells
//! rather than maps -- which is the point being demonstrated.

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

/// `PropertyCell::InitialType`: a cell created holding `undefined` has never
/// really been assigned, so it starts below `Constant`.
pub fn initial_cell_type(value: Tagged, undefined: Tagged) -> PropertyCellType {
    if value == undefined {
        PropertyCellType::Undefined
    } else {
        PropertyCellType::Constant
    }
}

/// `RemainsConstantType`: whether `value` is the same *kind* of value the
/// cell already holds, so the cell can stay `ConstantType` instead of going
/// fully mutable.
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
            // Same map, and the map has not been generalised under us.
            let current_map = heap.map(current);
            let value_map = heap.map(value);
            current_map == value_map
                && !crate::objects::map::Map::unchecked_cast(value_map).is_unstable(heap)
        }
        _ => false,
    }
}

/// `PropertyCell::UpdatedType`: the one-way walk down the lattice.
pub fn updated_cell_type(heap: &Heap, cell: PropertyCell, value: Tagged) -> PropertyCellType {
    match cell.cell_type(heap) {
        // The first real assignment makes it a constant.
        PropertyCellType::Undefined => PropertyCellType::Constant,
        PropertyCellType::Constant => {
            if value == cell.value(heap) {
                // Assigned the same value again: still a constant.
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
        // Which matters: `var x;` must not make `x` a constant undefined
        // that an optimizer could inline.
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
