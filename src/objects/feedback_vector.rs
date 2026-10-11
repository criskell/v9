use crate::common::globals::{InlineCacheState, K_TAGGED_SIZE};
use crate::heap::heap::Heap;
use crate::objects::fixed_array::WeakFixedArray;
use crate::objects::map::Map;
use crate::objects::smi::Smi;
use crate::objects::tagged::{Address, HeapObject, Tagged};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum FeedbackSlotKind {
    Invalid = 0,
    Call,
    LoadProperty,
    LoadGlobalNotInsideTypeof,
    LoadGlobalInsideTypeof,
    LoadKeyed,
    SetNamedStrict,
    DefineNamedOwn,
    SetKeyedStrict,
    StoreGlobalStrict,
    BinaryOp,
    CompareOp,
    Literal,
    TypeOf,
    InstanceOf,
    JumpLoop,
}

impl FeedbackSlotKind {
    pub fn from_u8(value: u8) -> FeedbackSlotKind {
        use FeedbackSlotKind::*;
        match value {
            1 => Call,
            2 => LoadProperty,
            3 => LoadGlobalNotInsideTypeof,
            4 => LoadGlobalInsideTypeof,
            5 => LoadKeyed,
            6 => SetNamedStrict,
            7 => DefineNamedOwn,
            8 => SetKeyedStrict,
            9 => StoreGlobalStrict,
            10 => BinaryOp,
            11 => CompareOp,
            12 => Literal,
            13 => TypeOf,
            14 => InstanceOf,
            15 => JumpLoop,
            _ => Invalid,
        }
    }

    pub fn slot_size(self) -> i32 {
        use FeedbackSlotKind::*;
        match self {
            BinaryOp | CompareOp | Literal | TypeOf | InstanceOf | JumpLoop => 1,
            Call
            | LoadProperty
            | LoadGlobalNotInsideTypeof
            | LoadGlobalInsideTypeof
            | LoadKeyed
            | SetNamedStrict
            | DefineNamedOwn
            | SetKeyedStrict
            | StoreGlobalStrict => 2,
            Invalid => unreachable!("an invalid slot has no size"),
        }
    }

    /// `IsLoadICKind`
    pub fn is_load_ic(self) -> bool {
        matches!(self, FeedbackSlotKind::LoadProperty)
    }

    pub fn is_store_ic(self) -> bool {
        matches!(
            self,
            FeedbackSlotKind::SetNamedStrict | FeedbackSlotKind::DefineNamedOwn
        )
    }

    /// `IsGlobalICKind`
    pub fn is_global_ic(self) -> bool {
        matches!(
            self,
            FeedbackSlotKind::LoadGlobalNotInsideTypeof
                | FeedbackSlotKind::LoadGlobalInsideTypeof
                | FeedbackSlotKind::StoreGlobalStrict
        )
    }

    pub fn caches_maps(self) -> bool {
        self.is_load_ic() || self.is_store_ic() || matches!(self, FeedbackSlotKind::LoadKeyed)
    }

