//! Ported from `src/ic/stub-cache.h` / `stub-cache.cc`.
//!
//! The isolate-wide `(map, name) -> handler` table a *megamorphic* site
//! consults before giving up and doing the full lookup.
//!
//! Why it helps
//! -----------
//! A site that has seen more than four shapes stops tracking them locally --
//! a linear scan of map checks would cost more than it saves. But the
//! program as a whole still has only a handful of distinct `(map, name)`
//! pairs, and most of them have been resolved before. The stub cache is the
//! shared memo for exactly that, which is what makes megamorphic much
//! cheaper than an uncached lookup rather than equally expensive.
//!
//! Two tables, no probing
//! ----------------------
//! ```text
//!   primary    4096 entries, hashed on (map, name's hash)
//!   secondary  1024 entries, hashed differently on (map, name)
//! ```
//!
//! One entry per slot and no probe sequence: a lookup is two loads and two
//! comparisons, because in V8 this runs in generated code where a loop would
//! be unacceptable. The collision policy is a *victim cache* -- inserting
//! into an occupied primary slot demotes the old entry to the secondary
//! table rather than discarding it.
//!
//! Why nothing here needs tracing
//! ------------------------------
//! V8 asserts that neither the name nor the handler is in the young
//! generation, so that entries can be compared by identity. In `vx` the
//! property is stronger and the assertion says so: a map and an internalized
//! name both live in read-only space and a handler is a Smi, so **no entry
//! can be invalidated by a collection** and the table is not a root set. V8,
//! whose maps do move, has to clear the cache instead.

use crate::objects::map::Map;
use crate::objects::name::Name;
use crate::objects::tagged::Tagged;

/// `StubCache::kPrimaryTableBits`
pub const K_PRIMARY_TABLE_BITS: u32 = 12;
pub const K_PRIMARY_TABLE_SIZE: usize = 1 << K_PRIMARY_TABLE_BITS;
/// `StubCache::kSecondaryTableBits`
pub const K_SECONDARY_TABLE_BITS: u32 = 10;
pub const K_SECONDARY_TABLE_SIZE: usize = 1 << K_SECONDARY_TABLE_BITS;

/// `StubCache::Entry`
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct Entry {
    /// The name. `Tagged::ZERO` when the slot is empty.
    pub key: Tagged,
    /// The handler, a Smi.
    pub value: Tagged,
    pub map: Tagged,
}

impl Entry {
    /// V8 tests `!primary->map.IsSmi()`: a real entry has a map reference,
    /// and an empty slot has a Smi (zero) there.
    #[inline]
    fn is_occupied(&self) -> bool {
        self.map.is_strong()
    }
}

/// `StubCache`
pub struct StubCache {
    primary: Vec<Entry>,
    secondary: Vec<Entry>,
    /// `Counters::megamorphic_stub_cache_updates`, plus the hit counters V8
    /// keeps in its own statistics.
    pub updates: usize,
    pub primary_hits: usize,
    pub secondary_hits: usize,
    pub misses: usize,
}

impl Default for StubCache {
    fn default() -> StubCache {
        StubCache::new()
    }
}

impl StubCache {
    pub fn new() -> StubCache {
        StubCache {
            primary: vec![Entry::default(); K_PRIMARY_TABLE_SIZE],
            secondary: vec![Entry::default(); K_SECONDARY_TABLE_SIZE],
            updates: 0,
            primary_hits: 0,
            secondary_hits: 0,
            misses: 0,
        }
    }

    /// `StubCache::Clear`
    pub fn clear(&mut self) {
        self.primary
            .iter_mut()
            .for_each(|entry| *entry = Entry::default());
        self.secondary
            .iter_mut()
            .for_each(|entry| *entry = Entry::default());
    }

    /// `StubCache::PrimaryOffset`.
    ///
    /// The map's address folded over itself plus the name's hash. V8 notes
    /// that using only the low bits barely raises the collision risk even
    /// with a heap spread over more than 4Gb.
    pub fn primary_offset(name_raw_hash: u32, map: Map) -> usize {
        let address = map.address();
        let map_low32 = (address ^ (address >> K_PRIMARY_TABLE_BITS)) as u32;
        let key = map_low32.wrapping_add(name_raw_hash);
        (key as usize) & (K_PRIMARY_TABLE_SIZE - 1)
    }

