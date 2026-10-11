use crate::common::globals::{InlineCacheState, K_MAX_POLYMORPHIC_MAP_COUNT, K_TAGGED_SIZE};
use crate::execution::isolate::Isolate;
use crate::handles::handles::*;
use crate::ic::handler_configuration::{Handler, HandlerKind};
use crate::objects::feedback_vector::{
    FeedbackNexus, FeedbackSlotKind, FeedbackVector, MapAndHandler,
};
use crate::objects::fixed_array::WeakFixedArray;
use crate::objects::js_objects::JSObject;
use crate::objects::map::Map;
use crate::objects::property_details::PropertyLocation;
use crate::objects::tagged::Tagged;

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct SiteStatistics {
    pub hits: usize,
    pub shared_hits: usize,
    pub misses: usize,
    pub monomorphic_transitions: usize,
    pub polymorphic_transitions: usize,
    pub megamorphic_transitions: usize,
}

impl SiteStatistics {
    pub fn total(&self) -> usize {
        self.hits + self.shared_hits + self.misses
    }

    pub fn answered(&self) -> usize {
        self.hits + self.shared_hits
    }

    pub fn hit_rate(&self) -> f64 {
        if self.total() == 0 {
            0.0
        } else {
            self.answered() as f64 / self.total() as f64
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct ICStatistics {
    pub property: SiteStatistics,
    pub global: SiteStatistics,
}

impl ICStatistics {
    pub fn hits(&self) -> usize {
        self.property.answered() + self.global.answered()
    }

    pub fn misses(&self) -> usize {
        self.property.misses + self.global.misses
    }

    pub fn total(&self) -> usize {
        self.hits() + self.misses()
    }
}

impl Isolate {
    #[inline]
    pub fn uninitialized_sentinel(&self) -> Tagged {
        self.roots.uninitialized_value
    }

    /// `FeedbackVector::MegamorphicSentinel`
    #[inline]
    pub fn megamorphic_sentinel(&self) -> Tagged {
        self.roots.megamorphic_symbol
    }

    pub fn ic_statistics(&self) -> ICStatistics {
        self.ic_statistics
    }

    pub fn reset_ic_statistics(&mut self) {
        self.ic_statistics = ICStatistics::default();
        self.ic_trace.clear();
    }

    pub fn ic_transition_log(&self) -> &[String] {
        &self.ic_trace
    }

    pub fn ic_state(&self, nexus: FeedbackNexus) -> InlineCacheState {
        nexus.ic_state(
            &self.heap,
            self.uninitialized_sentinel(),
            self.megamorphic_sentinel(),
        )
    }

    // -----------------------------------------------------------------------
    // LoadIC
    // -----------------------------------------------------------------------

    pub fn load_ic(
        &mut self,
        receiver: JSObject,
        name: Tagged,
        vector: FeedbackVector,
        slot: i32,
    ) -> Tagged {
        if self.is_global_object(receiver) {
            self.ic_statistics.property.misses += 1;
            return self.get_property(receiver, name);
        }

        if !receiver.has_fast_properties(&self.heap) {
            self.ic_statistics.property.misses += 1;
            return self.get_property(receiver, name);
        }

        let nexus = FeedbackNexus::new(&self.heap, vector, slot);
        debug_assert!(nexus.kind().is_load_ic() || nexus.kind().is_global_ic());
        let map = receiver.map(&self.heap);

        if let Some(cached) = nexus.find_handler_for_map(
            &self.heap,
            map,
            self.uninitialized_sentinel(),
            self.megamorphic_sentinel(),
        ) {
            if let Some(value) = self.load_from_handler(receiver, cached) {
                self.ic_statistics.property.hits += 1;
                return value;
            }
        }

        if self.ic_state(nexus) == InlineCacheState::Megamorphic {
            if let Some(handler) = self.stub_cache_get(name, map) {
                if let Some(value) = self.load_from_handler(receiver, handler) {
                    self.ic_statistics.property.shared_hits += 1;
                    return value;
                }
            }
        }

        self.ic_statistics.property.misses += 1;

        if let Some(length) = self.array_length_if_requested(receiver, name) {
            let handler = Handler::native_data_property(0);
            self.update_ic(nexus, map, handler.as_smi(), name);
            return length;
        }

        let found = self.lookup_property(receiver, name);
        let value = match &found {
            Some(found) => self.property_value_at(found),
            None => self.undefined_value(),
        };

        if let Some(found) = found {
            if found.holder == receiver
                && found.dictionary_entry == crate::objects::hash_table::K_NOT_FOUND
                && found.details.location() == PropertyLocation::Field
            {
                let handler = self.handler_for_field(map, found.details);
                self.update_ic(nexus, map, handler.as_smi(), name);
            }
        }
        value
    }

    /// `StoreIC::Store`
    pub fn store_ic(
        &mut self,
        receiver: JSObject,
        name: Tagged,
        value: Tagged,
        vector: FeedbackVector,
        slot: i32,
    ) -> bool {
        if self.is_global_object(receiver) {
            self.ic_statistics.property.misses += 1;
            return self.set_property(receiver, name, value);
        }

        if !receiver.has_fast_properties(&self.heap) {
            self.ic_statistics.property.misses += 1;
            return self.set_property(receiver, name, value);
        }

        if self.array_length_if_requested(receiver, name).is_some() {
            self.ic_statistics.property.misses += 1;
            return self.set_property(receiver, name, value);
        }

        let nexus = FeedbackNexus::new(&self.heap, vector, slot);
        debug_assert!(nexus.kind().is_store_ic() || nexus.kind().is_global_ic());
        let map = receiver.map(&self.heap);

        if let Some(cached) = nexus.find_handler_for_map(
            &self.heap,
            map,
            self.uninitialized_sentinel(),
            self.megamorphic_sentinel(),
        ) {
            if self.store_through_handler(receiver, cached, value) {
                self.ic_statistics.property.hits += 1;
                return true;
            }
        }

        if self.ic_state(nexus) == InlineCacheState::Megamorphic {
            if let Some(handler) = self.stub_cache_get(name, map) {
                if self.store_through_handler(receiver, handler, value) {
                    self.ic_statistics.property.shared_hits += 1;
                    return true;
                }
            }
        }

        self.ic_statistics.property.misses += 1;
        let existing = self.lookup_own_property(receiver, name);
        let stored = self.set_property(receiver, name, value);

        if stored {
            if let Some(existing) = existing {
                if existing.details.location() == PropertyLocation::Field {
                    let handler = self.handler_for_field(map, existing.details);
                    self.update_ic(nexus, map, handler.as_smi(), name);
                }
            }
        }
        stored
    }

    fn handler_for_field(
        &self,
        map: Map,
        details: crate::objects::property_details::PropertyDetails,
    ) -> Handler {
        let field_index = details.field_index();
        let inobject_count = map.inobject_properties(&self.heap) as u32;
        if field_index < inobject_count {
            let offset_in_words = (map.inobject_property_offset(&self.heap, field_index as i32)
                / K_TAGGED_SIZE) as u32;
            Handler::field(offset_in_words, true, false, field_index)
        } else {
            Handler::field(field_index - inobject_count, false, false, field_index)
        }
    }

    fn load_from_handler(&self, receiver: JSObject, handler: Tagged) -> Option<Tagged> {
        let handler = Handler::from_tagged(handler)?;
        match handler.kind() {
            HandlerKind::Field => Some(self.read_field(receiver, handler)),
            HandlerKind::NativeDataProperty => {
                let array = crate::objects::js_array::JSArray::unchecked_cast(receiver.object());
                Some(self.array_length_getter(array))
            }
            _ => None,
        }
    }

    fn store_through_handler(
        &mut self,
        receiver: JSObject,
        handler: Tagged,
        value: Tagged,
    ) -> bool {
        let Some(handler) = Handler::from_tagged(handler) else {
            return false;
        };
        if handler.kind() != HandlerKind::Field {
            return false;
        }
        self.write_field(receiver, handler, value);
        true
    }

    #[inline]
    fn read_field(&self, receiver: JSObject, handler: Handler) -> Tagged {
        if handler.is_inobject() {
            let offset = handler.storage_offset_in_words() as i32 * K_TAGGED_SIZE;
            self.heap.read_tagged(receiver.field(offset))
        } else {
            receiver
                .property_array(&self.heap)
                .get(&self.heap, handler.storage_offset_in_words() as i32)
        }
    }

    #[inline]
    fn write_field(&mut self, receiver: JSObject, handler: Handler, value: Tagged) {
        if handler.is_inobject() {
            let offset = handler.storage_offset_in_words() as i32 * K_TAGGED_SIZE;
            self.heap.write_tagged(receiver.field(offset), value);
        } else {
            let array = receiver.property_array(&self.heap);
            array.set(
                &mut self.heap,
                handler.storage_offset_in_words() as i32,
                value,
            );
        }
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    pub fn load_global_ic(
        &mut self,
        name: Tagged,
        vector: FeedbackVector,
        slot: i32,
    ) -> Option<Tagged> {
        use crate::objects::property_cell::PropertyCell;

        let nexus = FeedbackNexus::new(&self.heap, vector, slot);
        debug_assert!(nexus.kind().is_global_ic());

        let feedback = nexus.feedback(&self.heap);
        if feedback.is_weak() {
            if let Some(object) = feedback.to_heap_object() {
                if self.heap.instance_type(object)
                    == crate::objects::instance_type::PROPERTY_CELL_TYPE
                {
                    self.ic_statistics.global.hits += 1;
                    return Some(PropertyCell::unchecked_cast(object).value(&self.heap));
                }
            }
        }

        self.ic_statistics.global.misses += 1;
        let cell = self.global_cell(name)?;
        self.configure_property_cell_mode(nexus, cell);
        Some(cell.value(&self.heap))
    }

    /// `StoreGlobalIC::Store`
    pub fn store_global_ic(
        &mut self,
        name: Tagged,
        value: Tagged,
        vector: FeedbackVector,
        slot: i32,
    ) {
        use crate::objects::property_cell::PropertyCell;

        let nexus = FeedbackNexus::new(&self.heap, vector, slot);
        debug_assert!(nexus.kind().is_global_ic());

        let feedback = nexus.feedback(&self.heap);
        if feedback.is_weak() {
            if let Some(object) = feedback.to_heap_object() {
                if self.heap.instance_type(object)
                    == crate::objects::instance_type::PROPERTY_CELL_TYPE
                {
                    self.ic_statistics.global.hits += 1;
                    let cell = PropertyCell::unchecked_cast(object);
                    self.set_global_cell_value(cell, value);
                    return;
                }
            }
        }

        self.ic_statistics.global.misses += 1;
        let scope = self.handle_scope();
        let value_handle = self.create_handle(value);
        let cell = self.declare_global_cell(name, value);
        let cell_handle = self.create_object_handle(cell.object());
        let value = value_handle.get(self);
        let cell = PropertyCell::unchecked_cast(cell_handle.object(self));
        self.set_global_cell_value(cell, value);
        let cell = PropertyCell::unchecked_cast(cell_handle.object(self));
        self.configure_property_cell_mode(nexus, cell);
        self.close_handle_scope(scope);
    }

    /// `FeedbackNexus::ConfigurePropertyCellMode`
    fn configure_property_cell_mode(
        &mut self,
        nexus: FeedbackNexus,
        cell: crate::objects::property_cell::PropertyCell,
    ) {
        let before = self.ic_state(nexus);
        nexus.set_feedback(&mut self.heap, cell.object().tagged_weak());
        let sentinel = self.uninitialized_sentinel();
        nexus.set_feedback_extra(&mut self.heap, sentinel);

        if before == InlineCacheState::Uninitialized {
            self.ic_statistics.global.monomorphic_transitions += 1;
            if self.trace_ic_enabled {
                self.ic_trace.push(format!(
                    "{} slot {}: Uninitialized -> Monomorphic (cell 0x{:x})",
                    nexus.kind().name(),
                    nexus.slot(),
                    cell.address()
                ));
            }
        }
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    fn update_ic(&mut self, nexus: FeedbackNexus, map: Map, handler: Tagged, name: Tagged) {
        let before = self.ic_state(nexus);
        self.update_ic_state(nexus, map, handler, name);
        if self.trace_ic_enabled {
            let after = self.ic_state(nexus);
            if before != after {
                let count = self.ic_tracked_map_count(nexus.vector(), nexus.slot());
                self.ic_trace.push(format!(
                    "{} slot {}: {:?} -> {:?} (map 0x{:x}, {} tracked)",
                    nexus.kind().name(),
                    nexus.slot(),
                    before,
                    after,
                    map.address(),
                    count
                ));
            }
        }
    }

    fn update_ic_state(&mut self, nexus: FeedbackNexus, map: Map, handler: Tagged, name: Tagged) {
        match self.ic_state(nexus) {
            InlineCacheState::Uninitialized => {
                nexus.configure_monomorphic(&mut self.heap, map, handler);
                self.ic_statistics.property.monomorphic_transitions += 1;
            }
            InlineCacheState::Monomorphic | InlineCacheState::Polymorphic => {
                self.update_polymorphic_ic(nexus, map, handler, name);
            }
            InlineCacheState::Megamorphic => {
                self.update_megamorphic_cache(map, name, handler);
            }
            InlineCacheState::Generic | InlineCacheState::NoFeedback => {}
        }
    }

    /// `IC::UpdateMegamorphicCache`
    fn update_megamorphic_cache(&mut self, map: Map, name: Tagged, handler: Tagged) {
        let Some(name) = self.as_internalized_name(name) else {
            return;
        };
        let hash = name.raw_hash_field(&self.heap);
        debug_assert!(
            self.heap.in_read_only_space(map.object())
                && self.heap.in_read_only_space(name.object()),
            "the shared table compares by identity, so its keys must not move"
        );
        self.stub_cache.set(name, hash, map, handler);
    }

    fn copy_ic_to_megamorphic_cache(&mut self, nexus: FeedbackNexus, name: Tagged) {
        let pairs = nexus.extract_maps_and_handlers(
            &self.heap,
            self.uninitialized_sentinel(),
            self.megamorphic_sentinel(),
        );
        for pair in pairs {
            self.update_megamorphic_cache(pair.map, name, pair.handler);
        }
    }

    fn stub_cache_get(&mut self, name: Tagged, map: Map) -> Option<Tagged> {
        let name = self.as_internalized_name(name)?;
        let hash = name.raw_hash_field(&self.heap);
        self.stub_cache.get(name, hash, map)
    }

    fn as_internalized_name(&self, name: Tagged) -> Option<crate::objects::name::Name> {
        let string = crate::objects::string::String::try_cast(&self.heap, name)?;
        if string.is_internalized(&self.heap) {
            Some(string.as_name())
        } else {
            None
        }
    }

    pub fn stub_cache_statistics(&self) -> (usize, usize, usize, usize) {
        (
            self.stub_cache.primary_hits,
            self.stub_cache.secondary_hits,
            self.stub_cache.misses,
            self.stub_cache.updates,
        )
    }

    pub fn stub_cache_occupancy(&self) -> (usize, usize) {
        self.stub_cache.occupancy()
    }

    fn update_polymorphic_ic(
        &mut self,
        nexus: FeedbackNexus,
        map: Map,
        handler: Tagged,
        name: Tagged,
    ) {
        let mut pairs = nexus.extract_maps_and_handlers(
            &self.heap,
            self.uninitialized_sentinel(),
            self.megamorphic_sentinel(),
        );

        if let Some(existing) = pairs.iter_mut().find(|pair| pair.map == map) {
            if existing.handler == handler {
                return;
            }
            existing.handler = handler;
        } else {
            if pairs.len() >= K_MAX_POLYMORPHIC_MAP_COUNT {
                self.copy_ic_to_megamorphic_cache(nexus, name);
                let sentinel = self.megamorphic_sentinel();
                nexus.configure_megamorphic(&mut self.heap, sentinel);
                self.update_megamorphic_cache(map, name, handler);
                self.ic_statistics.property.megamorphic_transitions += 1;
                return;
            }
            pairs.push(MapAndHandler { map, handler });
        }

        if pairs.len() == 1 {
            nexus.configure_monomorphic(&mut self.heap, pairs[0].map, pairs[0].handler);
            return;
        }
        self.configure_polymorphic(nexus, &pairs);
        self.ic_statistics.property.polymorphic_transitions += 1;
    }

    fn configure_polymorphic(&mut self, nexus: FeedbackNexus, pairs: &[MapAndHandler]) {
        let length = (pairs.len() * 2) as i32;
        let array = self.new_weak_fixed_array(length);
        for (index, pair) in pairs.iter().enumerate() {
            let slot = (index * 2) as i32;
            array.set(&mut self.heap, slot, pair.map.object().tagged_weak());
            array.set(&mut self.heap, slot + 1, pair.handler);
        }
        nexus.set_feedback(&mut self.heap, array.tagged());
        let sentinel = self.uninitialized_sentinel();
        nexus.set_feedback_extra(&mut self.heap, sentinel);
    }

    pub fn ic_tracked_map_count(&self, vector: FeedbackVector, slot: i32) -> usize {
        let nexus = FeedbackNexus::new(&self.heap, vector, slot);
        nexus
            .extract_maps_and_handlers(
                &self.heap,
                self.uninitialized_sentinel(),
                self.megamorphic_sentinel(),
            )
            .len()
    }

    pub fn describe_ic(&self, vector: FeedbackVector, slot: i32) -> String {
        let nexus = FeedbackNexus::new(&self.heap, vector, slot);
        let state = self.ic_state(nexus);
        let count = self.ic_tracked_map_count(vector, slot);
        format!(
            "slot {slot} [{}] {state:?} ({count} map{})",
            nexus.kind().name(),
            if count == 1 { "" } else { "s" }
        )
    }

    pub(crate) fn new_weak_fixed_array(&mut self, length: i32) -> WeakFixedArray {
        use crate::heap::heap::AllocationType;
        let map = self.roots.fixed_array_map.to_strong_heap_object().unwrap();
        let object = self.allocate_raw_with_retry_or_fail(
            WeakFixedArray::size_for(length) as usize,
            AllocationType::Old,
        );
        self.heap.set_map(object, map);
        let array = WeakFixedArray::unchecked_cast(object);
        array.set_length(&mut self.heap, length);
        let sentinel = self.uninitialized_sentinel();
        for index in 0..length {
            array.set(&mut self.heap, index, sentinel);
        }
        array
    }
}

pub fn expected_slot_kind_for_load() -> FeedbackSlotKind {
    FeedbackSlotKind::LoadProperty
}
