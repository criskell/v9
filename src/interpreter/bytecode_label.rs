/// `BytecodeLabel::kInvalidOffset`
const K_INVALID_OFFSET: usize = usize::MAX;

#[derive(Copy, Clone, Debug, Default)]
pub struct BytecodeLoopHeader {
    offset: Option<usize>,
}

impl BytecodeLoopHeader {
    pub fn new() -> BytecodeLoopHeader {
        BytecodeLoopHeader { offset: None }
    }

    /// `BytecodeLoopHeader::offset`
    pub fn offset(&self) -> usize {
        self.offset.expect("the loop header has not been bound")
    }

    pub fn is_bound(&self) -> bool {
        self.offset.is_some()
    }

    /// `BytecodeLoopHeader::bind_to`
    pub(crate) fn bind_to(&mut self, offset: usize) {
        debug_assert!(self.offset.is_none());
        self.offset = Some(offset);
    }
}

#[derive(Copy, Clone, Debug)]
pub struct BytecodeLabel {
    bound: bool,
    jump_offset: usize,
}

impl Default for BytecodeLabel {
    fn default() -> BytecodeLabel {
        BytecodeLabel::new()
    }
}

impl BytecodeLabel {
    pub fn new() -> BytecodeLabel {
        BytecodeLabel {
            bound: false,
            jump_offset: K_INVALID_OFFSET,
        }
    }

    /// `BytecodeLabel::is_bound`
    #[inline]
    pub fn is_bound(&self) -> bool {
        self.bound
    }

    /// `BytecodeLabel::jump_offset`
    #[inline]
    pub fn jump_offset(&self) -> usize {
        debug_assert!(self.has_referrer_jump());
        self.jump_offset
    }

    /// `BytecodeLabel::has_referrer_jump`
    #[inline]
    pub fn has_referrer_jump(&self) -> bool {
        self.jump_offset != K_INVALID_OFFSET
    }

    /// `BytecodeLabel::bind`
    pub(crate) fn bind(&mut self) {
        debug_assert!(!self.bound);
        self.bound = true;
    }

    /// `BytecodeLabel::set_referrer`
    pub(crate) fn set_referrer(&mut self, offset: usize) {
        debug_assert!(!self.bound);
        debug_assert!(!self.has_referrer_jump());
        self.jump_offset = offset;
    }
}

#[derive(Default, Debug)]
pub struct BytecodeLabels {
    labels: Vec<BytecodeLabel>,
    is_bound: bool,
}

impl BytecodeLabels {
    pub fn new() -> BytecodeLabels {
        BytecodeLabels::default()
    }

    pub fn new_label(&mut self) -> usize {
        debug_assert!(!self.is_bound);
        self.labels.push(BytecodeLabel::new());
        self.labels.len() - 1
    }

    pub fn label_mut(&mut self, index: usize) -> &mut BytecodeLabel {
        &mut self.labels[index]
    }

    pub fn label(&self, index: usize) -> &BytecodeLabel {
        &self.labels[index]
    }

    /// `BytecodeLabels::is_bound`
    pub fn is_bound(&self) -> bool {
        self.is_bound
    }

    /// `BytecodeLabels::empty`
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }

    pub fn len(&self) -> usize {
        self.labels.len()
    }

    pub(crate) fn mark_bound(&mut self) {
        self.is_bound = true;
    }

    pub fn pending(&self) -> Vec<usize> {
        (0..self.labels.len())
            .filter(|index| {
                let label = &self.labels[*index];
                label.has_referrer_jump() && !label.is_bound()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_label_is_unbound_and_has_no_jump() {
        let label = BytecodeLabel::new();
        assert!(!label.is_bound());
        assert!(!label.has_referrer_jump());
    }

    #[test]
    fn a_label_records_its_jump_and_then_binds() {
        let mut label = BytecodeLabel::new();
        label.set_referrer(12);
        assert!(label.has_referrer_jump());
        assert_eq!(label.jump_offset(), 12);
        assert!(!label.is_bound());
        label.bind();
        assert!(label.is_bound());
    }

    #[test]
    #[should_panic(expected = "has_referrer_jump")]
    fn a_label_refuses_a_second_jump() {
        let mut label = BytecodeLabel::new();
        label.set_referrer(1);
        label.set_referrer(2);
    }

    #[test]
    #[should_panic(expected = "self.bound")]
    fn a_label_refuses_a_second_bind() {
        let mut label = BytecodeLabel::new();
        label.bind();
        label.bind();
    }

    #[test]
    fn several_jumps_to_one_target_use_a_label_set() {
        let mut labels = BytecodeLabels::new();
        let first = labels.new_label();
        let second = labels.new_label();
        labels.label_mut(first).set_referrer(4);
        labels.label_mut(second).set_referrer(9);

        assert_eq!(labels.len(), 2);
        assert!(!labels.is_empty());
        assert_eq!(labels.pending(), vec![first, second]);

        labels.label_mut(first).bind();
        assert_eq!(labels.pending(), vec![second]);
    }

    #[test]
    fn a_loop_header_binds_before_it_is_jumped_to() {
        let mut header = BytecodeLoopHeader::new();
        assert!(!header.is_bound());
        header.bind_to(7);
        assert!(header.is_bound());
        assert_eq!(header.offset(), 7);
    }

    #[test]
    #[should_panic(expected = "not been bound")]
    fn reading_an_unbound_loop_header_is_a_bug() {
        BytecodeLoopHeader::new().offset();
    }
}