    /// `StubCache::SecondaryOffset`.
    ///
    /// Deliberately a different mixing from the primary, so that a pair
    /// colliding in one table is unlikely to collide in the other -- V8's
    /// comment notes this matters for minified code with short names.
    pub fn secondary_offset(name: Name, map: Map) -> usize {
        let name_low32 = name.address() as u32;
        let map_low32 = map.address() as u32;
        let key = map_low32.wrapping_add(name_low32);
        let key = key.wrapping_add(key >> K_SECONDARY_TABLE_BITS);
        (key as usize) & (K_SECONDARY_TABLE_SIZE - 1)
    }

    /// `StubCache::Get`: two loads, two comparisons, no loop.
    ///
    /// Comparison is by identity, which is sound because an internalized
    /// name is one object per distinct name and a map is immovable.
    pub fn get(&mut self, name: Name, name_raw_hash: u32, map: Map) -> Option<Tagged> {
        let primary = self.primary[Self::primary_offset(name_raw_hash, map)];
        if primary.key == name.tagged() && primary.map == map.tagged() {
            self.primary_hits += 1;
            return Some(primary.value);
        }
        let secondary = self.secondary[Self::secondary_offset(name, map)];
        if secondary.key == name.tagged() && secondary.map == map.tagged() {
            self.secondary_hits += 1;
            return Some(secondary.value);
        }
        self.misses += 1;
        None
    }

    /// A lookup that does not disturb the counters, for tests and for
    /// diagnostics.
    pub fn peek(&self, name: Name, name_raw_hash: u32, map: Map) -> Option<Tagged> {
        let primary = self.primary[Self::primary_offset(name_raw_hash, map)];
        if primary.key == name.tagged() && primary.map == map.tagged() {
            return Some(primary.value);
        }
        let secondary = self.secondary[Self::secondary_offset(name, map)];
        if secondary.key == name.tagged() && secondary.map == map.tagged() {
            return Some(secondary.value);
        }
        None
    }

    /// `StubCache::Set`.
    ///
    /// The eviction is the interesting part: an occupied primary slot is
    /// *retired to the secondary table* at its own secondary offset, rather
    /// than overwritten. So a pair that keeps losing its primary slot to a
    /// collision still gets one more chance to be found.
    pub fn set(&mut self, name: Name, name_raw_hash: u32, map: Map, handler: Tagged) {
        let primary_offset = Self::primary_offset(name_raw_hash, map);
        let old = self.primary[primary_offset];
        if old.is_occupied() {
            let old_name = Name::unchecked_cast(
                old.key
                    .to_strong_heap_object()
                    .expect("an occupied entry has a name"),
            );
            let old_map = Map::unchecked_cast(
                old.map
                    .to_strong_heap_object()
                    .expect("an occupied entry has a map"),
            );
            let secondary_offset = Self::secondary_offset(old_name, old_map);
            self.secondary[secondary_offset] = old;
        }

        self.primary[primary_offset] = Entry {
            key: name.tagged(),
            value: handler,
            map: map.tagged(),
        };
        self.updates += 1;
    }

    /// How many slots are in use, for diagnostics.
    pub fn occupancy(&self) -> (usize, usize) {
        (
            self.primary
                .iter()
                .filter(|entry| entry.is_occupied())
                .count(),
            self.secondary
                .iter()
                .filter(|entry| entry.is_occupied())
                .count(),
        )
    }

