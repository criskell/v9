//! Ported from `src/objects/heap-number.h` and `heap-number.tq`.
//!
//! A boxed double: every JavaScript number that is not a Smi.
//!
//! ```text
//!   0  map
//!   8  value   float64
//!  16  == HeapNumber::kSize
//! ```

use crate::heap::heap::Heap;
use crate::objects::instance_type;
use crate::objects::smi::Smi;
use crate::objects::tagged::Tagged;

object_type!(
    /// `HeapNumber`
    HeapNumber
);

impl HeapNumber {
    pub const K_VALUE_OFFSET: i32 = 8;
    /// `HeapNumber::kSize`
    pub const K_SIZE: i32 = Self::K_VALUE_OFFSET + 8;

    #[inline]
    pub fn value(self, heap: &Heap) -> f64 {
        heap.read_f64(self.field(Self::K_VALUE_OFFSET))
    }

    #[inline]
    pub fn set_value(self, heap: &mut Heap, value: f64) {
        heap.write_f64(self.field(Self::K_VALUE_OFFSET), value);
    }
}

/// `Object::NumberValue`: the numeric value of a Smi or a HeapNumber.
///
/// `None` for anything else -- conversion of non-numbers is
/// `Object::ToNumber`'s job, which can call user code and therefore cannot
/// live here.
pub fn number_value(heap: &Heap, value: Tagged) -> Option<f64> {
    if value.is_smi() {
        return Some(Smi::to_int(value) as f64);
    }
    let object = value.to_strong_heap_object()?;
    if instance_type::is_heap_number(heap.instance_type(object)) {
        Some(HeapNumber::unchecked_cast(object).value(heap))
    } else {
        None
    }
}

/// `Object::IsNumber`
pub fn is_number(heap: &Heap, value: Tagged) -> bool {
    if value.is_smi() {
        return true;
    }
    match value.to_strong_heap_object() {
        Some(object) => instance_type::is_heap_number(heap.instance_type(object)),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::heap::AllocationType;
    use crate::objects::map::Map;

    fn world() -> (Heap, Map) {
        let mut heap = Heap::new();
        let meta = heap.allocate_raw_or_fail(Map::K_SIZE as usize, AllocationType::Map);
        heap.set_map(meta, meta);
        let meta_map = Map::unchecked_cast(meta);
        meta_map.set_instance_type(&mut heap, instance_type::MAP_TYPE);
        meta_map.set_instance_size(&mut heap, Map::K_SIZE);

        let object = heap.allocate_raw_or_fail(Map::K_SIZE as usize, AllocationType::Map);
        heap.set_map(object, meta);
        let map = Map::unchecked_cast(object);
        map.set_instance_type(&mut heap, instance_type::HEAP_NUMBER_TYPE);
        map.set_instance_size(&mut heap, HeapNumber::K_SIZE);
        (heap, map)
    }

    fn new_number(heap: &mut Heap, map: Map, value: f64) -> HeapNumber {
        let object = heap.allocate_raw_or_fail(HeapNumber::K_SIZE as usize, AllocationType::Young);
        heap.set_map(object, map.object());
        let number = HeapNumber::unchecked_cast(object);
        number.set_value(heap, value);
        number
    }

    #[test]
    fn layout() {
        assert_eq!(HeapNumber::K_SIZE, 16);
    }

    #[test]
    fn doubles_round_trip_including_the_awkward_ones() {
        let (mut heap, map) = world();
        for value in [
            0.0,
            -0.0,
            1.5,
            -1.5,
            f64::MIN,
            f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
            1e308,
            5e-324,
        ] {
            let number = new_number(&mut heap, map, value);
            assert_eq!(number.value(&heap).to_bits(), value.to_bits(), "{value}");
        }
    }

    #[test]
    fn negative_zero_keeps_its_sign() {
        let (mut heap, map) = world();
        let number = new_number(&mut heap, map, -0.0);
        assert_eq!(number.value(&heap), 0.0);
        assert!(
            number.value(&heap).is_sign_negative(),
            "-0 must not decay to 0, or 1/x gives the wrong infinity"
        );
    }

    #[test]
    fn nan_survives() {
        let (mut heap, map) = world();
        let number = new_number(&mut heap, map, f64::NAN);
        assert!(number.value(&heap).is_nan());
    }

    #[test]
    fn number_value_accepts_smis_and_heap_numbers_and_nothing_else() {
        let (mut heap, map) = world();
        assert_eq!(number_value(&heap, Smi::from_int(42)), Some(42.0));
        assert_eq!(number_value(&heap, Smi::from_int(-1)), Some(-1.0));

        let boxed = new_number(&mut heap, map, 1.5);
        assert_eq!(number_value(&heap, boxed.tagged()), Some(1.5));
        assert!(is_number(&heap, boxed.tagged()));
        assert!(is_number(&heap, Smi::zero()));

        // A map is not a number.
        assert_eq!(number_value(&heap, map.tagged()), None);
        assert!(!is_number(&heap, map.tagged()));
    }

    #[test]
    fn a_double_outside_smi_range_must_be_boxed() {
        // The division of labour between Smi and HeapNumber.
        assert!(Smi::try_from_f64(42.0).is_some());
        assert!(Smi::try_from_f64(4e9).is_none(), "beyond i32");
        assert!(Smi::try_from_f64(0.5).is_none(), "not integral");
        assert!(Smi::try_from_f64(-0.0).is_none(), "sign would be lost");
    }
}
