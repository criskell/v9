use crate::execution::isolate::Isolate;
use crate::objects::tagged::{HeapObject, Tagged};

pub struct HandleScope {
    prev_next: usize,
}

impl Isolate {
    #[must_use]
    pub(crate) fn handle_scope(&self) -> HandleScope {
        HandleScope {
            prev_next: self.handles.len(),
        }
    }

    pub(crate) fn close_handle_scope(&mut self, scope: HandleScope) {
        self.handles.truncate(scope.prev_next);
    }

    pub(crate) fn create_handle(&mut self, value: Tagged) -> Handle {
        self.handles.push(value);
        Handle {
            index: self.handles.len() - 1,
        }
    }

    pub(crate) fn create_object_handle(&mut self, object: HeapObject) -> Handle {
        self.create_handle(object.tagged())
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Handle {
    index: usize,
}

impl Handle {
    #[inline]
    pub(crate) fn get(self, isolate: &Isolate) -> Tagged {
        isolate.handles[self.index]
    }

    #[inline]
    pub(crate) fn index(self) -> usize {
        self.index
    }

    #[inline]
    pub(crate) fn object(self, isolate: &Isolate) -> HeapObject {
        self.get(isolate)
            .to_strong_heap_object()
            .expect("a handle to a heap object")
    }
}