    pub fn name(self) -> &'static str {
        use FeedbackSlotKind::*;
        match self {
            Invalid => "Invalid",
            Call => "Call",
            LoadProperty => "LoadProperty",
            LoadGlobalNotInsideTypeof => "LoadGlobalNotInsideTypeof",
            LoadGlobalInsideTypeof => "LoadGlobalInsideTypeof",
            LoadKeyed => "LoadKeyed",
            SetNamedStrict => "SetNamedStrict",
            DefineNamedOwn => "DefineNamedOwn",
            SetKeyedStrict => "SetKeyedStrict",
            StoreGlobalStrict => "StoreGlobalStrict",
            BinaryOp => "BinaryOp",
            CompareOp => "CompareOp",
            Literal => "Literal",
            TypeOf => "TypeOf",
            InstanceOf => "InstanceOf",
            JumpLoop => "JumpLoop",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FeedbackVectorSpec {
    kinds: Vec<FeedbackSlotKind>,
    slot_count: i32,
}

impl FeedbackVectorSpec {
    pub fn new() -> FeedbackVectorSpec {
        FeedbackVectorSpec::default()
    }

    pub fn add_slot(&mut self, kind: FeedbackSlotKind) -> i32 {
        let slot = self.slot_count;
        self.slot_count += kind.slot_size();
        self.kinds.push(kind);
        slot
    }

    pub fn add_load_ic_slot(&mut self) -> i32 {
        self.add_slot(FeedbackSlotKind::LoadProperty)
    }

    pub fn add_keyed_load_ic_slot(&mut self) -> i32 {
        self.add_slot(FeedbackSlotKind::LoadKeyed)
    }

    pub fn add_store_ic_slot(&mut self) -> i32 {
        self.add_slot(FeedbackSlotKind::SetNamedStrict)
    }

    pub fn add_define_named_own_ic_slot(&mut self) -> i32 {
        self.add_slot(FeedbackSlotKind::DefineNamedOwn)
    }

    pub fn add_keyed_store_ic_slot(&mut self) -> i32 {
        self.add_slot(FeedbackSlotKind::SetKeyedStrict)
    }

    pub fn add_load_global_ic_slot(&mut self, inside_typeof: bool) -> i32 {
        self.add_slot(if inside_typeof {
            FeedbackSlotKind::LoadGlobalInsideTypeof
        } else {
            FeedbackSlotKind::LoadGlobalNotInsideTypeof
        })
    }

    pub fn add_store_global_ic_slot(&mut self) -> i32 {
        self.add_slot(FeedbackSlotKind::StoreGlobalStrict)
    }

    pub fn add_call_ic_slot(&mut self) -> i32 {
        self.add_slot(FeedbackSlotKind::Call)
    }

    pub fn add_type_of_slot(&mut self) -> i32 {
        self.add_slot(FeedbackSlotKind::TypeOf)
    }

    pub fn add_instance_of_slot(&mut self) -> i32 {
        self.add_slot(FeedbackSlotKind::InstanceOf)
    }

    pub fn add_literal_slot(&mut self) -> i32 {
        self.add_slot(FeedbackSlotKind::Literal)
    }

    pub fn add_jump_loop_slot(&mut self) -> i32 {
        self.add_slot(FeedbackSlotKind::JumpLoop)
    }

    pub fn slot_count(&self) -> i32 {
        self.slot_count
    }

    pub fn kinds(&self) -> &[FeedbackSlotKind] {
        &self.kinds
    }

    pub fn to_slot_kinds(&self) -> Vec<FeedbackSlotKind> {
        let mut kinds = Vec::with_capacity(self.slot_count as usize);
        for kind in &self.kinds {
            kinds.push(*kind);
            for _ in 1..kind.slot_size() {
                kinds.push(FeedbackSlotKind::Invalid);
            }
        }
        kinds
    }
}

object_type!(FeedbackMetadata);

impl FeedbackMetadata {
    pub const K_SLOT_COUNT_OFFSET: i32 = K_TAGGED_SIZE;
    pub const K_HEADER_SIZE: i32 = Self::K_SLOT_COUNT_OFFSET + 8;

    pub fn size_for(slot_count: i32) -> i32 {
        crate::common::globals::object_pointer_align((Self::K_HEADER_SIZE + slot_count) as u64)
            as i32
    }

    #[inline]
    pub fn slot_count(self, heap: &Heap) -> i32 {
        heap.read_u32(self.field(Self::K_SLOT_COUNT_OFFSET)) as i32
    }

    #[inline]
    pub fn set_slot_count(self, heap: &mut Heap, value: i32) {
        heap.write_u32(self.field(Self::K_SLOT_COUNT_OFFSET), value as u32);
    }

    /// `FeedbackMetadata::GetKind`
    #[inline]
    pub fn get_kind(self, heap: &Heap, slot: i32) -> FeedbackSlotKind {
        debug_assert!(slot >= 0 && slot < self.slot_count(heap));
        FeedbackSlotKind::from_u8(heap.read_u8(self.field(Self::K_HEADER_SIZE + slot)))
    }

    #[inline]
    pub fn set_kind(self, heap: &mut Heap, slot: i32, kind: FeedbackSlotKind) {
        heap.write_u8(self.field(Self::K_HEADER_SIZE + slot), kind as u8);
    }
}

object_type!(
    /// `FeedbackVector`
    FeedbackVector
);

impl FeedbackVector {
    pub const K_LENGTH_OFFSET: i32 = K_TAGGED_SIZE;
    pub const K_INVOCATION_COUNT_OFFSET: i32 = Self::K_LENGTH_OFFSET + 4;
    pub const K_SHARED_FUNCTION_INFO_OFFSET: i32 = Self::K_INVOCATION_COUNT_OFFSET + 4;
    pub const K_METADATA_OFFSET: i32 = Self::K_SHARED_FUNCTION_INFO_OFFSET + K_TAGGED_SIZE;
    /// `FeedbackVector::kHeaderSize`
    pub const K_HEADER_SIZE: i32 = Self::K_METADATA_OFFSET + K_TAGGED_SIZE;

    pub fn size_for(length: i32) -> i32 {
        Self::K_HEADER_SIZE + length * K_TAGGED_SIZE
    }

    #[inline]
    pub fn length(self, heap: &Heap) -> i32 {
        heap.read_u32(self.field(Self::K_LENGTH_OFFSET)) as i32
    }

    #[inline]
    pub fn set_length(self, heap: &mut Heap, value: i32) {
        heap.write_u32(self.field(Self::K_LENGTH_OFFSET), value as u32);
    }

    #[inline]
    pub fn invocation_count(self, heap: &Heap) -> i32 {
        heap.read_u32(self.field(Self::K_INVOCATION_COUNT_OFFSET)) as i32
    }

    #[inline]
    pub fn set_invocation_count(self, heap: &mut Heap, value: i32) {
        heap.write_u32(self.field(Self::K_INVOCATION_COUNT_OFFSET), value as u32);
    }

    pub fn increment_invocation_count(self, heap: &mut Heap) {
        let count = self.invocation_count(heap);
        self.set_invocation_count(heap, count.saturating_add(1));
    }

    #[inline]
    pub fn shared_function_info(self, heap: &Heap) -> Tagged {
        heap.read_tagged(self.field(Self::K_SHARED_FUNCTION_INFO_OFFSET))
    }

    #[inline]
    pub fn set_shared_function_info(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_SHARED_FUNCTION_INFO_OFFSET), value);
    }

