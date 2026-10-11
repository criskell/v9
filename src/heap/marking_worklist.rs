use crate::objects::tagged::HeapObject;

/// `MarkingWorklists::Local`
#[derive(Default)]
pub struct MarkingWorklist {
    shared: Vec<HeapObject>,
}

impl MarkingWorklist {
    pub fn new() -> MarkingWorklist {
        MarkingWorklist::default()
    }

    /// `MarkingWorklists::Local::Push`
    #[inline]
    pub fn push(&mut self, object: HeapObject) {
        self.shared.push(object);
    }

    /// `MarkingWorklists::Local::Pop`
    #[inline]
    pub fn pop(&mut self) -> Option<HeapObject> {
        self.shared.pop()
    }

    /// `MarkingWorklists::Local::IsEmpty`
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.shared.is_empty()
    }
}
