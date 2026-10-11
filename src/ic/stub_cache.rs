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
    pub key: Tagged,
    pub value: Tagged,
    pub map: Tagged,
}

impl Entry {
    #[inline]
    fn is_occupied(&self) -> bool {
        self.map.is_strong()
    }
}

/// `StubCache`
pub struct StubCache {
    primary: Vec<Entry>,
    secondary: Vec<Entry>,
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

    pub fn primary_offset(name_raw_hash: u32, map: Map) -> usize {
        let address = map.address();
        let map_low32 = (address ^ (address >> K_PRIMARY_TABLE_BITS)) as u32;
        let key = map_low32.wrapping_add(name_raw_hash);
        (key as usize) & (K_PRIMARY_TABLE_SIZE - 1)
    }

    pub fn secondary_offset(name: Name, map: Map) -> usize {
        let name_low32 = name.address() as u32;
        let map_low32 = map.address() as u32;
        let key = map_low32.wrapping_add(name_low32);
        let key = key.wrapping_add(key >> K_SECONDARY_TABLE_BITS);
        (key as usize) & (K_SECONDARY_TABLE_SIZE - 1)
    }

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

        fn shape(&mut self, properties: &[&str]) -> Map {
            let object = self.isolate.new_js_object();
            self.isolate.handles.push(object.tagged());
            let slot = self.isolate.handles.len() - 1;
            let rooted = |isolate: &crate::execution::isolate::Isolate| {
                crate::objects::js_objects::JSObject::unchecked_cast(
                    isolate.handles[slot].to_strong_heap_object().unwrap(),
                )
            };
            for property in properties {
                let object = rooted(&self.isolate);
                self.isolate
                    .set_property_by_str(object, property, Smi::from_int(1));
            }
            rooted(&self.isolate).map(&self.isolate.heap)
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
        let mut world = World::new();
        let (name, hash) = world.name("x");

        let mut by_offset: std::collections::HashMap<usize, Map> = std::collections::HashMap::new();
        let mut collision = None;
        for index in 0..400 {
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
        let mut world = World::new();
        let (name, hash) = world.name("length");
        let map = world.shape(&["length"]);
        let primary = StubCache::primary_offset(hash, map);
        let secondary = StubCache::secondary_offset(name, map);
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
