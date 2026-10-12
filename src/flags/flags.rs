use std::sync::atomic::{AtomicBool, Ordering};

pub struct FlagValues {
    pub allow_natives_syntax: AtomicBool,
}

impl FlagValues {
    pub fn allow_natives_syntax(&self) -> bool {
        self.allow_natives_syntax.load(Ordering::Relaxed)
    }

    pub fn set_allow_natives_syntax(&self, value: bool) {
        self.allow_natives_syntax.store(value, Ordering::Relaxed);
    }
}

pub static V8_FLAGS: FlagValues = FlagValues {
    allow_natives_syntax: AtomicBool::new(false),
};
