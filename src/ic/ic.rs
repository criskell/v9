//! Ported from `src/ic/ic.h` / `ic.cc`.
//!
//! The state machine that fills in the feedback vector, and the fast path
//! that reads it back.
//!
//! What an inline cache is for
//! --------------------------
//! `o.x` has to find `x`. Without a cache that means: look at `o`'s map,
//! search its descriptor array for the name, read the field index out of the
//! details, then read the field. The search is the expensive part, and it
//! gives the same answer every time `o` has the same shape.
//!
//! So the site records the answer: *this map, that field*. The next execution
//! compares one map pointer and, on a match, decodes a Smi handler and reads
//! one word. No search.
//!
//! The lattice
//! -----------
//! A site moves one way only, which is what makes it terminate:
//!
//! ```text
//!   UNINITIALIZED  --first miss-->  MONOMORPHIC
//!                                       |
//!                            a second shape appears
//!                                       v
//!                                  POLYMORPHIC  (up to 4 shapes)
//!                                       |
//!                              a fifth shape appears
//!                                       v
//!                                  MEGAMORPHIC  (stop caching)
//!   ```
//!
//! Going megamorphic is not a failure to be avoided at runtime -- it is the
//! engine deciding that the cache costs more than it saves, and that is
//! exactly what a site reached with many different shapes should do.
//!
//! Deviations, tracked in `PORTING.md`: no `RECOMPUTE_HANDLER` (it exists for
//! prototype-chain invalidation via validity cells), no MEGADOM, no
//! stub-cache -- V8 has a global hash table of (map, name) -> handler that a
//! megamorphic site consults before giving up entirely. A property found on
//! a prototype is not cached at all, because that needs the object-form
//! handler.

use crate::common::globals::{InlineCacheState, K_MAX_POLYMORPHIC_MAP_COUNT, K_TAGGED_SIZE};
use crate::execution::isolate::Isolate;
use crate::ic::handler_configuration::{Handler, HandlerKind};
use crate::objects::feedback_vector::{
    FeedbackNexus, FeedbackSlotKind, FeedbackVector, MapAndHandler,
};
use crate::objects::fixed_array::WeakFixedArray;
use crate::objects::js_objects::JSObject;
use crate::objects::map::Map;
use crate::objects::property_details::PropertyLocation;
use crate::objects::tagged::Tagged;

/// How a site was resolved, for `--trace-ic` and for the tests that prove the
/// cache is actually being used.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct ICStatistics {
    /// The cached handler answered.
    pub hits: usize,
    /// The site had to do the full lookup, and then recorded what it found.
    pub misses: usize,
    /// The site gave up on caching.
    pub megamorphic_transitions: usize,
    pub monomorphic_transitions: usize,
    pub polymorphic_transitions: usize,
}

impl Isolate {
    /// The sentinel a slot holds before anything has executed.
    ///
    /// V8 uses a `Symbol`; `vx` has no symbols, so it uses the oddball that
    /// already means exactly this.
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

    /// Every transition logged while `trace_ic_enabled` was set.
    pub fn ic_transition_log(&self) -> &[String] {
        &self.ic_trace
    }

    /// `FeedbackNexus::ic_state`, with the sentinels supplied.
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

    /// `LoadIC::Load`: read `name` from `receiver`, using and updating the
    /// cache at `slot`.
    pub fn load_ic(
        &mut self,
        receiver: JSObject,
        name: Tagged,
        vector: FeedbackVector,
        slot: i32,
    ) -> Tagged {
        let nexus = FeedbackNexus::new(&self.heap, vector, slot);
        debug_assert!(nexus.kind().is_load_ic() || nexus.kind().is_global_ic());
        let map = receiver.map(&self.heap);

        // The fast path: one map comparison, then decode and read.
        if let Some(cached) = nexus.find_handler_for_map(
            &self.heap,
            map,
            self.uninitialized_sentinel(),
            self.megamorphic_sentinel(),
        ) {
            if let Some(value) = self.load_from_handler(receiver, cached) {
                self.ic_statistics.hits += 1;
                return value;
            }
        }

        // The slow path: the full lookup, then record what it found.
        self.ic_statistics.misses += 1;
        let found = self.lookup_property(receiver, name);
        let value = match &found {
            Some(found) => found
                .holder
                .fast_property_at(&self.heap, found.details.field_index() as i32),
            None => self.undefined_value(),
        };

        // Only an own data property in a field can be described by a Smi
        // handler. A miss on the prototype chain, or a missing property, is
        // left uncached -- V8 caches both, with handlers `vx` does not have.
        if let Some(found) = found {
            if found.holder == receiver && found.details.location() == PropertyLocation::Field {
                let handler = self.handler_for_field(map, found.details);
                self.update_ic(nexus, map, handler.as_smi());
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
                self.ic_statistics.hits += 1;
                return true;
            }
        }

        self.ic_statistics.misses += 1;
        let existing = self.lookup_own_property(receiver, name);
        let stored = self.set_property(receiver, name, value);

        // Only a store to a property that *already existed* is cacheable. A
        // store that adds one changes the receiver's map, so caching the old
        // map would make the next object of that shape take a handler whose
        // field does not exist yet. V8 caches those too, with a transition
        // handler that performs the map change.
        if stored {
            if let Some(existing) = existing {
                if existing.details.location() == PropertyLocation::Field {
                    let handler = self.handler_for_field(map, existing.details);
                    self.update_ic(nexus, map, handler.as_smi());
                }
            }
        }
        stored
    }

