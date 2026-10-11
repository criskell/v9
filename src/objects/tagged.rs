use crate::common::globals::*;

pub type Address = u64;

pub const NULL_ADDRESS: Address = 0;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Tagged(pub u64);

impl Tagged {
    pub const ZERO: Tagged = Tagged(0);

    #[inline]
    pub const fn ptr(self) -> u64 {
        self.0
    }

    // -- Smi ---------------------------------------------------------------

    /// `TaggedImpl::IsSmi`
    #[inline]
    pub const fn is_smi(self) -> bool {
        self.0 & K_SMI_TAG_MASK == K_SMI_TAG
    }

    // -- Strong / weak -------------------------------------------------------

    #[inline]
    pub const fn is_strong(self) -> bool {
        self.0 & K_HEAP_OBJECT_TAG_MASK == K_HEAP_OBJECT_TAG
    }

    #[inline]
    pub const fn is_heap_object(self) -> bool {
        self.is_strong()
    }

    #[inline]
    pub const fn is_object(self) -> bool {
        !self.is_weak_or_cleared()
    }

    /// `TaggedImpl::IsStrongOrSmi`
    #[inline]
    pub const fn is_strong_or_smi(self) -> bool {
        self.0 & K_HEAP_OBJECT_TAG_MASK != K_WEAK_HEAP_OBJECT_TAG
    }

    #[inline]
    pub const fn is_strong_or_weak(self) -> bool {
        !self.is_smi() && !self.is_cleared()
    }

    #[inline]
    pub const fn is_weak(self) -> bool {
        self.0 & K_HEAP_OBJECT_TAG_MASK == K_WEAK_HEAP_OBJECT_TAG && !self.is_cleared()
    }

    /// `TaggedImpl::IsCleared`
    #[inline]
    pub const fn is_cleared(self) -> bool {
        self.0 as u32 == K_CLEARED_WEAK_HEAP_OBJECT_LOWER32
    }

    /// `TaggedImpl::IsWeakOrCleared`
    #[inline]
    pub const fn is_weak_or_cleared(self) -> bool {
        self.0 & K_HEAP_OBJECT_TAG_MASK == K_WEAK_HEAP_OBJECT_TAG
    }

    /// `ClearedValue`
    #[inline]
    pub const fn cleared() -> Tagged {
        Tagged(K_CLEARED_WEAK_HEAP_OBJECT_LOWER32 as u64)
    }

    // -- Conversions --------------------------------------------------------

    #[inline]
    pub const fn to_heap_object(self) -> Option<HeapObject> {
        if self.is_smi() || self.is_cleared() {
            None
        } else {
            Some(HeapObject(self.0 & !K_HEAP_OBJECT_TAG_MASK))
        }
    }

    /// `TaggedImpl::GetHeapObjectIfStrong`
    #[inline]
    pub const fn to_strong_heap_object(self) -> Option<HeapObject> {
        if self.is_strong() {
            Some(HeapObject(self.0 & !K_HEAP_OBJECT_TAG_MASK))
        } else {
            None
        }
    }

    #[inline]
    pub const fn address_unchecked(self) -> Address {
        self.0 & !K_HEAP_OBJECT_TAG_MASK
    }
}

impl std::fmt::Debug for Tagged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_smi() {
            write!(f, "Smi({})", crate::objects::smi::Smi::to_int(*self))
        } else if self.is_cleared() {
            write!(f, "ClearedWeakRef")
        } else if self.is_weak_or_cleared() {
            write!(f, "Weak(0x{:x})", self.address_unchecked())
        } else {
            write!(f, "HeapObject(0x{:x})", self.address_unchecked())
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HeapObject(Address);

impl HeapObject {
    /// `HeapObject::FromAddress`
    #[inline]
    pub const fn from_address(address: Address) -> HeapObject {
        debug_assert!(is_aligned(address, K_OBJECT_ALIGNMENT));
        HeapObject(address)
    }

    /// `HeapObject::address`
    #[inline]
    pub const fn address(self) -> Address {
        self.0
    }

    #[inline]
    pub const fn tagged(self) -> Tagged {
        Tagged(self.0 | K_HEAP_OBJECT_TAG)
    }

    #[inline]
    pub const fn tagged_weak(self) -> Tagged {
        Tagged(self.0 | K_WEAK_HEAP_OBJECT_TAG)
    }

    #[inline]
    pub const fn field_address(self, offset: i32) -> Address {
        self.0 + offset as u64
    }
}

impl std::fmt::Debug for HeapObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HeapObject(0x{:x})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::smi::Smi;

    #[test]
    fn strong_reference_roundtrip() {
        let obj = HeapObject::from_address(0x1_0008);
        let t = obj.tagged();
        assert!(t.is_heap_object());
        assert!(t.is_strong());
        assert!(t.is_object());
        assert!(t.is_strong_or_smi());
        assert!(!t.is_smi());
        assert!(!t.is_weak());
        assert!(!t.is_weak_or_cleared());
        assert_eq!(t.to_heap_object(), Some(obj));
        assert_eq!(t.to_strong_heap_object(), Some(obj));
    }

    #[test]
    fn weak_reference_roundtrip() {
        let obj = HeapObject::from_address(0x1_0008);
        let t = obj.tagged_weak();
        assert!(t.is_weak());
        assert!(t.is_weak_or_cleared());
        assert!(!t.is_object());
        assert!(!t.is_strong_or_smi());
        assert!(t.is_strong_or_weak());
        assert!(
            !t.is_heap_object(),
            "a weak ref is not a strong heap object"
        );
        assert!(!t.is_smi());
        assert!(!t.is_cleared());
        assert_eq!(t.to_heap_object(), Some(obj));
        assert_eq!(t.to_strong_heap_object(), None);
    }

    #[test]
    fn cleared_weak_reference() {
        let c = Tagged::cleared();
        assert!(c.is_cleared());
        assert!(c.is_weak_or_cleared());
        assert!(!c.is_weak(), "a cleared ref is not a live weak ref");
        assert_eq!(c.to_heap_object(), None);
    }

    #[test]
    fn smi_is_not_a_reference() {
        for v in [0i32, 1, -1, i32::MAX, i32::MIN] {
            let t = Smi::from_int(v);
            assert!(t.is_smi());
            assert!(t.is_object());
            assert!(t.is_strong_or_smi());
            assert!(!t.is_strong());
            assert!(!t.is_heap_object());
            assert!(!t.is_weak_or_cleared());
            assert_eq!(t.to_heap_object(), None);
        }
    }

    #[test]
    fn zeroed_memory_reads_back_as_smi_zero() {
        assert!(Tagged::ZERO.is_smi());
        assert_eq!(Smi::to_int(Tagged::ZERO), 0);
    }
}
