use crate::execution::isolate::Isolate;
use crate::heap::heap::AllocationType;
use crate::objects::tagged::HeapObject;

#[derive(Default)]
pub struct HeapAllocator {
    allocation_gc_interval: Option<i32>,
    allocation_timeout: Option<i32>,
    always_allocate_scope_count: usize,
}

impl HeapAllocator {
    pub fn new() -> HeapAllocator {
        let mut allocator = HeapAllocator::default();
        if let Ok(value) = std::env::var("VX_GC_INTERVAL") {
            if let Ok(interval) = value.parse::<i32>() {
                allocator.set_allocation_gc_interval(interval);
            }
        }
        allocator
    }

    pub fn set_allocation_gc_interval(&mut self, interval: i32) {
        self.allocation_gc_interval = if interval > 0 { Some(interval) } else { None };
        self.update_allocation_timeout();
    }

    pub fn update_allocation_timeout(&mut self) {
        self.allocation_timeout = self.allocation_gc_interval;
    }

    fn reached_allocation_timeout(&mut self) -> bool {
        if self.always_allocate() {
            return false;
        }
        let Some(timeout) = self.allocation_timeout else {
            return false;
        };
        let timeout = 0.max(timeout - 1);
        self.allocation_timeout = Some(timeout);
        timeout <= 0
    }

    #[inline]
    pub fn always_allocate(&self) -> bool {
        self.always_allocate_scope_count != 0
    }
}

impl Isolate {
    pub(crate) fn enter_always_allocate_scope(&mut self) {
        self.heap_allocator.always_allocate_scope_count += 1;
    }

    pub(crate) fn leave_always_allocate_scope(&mut self) {
        self.heap_allocator.always_allocate_scope_count -= 1;
    }

    pub fn set_allocation_gc_interval(&mut self, interval: i32) {
        self.heap_allocator.set_allocation_gc_interval(interval);
    }

    pub fn allocate_raw_with_retry_or_fail(
        &mut self,
        size_in_bytes: usize,
        allocation: AllocationType,
    ) -> HeapObject {
        if self.heap_allocator.reached_allocation_timeout() {
            self.heap_allocator.update_allocation_timeout();
            self.collect_garbage_for_allocation(allocation);
        }
        if let Some(object) = self.heap.allocate_raw(size_in_bytes, allocation) {
            return object;
        }
        self.retry_allocate_or_fail(size_in_bytes, allocation)
    }

    fn retry_allocate_or_fail(
        &mut self,
        size_in_bytes: usize,
        allocation: AllocationType,
    ) -> HeapObject {
        for _ in 0..2 {
            self.collect_garbage_for_allocation(allocation);
            if let Some(object) = self.heap.allocate_raw(size_in_bytes, allocation) {
                return object;
            }
        }
        // In the case of young allocations, the GCs above were minor GCs. Try
        // "light" full GCs before performing the last-resort GCs.
        if allocation == AllocationType::Young {
            self.collect_all_garbage();
            if let Some(object) = self.heap.allocate_raw(size_in_bytes, allocation) {
                return object;
            }
        }
        self.heap.allocate_raw_or_fail(size_in_bytes, allocation)
    }

    fn collect_garbage_for_allocation(&mut self, allocation: AllocationType) {
        match allocation {
            AllocationType::Young => {
                self.collect_garbage();
            }
            AllocationType::Old => {
                self.collect_all_garbage();
            }
            AllocationType::Map | AllocationType::ReadOnly => {}
        }
    }
}
