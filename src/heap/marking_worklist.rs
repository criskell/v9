use crate::objects::tagged::HeapObject;

#[derive(Default)]
pub struct MarkingWorklist {
    shared: Vec<HeapObject>,
}

impl MarkingWorklist {
    pub fn new() -> MarkingWorklist {
        MarkingWorklist::default()
    }

    #[inline]
    pub fn push(&mut self, object: HeapObject) {
        self.shared.push(object);
    }

    #[inline]
    pub fn pop(&mut self) -> Option<HeapObject> {
        self.shared.pop()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.shared.is_empty()
    }
}
