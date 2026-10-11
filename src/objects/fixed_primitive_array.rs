use crate::common::globals::{K_DOUBLE_SIZE, K_HOLE_NAN_INT64, K_TAGGED_SIZE};
use crate::heap::heap::Heap;
use crate::objects::smi::Smi;
use crate::objects::tagged::Address;

object_type!(
    // FixedDoubleArray describes fixed-sized arrays with element type double.
    FixedDoubleArray
);

impl FixedDoubleArray {
    pub const K_LENGTH_OFFSET: i32 = K_TAGGED_SIZE;
    pub const K_HEADER_SIZE: i32 = Self::K_LENGTH_OFFSET + K_TAGGED_SIZE;

    #[inline]
    pub const fn size_for(length: i32) -> i32 {
        Self::K_HEADER_SIZE + length * K_DOUBLE_SIZE
    }

    #[inline]
    pub const fn offset_of_element_at(index: i32) -> i32 {
        Self::K_HEADER_SIZE + index * K_DOUBLE_SIZE
    }

    #[inline]
    pub fn length(self, heap: &Heap) -> i32 {
        Smi::to_int(heap.read_tagged(self.field(Self::K_LENGTH_OFFSET)))
    }

    #[inline]
    pub fn set_length(self, heap: &mut Heap, length: i32) {
        heap.write_tagged(self.field(Self::K_LENGTH_OFFSET), Smi::from_int(length));
    }

    #[inline]
    pub fn element_address(self, index: i32) -> Address {
        self.field(Self::offset_of_element_at(index))
    }

    #[inline]
    fn in_bounds(self, heap: &Heap, index: i32) -> bool {
        index >= 0 && index < self.length(heap)
    }

    #[inline]
    pub fn get_representation(self, heap: &Heap, index: i32) -> u64 {
        debug_assert!(self.in_bounds(heap, index));
        heap.read_u64(self.element_address(index))
    }

    #[inline]
    pub fn get_scalar(self, heap: &Heap, index: i32) -> f64 {
        debug_assert!(!self.is_the_hole(heap, index));
        heap.read_f64(self.element_address(index))
    }

    #[inline]
    pub fn set(self, heap: &mut Heap, index: i32, value: f64) {
        debug_assert!(self.in_bounds(heap, index));
        if value.is_nan() {
            heap.write_u64(self.element_address(index), f64::NAN.to_bits());
        } else {
            heap.write_f64(self.element_address(index), value);
        }
    }

    #[inline]
    pub fn set_the_hole(self, heap: &mut Heap, index: i32) {
        debug_assert!(self.in_bounds(heap, index));
        heap.write_u64(self.element_address(index), K_HOLE_NAN_INT64);
    }

    #[inline]
    pub fn is_the_hole(self, heap: &Heap, index: i32) -> bool {
        self.get_representation(heap, index) == K_HOLE_NAN_INT64
    }

    pub fn fill_with_holes(self, heap: &mut Heap, from: i32, to: i32) {
        for index in from..to {
            self.set_the_hole(heap, index);
        }
    }

    pub fn move_elements(self, heap: &mut Heap, dst_index: i32, src_index: i32, len: i32) {
        if dst_index <= src_index {
            for offset in 0..len {
                let value = self.get_representation(heap, src_index + offset);
                heap.write_u64(self.element_address(dst_index + offset), value);
            }
        } else {
            for offset in (0..len).rev() {
                let value = self.get_representation(heap, src_index + offset);
                heap.write_u64(self.element_address(dst_index + offset), value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::isolate::Isolate;

    fn array(isolate: &mut Isolate, length: i32) -> FixedDoubleArray {
        isolate.new_fixed_double_array(length)
    }

    #[test]
    fn a_double_is_stored_unboxed() {
        let mut isolate = Isolate::new();
        let doubles = array(&mut isolate, 3);
        doubles.set(&mut isolate.heap, 0, 1.5);
        doubles.set(&mut isolate.heap, 1, -0.0);
        doubles.set(&mut isolate.heap, 2, 1e300);
        assert_eq!(doubles.get_scalar(&isolate.heap, 0), 1.5);
        assert!(doubles.get_scalar(&isolate.heap, 1).is_sign_negative());
        assert_eq!(doubles.get_scalar(&isolate.heap, 2), 1e300);
        assert_eq!(
            FixedDoubleArray::size_for(3),
            FixedDoubleArray::K_HEADER_SIZE + 24
        );
    }

    #[test]
    fn the_hole_is_a_nan_that_no_computation_produces() {
        let mut isolate = Isolate::new();
        let doubles = array(&mut isolate, 2);
        doubles.fill_with_holes(&mut isolate.heap, 0, 2);
        assert!(doubles.is_the_hole(&isolate.heap, 0));

        doubles.set(&mut isolate.heap, 1, 0.0 / 0.0);
        assert!(!doubles.is_the_hole(&isolate.heap, 1));
        assert!(doubles.get_scalar(&isolate.heap, 1).is_nan());
    }

    #[test]
    fn move_elements_handles_overlap_in_both_directions() {
        let mut isolate = Isolate::new();
        let doubles = array(&mut isolate, 5);
        for index in 0..5 {
            doubles.set(&mut isolate.heap, index, index as f64);
        }
        doubles.move_elements(&mut isolate.heap, 1, 0, 4);
        let values: Vec<f64> = (0..5)
            .map(|index| doubles.get_scalar(&isolate.heap, index))
            .collect();
        assert_eq!(values, vec![0.0, 0.0, 1.0, 2.0, 3.0]);

        doubles.move_elements(&mut isolate.heap, 0, 1, 4);
        let values: Vec<f64> = (0..5)
            .map(|index| doubles.get_scalar(&isolate.heap, index))
            .collect();
        assert_eq!(values, vec![0.0, 1.0, 2.0, 3.0, 3.0]);
    }
}