    #[inline]
    pub fn shared_function_info_address(self) -> Address {
        self.field(Self::K_SHARED_FUNCTION_INFO_OFFSET)
    }

    #[inline]
    pub fn metadata(self, heap: &Heap) -> FeedbackMetadata {
        FeedbackMetadata::unchecked_cast(
            heap.read_tagged(self.field(Self::K_METADATA_OFFSET))
                .to_strong_heap_object()
                .expect("a FeedbackVector always has metadata"),
        )
    }

    #[inline]
    pub fn set_metadata(self, heap: &mut Heap, value: Tagged) {
        heap.write_tagged(self.field(Self::K_METADATA_OFFSET), value);
    }

    #[inline]
    pub fn metadata_address(self) -> Address {
        self.field(Self::K_METADATA_OFFSET)
    }

    #[inline]
    pub fn slot_address(self, slot: i32) -> Address {
        self.field(Self::K_HEADER_SIZE + slot * K_TAGGED_SIZE)
    }

    /// `FeedbackVector::Get`
    #[inline]
    pub fn get(self, heap: &Heap, slot: i32) -> Tagged {
        debug_assert!(slot >= 0 && slot < self.length(heap));
        heap.read_tagged(self.slot_address(slot))
    }

    /// `FeedbackVector::Set`
    #[inline]
    pub fn set(self, heap: &mut Heap, slot: i32, value: Tagged) {
        debug_assert!(slot >= 0 && slot < self.length(heap));
        heap.write_tagged(self.slot_address(slot), value);
    }

