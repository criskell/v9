#![allow(non_upper_case_globals)]

pub enum Distance {
    Near,
    Far,
}

#[derive(Default)]
pub struct Label {
    pos: i32,
    near_link_pos: i32,
}

impl Label {
    pub fn new() -> Label {
        Label {
            pos: 0,
            near_link_pos: 0,
        }
    }

    #[inline]
    pub fn unuse(&mut self) {
        self.pos = 0;
    }

    #[inline]
    pub fn unuse_near(&mut self) {
        self.near_link_pos = 0;
    }

    #[inline]
    pub fn is_bound(&self) -> bool {
        self.pos < 0
    }

    #[inline]
    pub fn is_unused(&self) -> bool {
        self.pos == 0 && self.near_link_pos == 0
    }

    #[inline]
    pub fn is_linked(&self) -> bool {
        self.pos > 0
    }

    #[inline]
    pub fn is_near_linked(&self) -> bool {
        self.near_link_pos > 0
    }

    pub fn pos(&self) -> i32 {
        if self.pos < 0 {
            return -self.pos - 1;
        }
        if self.pos > 0 {
            return self.pos - 1;
        }
        unreachable!()
    }

    pub fn near_link_pos(&self) -> i32 {
        self.near_link_pos - 1
    }

    pub(crate) fn bind_to(&mut self, pos: i32) {
        self.pos = -pos - 1;
        debug_assert!(self.is_bound());
    }

    pub(crate) fn link_to(&mut self, pos: i32, distance: Distance) {
        match distance {
            Distance::Near => {
                self.near_link_pos = pos + 1;
                debug_assert!(self.is_near_linked());
            }
            Distance::Far => {
                self.pos = pos + 1;
                debug_assert!(self.is_linked());
            }
        }
    }
}