    /// Builds the Smi handler for a data property in a field.
    fn handler_for_field(
        &self,
        map: Map,
        details: crate::objects::property_details::PropertyDetails,
    ) -> Handler {
        let field_index = details.field_index();
        let inobject_count = map.inobject_properties(&self.heap) as u32;
        if field_index < inobject_count {
            // The offset the interpreter will read, in words from the start
            // of the object.
            let offset_in_words = (map.inobject_property_offset(&self.heap, field_index as i32)
                / K_TAGGED_SIZE) as u32;
            Handler::field(offset_in_words, true, false, field_index)
        } else {
            Handler::field(field_index - inobject_count, false, false, field_index)
        }
    }

    /// Completes a load from a cached handler. `None` if the handler is not
    /// one `vx` can execute, which sends the caller down the slow path.
    fn load_from_handler(&self, receiver: JSObject, handler: Tagged) -> Option<Tagged> {
        let handler = Handler::from_tagged(handler)?;
        if handler.kind() != HandlerKind::Field {
            return None;
        }
        Some(self.read_field(receiver, handler))
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

    /// The read the whole scheme exists to make cheap: one indexed load at an
    /// offset the handler already knows.
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
    // The state transitions
    // -----------------------------------------------------------------------

    /// `IC::UpdateCaches`: move the site one step along the lattice.
    fn update_ic(&mut self, nexus: FeedbackNexus, map: Map, handler: Tagged) {
        let before = self.ic_state(nexus);
        self.update_ic_state(nexus, map, handler);
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

    fn update_ic_state(&mut self, nexus: FeedbackNexus, map: Map, handler: Tagged) {
        match self.ic_state(nexus) {
            InlineCacheState::Uninitialized => {
                // `IC::ConfigureVectorState(MONOMORPHIC)`
                nexus.configure_monomorphic(&mut self.heap, map, handler);
                self.ic_statistics.monomorphic_transitions += 1;
            }
            InlineCacheState::Monomorphic => {
                self.update_polymorphic_ic(nexus, map, handler);
            }
            InlineCacheState::Polymorphic => {
                self.update_polymorphic_ic(nexus, map, handler);
            }
            // Megamorphic is terminal: a site that has given up stays given
            // up, which is what stops the state from oscillating.
            InlineCacheState::Megamorphic
            | InlineCacheState::Generic
            | InlineCacheState::NoFeedback => {}
        }
    }

    /// `IC::UpdatePolymorphicIC`: add a (map, handler) pair, or go
    /// megamorphic if there are already too many.
    fn update_polymorphic_ic(&mut self, nexus: FeedbackNexus, map: Map, handler: Tagged) {
        let mut pairs = nexus.extract_maps_and_handlers(
            &self.heap,
            self.uninitialized_sentinel(),
            self.megamorphic_sentinel(),
        );

        // Already recorded with the same handler: the miss came from
        // somewhere this cache cannot help with, so do not keep growing.
        if let Some(existing) = pairs.iter_mut().find(|pair| pair.map == map) {
            if existing.handler == handler {
                return;
            }
            // The same shape with a different handler means the prototype
            // chain changed under us. V8 recomputes the handler; `vx` just
            // overwrites it.
            existing.handler = handler;
        } else {
            if pairs.len() >= K_MAX_POLYMORPHIC_MAP_COUNT {
                let sentinel = self.megamorphic_sentinel();
                nexus.configure_megamorphic(&mut self.heap, sentinel);
                self.ic_statistics.megamorphic_transitions += 1;
                return;
            }
            pairs.push(MapAndHandler { map, handler });
        }

        if pairs.len() == 1 {
            nexus.configure_monomorphic(&mut self.heap, pairs[0].map, pairs[0].handler);
            return;
        }
        self.configure_polymorphic(nexus, &pairs);
        self.ic_statistics.polymorphic_transitions += 1;
    }

    /// `FeedbackNexus::ConfigurePolymorphic`: the pairs go in a
    /// `WeakFixedArray`, maps weak so a dead shape is not kept alive.
    fn configure_polymorphic(&mut self, nexus: FeedbackNexus, pairs: &[MapAndHandler]) {
        let length = (pairs.len() * 2) as i32;
        let array = self.new_weak_fixed_array(length);
        for (index, pair) in pairs.iter().enumerate() {
            let slot = (index * 2) as i32;
            array.set(&mut self.heap, slot, pair.map.object().tagged_weak());
            array.set(&mut self.heap, slot + 1, pair.handler);
        }
        nexus.set_feedback(&mut self.heap, array.tagged());
        // The extra slot is unused in the polymorphic state; V8 parks the
        // name there for keyed sites and the sentinel otherwise.
        let sentinel = self.uninitialized_sentinel();
        nexus.set_feedback_extra(&mut self.heap, sentinel);
    }

    /// How many shapes a site is currently tracking. For `--trace-ic` and
    /// for the tests.
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

    /// A one-line description of a site, in the spirit of `--trace-ic`.
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
        let object = self.heap.allocate_raw_or_fail(
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

/// `FeedbackSlotKind` of the site a bytecode's slot belongs to, for the
/// assertions above.
pub fn expected_slot_kind_for_load() -> FeedbackSlotKind {
    FeedbackSlotKind::LoadProperty
}