    #[inline]
    pub fn get_kind(self, heap: &Heap, slot: i32) -> FeedbackSlotKind {
        self.metadata(heap).get_kind(heap, slot)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct FeedbackNexus {
    vector: FeedbackVector,
    slot: i32,
    kind: FeedbackSlotKind,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct MapAndHandler {
    pub map: Map,
    pub handler: Tagged,
}

impl FeedbackNexus {
    pub fn new(heap: &Heap, vector: FeedbackVector, slot: i32) -> FeedbackNexus {
        FeedbackNexus {
            vector,
            slot,
            kind: vector.get_kind(heap, slot),
        }
    }

    #[inline]
    pub fn kind(self) -> FeedbackSlotKind {
        self.kind
    }

    #[inline]
    pub fn slot(self) -> i32 {
        self.slot
    }

    #[inline]
    pub fn vector(self) -> FeedbackVector {
        self.vector
    }

    /// `FeedbackNexus::GetFeedbackPair`
    #[inline]
    pub fn feedback(self, heap: &Heap) -> Tagged {
        self.vector.get(heap, self.slot)
    }

    #[inline]
    pub fn feedback_extra(self, heap: &Heap) -> Tagged {
        debug_assert_eq!(self.kind.slot_size(), 2);
        self.vector.get(heap, self.slot + 1)
    }

    pub fn set_feedback(self, heap: &mut Heap, value: Tagged) {
        self.vector.set(heap, self.slot, value);
    }

    pub fn set_feedback_extra(self, heap: &mut Heap, value: Tagged) {
        debug_assert_eq!(self.kind.slot_size(), 2);
        self.vector.set(heap, self.slot + 1, value);
    }

    pub fn ic_state(
        self,
        heap: &Heap,
        uninitialized_sentinel: Tagged,
        megamorphic_sentinel: Tagged,
    ) -> InlineCacheState {
        if !self.kind.caches_maps() {
            let feedback = self.feedback(heap);
            return if feedback == uninitialized_sentinel {
                InlineCacheState::Uninitialized
            } else {
                InlineCacheState::Monomorphic
            };
        }

        let feedback = self.feedback(heap);
        if feedback == uninitialized_sentinel {
            return InlineCacheState::Uninitialized;
        }
        if feedback == megamorphic_sentinel {
            return InlineCacheState::Megamorphic;
        }
        if feedback.is_weak_or_cleared() {
            return InlineCacheState::Monomorphic;
        }
        InlineCacheState::Polymorphic
    }

    /// `FeedbackNexus::ConfigureMonomorphic`
    pub fn configure_monomorphic(self, heap: &mut Heap, map: Map, handler: Tagged) {
        self.set_feedback(heap, map.object().tagged_weak());
        self.set_feedback_extra(heap, handler);
    }

    pub fn configure_megamorphic(self, heap: &mut Heap, megamorphic_sentinel: Tagged) {
        self.set_feedback(heap, megamorphic_sentinel);
        self.set_feedback_extra(heap, megamorphic_sentinel);
    }

    pub fn extract_maps_and_handlers(
        self,
        heap: &Heap,
        uninitialized_sentinel: Tagged,
        megamorphic_sentinel: Tagged,
    ) -> Vec<MapAndHandler> {
        match self.ic_state(heap, uninitialized_sentinel, megamorphic_sentinel) {
            InlineCacheState::Monomorphic => {
                let feedback = self.feedback(heap);
                match feedback.to_heap_object() {
                    Some(object) => vec![MapAndHandler {
                        map: Map::unchecked_cast(object),
                        handler: self.feedback_extra(heap),
                    }],
                    None => Vec::new(),
                }
            }
            InlineCacheState::Polymorphic => {
                let array = WeakFixedArray::unchecked_cast(
                    self.feedback(heap)
                        .to_strong_heap_object()
                        .expect("a polymorphic site holds an array"),
                );
                let length = array.length(heap);
                let mut pairs = Vec::new();
                let mut index = 0;
                while index + 1 < length {
                    let map = array.get(heap, index);
                    if let Some(object) = map.to_heap_object() {
                        pairs.push(MapAndHandler {
                            map: Map::unchecked_cast(object),
                            handler: array.get(heap, index + 1),
                        });
                    }
                    index += 2;
                }
                pairs
            }
            _ => Vec::new(),
        }
    }

    pub fn find_handler_for_map(
        self,
        heap: &Heap,
        map: Map,
        uninitialized_sentinel: Tagged,
        megamorphic_sentinel: Tagged,
    ) -> Option<Tagged> {
        match self.ic_state(heap, uninitialized_sentinel, megamorphic_sentinel) {
            InlineCacheState::Monomorphic => {
                let feedback = self.feedback(heap);
                let cached = feedback.to_heap_object()?;
                if cached == map.object() {
                    Some(self.feedback_extra(heap))
                } else {
                    None
                }
            }
            InlineCacheState::Polymorphic => {
                let array =
                    WeakFixedArray::unchecked_cast(self.feedback(heap).to_strong_heap_object()?);
                let length = array.length(heap);
                let mut index = 0;
                while index + 1 < length {
                    if let Some(object) = array.get(heap, index).to_heap_object() {
                        if object == map.object() {
                            return Some(array.get(heap, index + 1));
                        }
                    }
                    index += 2;
                }
                None
            }
            _ => None,
        }
    }
}

pub fn polymorphic_array_length(count: usize) -> i32 {
    (count * 2) as i32
}

pub fn try_cast_feedback_vector(heap: &Heap, value: Tagged) -> Option<FeedbackVector> {
    let object: HeapObject = value.to_strong_heap_object()?;
    if heap.instance_type(object) == crate::objects::instance_type::FEEDBACK_VECTOR_TYPE {
        Some(FeedbackVector::unchecked_cast(object))
    } else {
        None
    }
}

pub fn hint_value(heap: &Heap, nexus: FeedbackNexus) -> i32 {
    let feedback = nexus.feedback(heap);
    if feedback.is_smi() {
        Smi::to_int(feedback)
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_widths_follow_what_the_site_caches() {
        for kind in [
            FeedbackSlotKind::LoadProperty,
            FeedbackSlotKind::SetNamedStrict,
            FeedbackSlotKind::LoadKeyed,
            FeedbackSlotKind::LoadGlobalNotInsideTypeof,
            FeedbackSlotKind::Call,
        ] {
            assert_eq!(kind.slot_size(), 2, "{}", kind.name());
        }
        for kind in [
            FeedbackSlotKind::BinaryOp,
            FeedbackSlotKind::CompareOp,
            FeedbackSlotKind::TypeOf,
            FeedbackSlotKind::JumpLoop,
        ] {
            assert_eq!(kind.slot_size(), 1, "{}", kind.name());
        }
    }

    #[test]
    fn the_spec_lays_out_slots_by_width() {
        let mut spec = FeedbackVectorSpec::new();
        let load = spec.add_load_ic_slot();
        let hint = spec.add_type_of_slot();
        let store = spec.add_store_ic_slot();

        assert_eq!(load, 0);
        assert_eq!(hint, 2, "the load took two slots");
        assert_eq!(store, 3);
        assert_eq!(spec.slot_count(), 5);
    }

    #[test]
    fn the_second_slot_of_a_pair_is_marked_invalid() {
        let mut spec = FeedbackVectorSpec::new();
        spec.add_load_ic_slot();
        spec.add_type_of_slot();
        let kinds = spec.to_slot_kinds();
        assert_eq!(
            kinds,
            vec![
                FeedbackSlotKind::LoadProperty,
                FeedbackSlotKind::Invalid,
                FeedbackSlotKind::TypeOf,
            ]
        );
    }

    #[test]
    fn slot_kinds_round_trip_through_a_byte() {
        for kind in [
            FeedbackSlotKind::Call,
            FeedbackSlotKind::LoadProperty,
            FeedbackSlotKind::SetNamedStrict,
            FeedbackSlotKind::JumpLoop,
        ] {
            assert_eq!(FeedbackSlotKind::from_u8(kind as u8), kind);
        }
        assert_eq!(FeedbackSlotKind::from_u8(200), FeedbackSlotKind::Invalid);
    }

    #[test]
    fn only_shape_caching_kinds_climb_the_lattice() {
        assert!(FeedbackSlotKind::LoadProperty.caches_maps());
        assert!(FeedbackSlotKind::SetNamedStrict.caches_maps());
        assert!(FeedbackSlotKind::LoadKeyed.caches_maps());
        assert!(!FeedbackSlotKind::BinaryOp.caches_maps());
        assert!(!FeedbackSlotKind::Call.caches_maps());
        assert!(FeedbackSlotKind::LoadGlobalNotInsideTypeof.is_global_ic());
    }

    #[test]
    fn layout() {
        assert_eq!(FeedbackVector::K_LENGTH_OFFSET, 8);
        assert_eq!(FeedbackVector::K_INVOCATION_COUNT_OFFSET, 12);
        assert_eq!(FeedbackVector::K_SHARED_FUNCTION_INFO_OFFSET, 16);
        assert_eq!(FeedbackVector::K_METADATA_OFFSET, 24);
        assert_eq!(FeedbackVector::K_HEADER_SIZE, 32);
        assert_eq!(FeedbackVector::size_for(0), 32);
        assert_eq!(FeedbackVector::size_for(2), 48);

        assert_eq!(FeedbackMetadata::K_HEADER_SIZE, 16);
        assert_eq!(FeedbackMetadata::size_for(0), 16);
        assert_eq!(FeedbackMetadata::size_for(1), 24);
        assert_eq!(FeedbackMetadata::size_for(8), 24);
    }
}