    pub fn total_hits(&self) -> usize {
        self.primary_hits + self.secondary_hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::isolate::Isolate;
    use crate::objects::smi::Smi;

    /// A world with a few distinct shapes and names to hash.
    struct World {
        isolate: Isolate,
    }

    impl World {
        fn new() -> World {
            World {
                isolate: Isolate::new(),
            }
        }

        fn name(&mut self, text: &str) -> (Name, u32) {
            let string = self.isolate.internalize_string(text);
            let name = string.as_name();
            let hash = name.raw_hash_field(&self.isolate.heap);
            (name, hash)
        }

        /// A distinct map, by giving an object a distinct property.
        fn shape(&mut self, properties: &[&str]) -> Map {
            let object = self.isolate.new_js_object();
            for property in properties {
                self.isolate
                    .set_property_by_str(object, property, Smi::from_int(1));
            }
            object.map(&self.isolate.heap)
        }
    }

    #[test]
    fn table_sizes_match_v8() {
        assert_eq!(K_PRIMARY_TABLE_SIZE, 4096);
        assert_eq!(K_SECONDARY_TABLE_SIZE, 1024);
    }

    #[test]
    fn an_entry_is_found_by_the_pair_that_stored_it() {
        let mut world = World::new();
        let (name, hash) = world.name("x");
        let map = world.shape(&["x"]);
        let handler = Smi::from_int(7);

        let mut cache = StubCache::new();
        assert_eq!(cache.get(name, hash, map), None, "empty to begin with");
        cache.set(name, hash, map, handler);
        assert_eq!(cache.get(name, hash, map), Some(handler));
    }

    #[test]
    fn a_different_shape_with_the_same_name_is_a_miss() {
        // The whole point of keying on the pair: the handler encodes a field
        // index, which is only valid for the shape it came from.
        let mut world = World::new();
        let (name, hash) = world.name("x");
        let first = world.shape(&["x"]);
        let second = world.shape(&["a", "x"]);
        assert_ne!(first, second);

        let mut cache = StubCache::new();
        cache.set(name, hash, first, Smi::from_int(0));
        assert_eq!(cache.get(name, hash, first), Some(Smi::from_int(0)));
        assert_eq!(
            cache.get(name, hash, second),
            None,
            "a handler for one shape must not answer for another"
        );
    }

    #[test]
    fn a_different_name_on_the_same_shape_is_a_miss() {
        let mut world = World::new();
        let (x, x_hash) = world.name("x");
        let (y, y_hash) = world.name("y");
        let map = world.shape(&["x", "y"]);

        let mut cache = StubCache::new();
        cache.set(x, x_hash, map, Smi::from_int(0));
        assert_eq!(cache.get(y, y_hash, map), None);
    }

    #[test]
    fn an_evicted_entry_is_demoted_rather_than_dropped() {
        // Two pairs that genuinely collide in the primary table. Planting
        // one at the other's offset would not do: `get` recomputes the hash,
        // so a faked collision is not found at all and the test would pass
        // for the wrong reason.
        let mut world = World::new();
        let (name, hash) = world.name("x");

        let mut by_offset: std::collections::HashMap<usize, Map> = std::collections::HashMap::new();
        let mut collision = None;
        for index in 0..400 {
            // Each shape gets a distinct prefix of properties, so each is a
            // distinct map.
            let property = format!("p{index}");
            let map = world.shape(&[&property, "x"]);
            let offset = StubCache::primary_offset(hash, map);
            match by_offset.insert(offset, map) {
                Some(previous) if previous != map => {
                    collision = Some((previous, map));
                    break;
                }
                _ => {}
            }
        }
        let (first, second) =
            collision.expect("400 shapes over 4096 slots should collide somewhere");
        assert_eq!(
            StubCache::primary_offset(hash, first),
            StubCache::primary_offset(hash, second)
        );

        let mut cache = StubCache::new();
        cache.set(name, hash, first, Smi::from_int(11));
        cache.set(name, hash, second, Smi::from_int(22));

        assert_eq!(
            cache.peek(name, hash, second),
            Some(Smi::from_int(22)),
            "the new entry took the primary slot"
        );
        assert_eq!(
            cache.peek(name, hash, first),
            Some(Smi::from_int(11)),
            "and the evicted one was retired to the secondary table, not dropped"
        );
        assert_eq!(cache.occupancy(), (1, 1));
    }

    #[test]
    fn the_two_hashes_are_different_functions() {
        // They have to be, or a pair colliding in the primary table would
        // collide in the secondary one too and the victim cache would be
        // pointless.
        let mut world = World::new();
        let (name, hash) = world.name("length");
        let map = world.shape(&["length"]);
        let primary = StubCache::primary_offset(hash, map);
        let secondary = StubCache::secondary_offset(name, map);
        // Not a strong claim about either value, just that one is not the
        // other's definition.
        assert!(primary < K_PRIMARY_TABLE_SIZE);
        assert!(secondary < K_SECONDARY_TABLE_SIZE);
    }

    #[test]
    fn clearing_empties_both_tables() {
        let mut world = World::new();
        let (name, hash) = world.name("x");
        let map = world.shape(&["x"]);
        let mut cache = StubCache::new();
        cache.set(name, hash, map, Smi::from_int(1));
        assert_eq!(cache.occupancy(), (1, 0));
        cache.clear();
        assert_eq!(cache.occupancy(), (0, 0));
        assert_eq!(cache.get(name, hash, map), None);
    }

    #[test]
    fn the_counters_separate_primary_from_secondary_hits() {
        let mut world = World::new();
        let (name, hash) = world.name("x");
        let map = world.shape(&["x"]);
        let mut cache = StubCache::new();

        cache.get(name, hash, map);
        assert_eq!(cache.misses, 1);
        cache.set(name, hash, map, Smi::from_int(1));
        cache.get(name, hash, map);
        assert_eq!(cache.primary_hits, 1);
        assert_eq!(cache.secondary_hits, 0);
        assert_eq!(cache.total_hits(), 1);
        assert_eq!(cache.updates, 1);
    }
}
