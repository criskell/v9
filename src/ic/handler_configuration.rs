use crate::objects::property_details::K_DESCRIPTOR_INDEX_BIT_COUNT;
use crate::objects::smi::Smi;
use crate::objects::tagged::Tagged;

/// `LoadHandler::Kind`
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum HandlerKind {
    Normal = 0,
    /// A property of the global object.
    Global = 1,
    Field = 2,
    ConstantFromPrototype = 3,
    NativeDataProperty = 4,
}

impl HandlerKind {
    fn from_u32(value: u32) -> HandlerKind {
        match value {
            1 => HandlerKind::Global,
            2 => HandlerKind::Field,
            3 => HandlerKind::ConstantFromPrototype,
            4 => HandlerKind::NativeDataProperty,
            _ => HandlerKind::Normal,
        }
    }
}

const KIND_SHIFT: u32 = 0;
const KIND_BITS: u32 = 4;
const DO_ACCESS_CHECK_SHIFT: u32 = KIND_SHIFT + KIND_BITS;
const LOOKUP_ON_START_OBJECT_SHIFT: u32 = DO_ACCESS_CHECK_SHIFT + 1;
const STORAGE_OFFSET_SHIFT: u32 = LOOKUP_ON_START_OBJECT_SHIFT + 1;
const STORAGE_OFFSET_BITS: u32 = K_DESCRIPTOR_INDEX_BIT_COUNT + 1;
const IS_INOBJECT_SHIFT: u32 = STORAGE_OFFSET_SHIFT + STORAGE_OFFSET_BITS;
const IS_DOUBLE_SHIFT: u32 = IS_INOBJECT_SHIFT + 1;
const DESCRIPTOR_INDEX_SHIFT: u32 = IS_DOUBLE_SHIFT + 1;
const DESCRIPTOR_INDEX_BITS: u32 = K_DESCRIPTOR_INDEX_BIT_COUNT + 1;

const _: () = assert!(DESCRIPTOR_INDEX_SHIFT + DESCRIPTOR_INDEX_BITS <= 31);

#[inline]
const fn mask(bits: u32) -> u32 {
    (1u32 << bits) - 1
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Handler(u32);

impl Handler {
    /// `LoadHandler::LoadNativeDataProperty`
    pub fn native_data_property(descriptor_index: u32) -> Handler {
        debug_assert!(descriptor_index <= mask(DESCRIPTOR_INDEX_BITS));
        Handler(
            ((HandlerKind::NativeDataProperty as u32) << KIND_SHIFT)
                | (descriptor_index << DESCRIPTOR_INDEX_SHIFT),
        )
    }

    pub fn field(
        storage_offset_in_words: u32,
        is_inobject: bool,
        is_double: bool,
        descriptor_index: u32,
    ) -> Handler {
        debug_assert!(storage_offset_in_words <= mask(STORAGE_OFFSET_BITS));
        debug_assert!(descriptor_index <= mask(DESCRIPTOR_INDEX_BITS));
        Handler(
            ((HandlerKind::Field as u32) << KIND_SHIFT)
                | (storage_offset_in_words << STORAGE_OFFSET_SHIFT)
                | ((is_inobject as u32) << IS_INOBJECT_SHIFT)
                | ((is_double as u32) << IS_DOUBLE_SHIFT)
                | (descriptor_index << DESCRIPTOR_INDEX_SHIFT),
        )
    }

    /// `LoadHandler::LoadGlobal`
    pub fn global(descriptor_index: u32) -> Handler {
        Handler(
            ((HandlerKind::Global as u32) << KIND_SHIFT)
                | (descriptor_index << DESCRIPTOR_INDEX_SHIFT),
        )
    }

    #[inline]
    pub fn kind(self) -> HandlerKind {
        HandlerKind::from_u32((self.0 >> KIND_SHIFT) & mask(KIND_BITS))
    }

    /// `LoadHandler::StorageOffsetInWordsBits::decode`
    #[inline]
    pub fn storage_offset_in_words(self) -> u32 {
        (self.0 >> STORAGE_OFFSET_SHIFT) & mask(STORAGE_OFFSET_BITS)
    }

    #[inline]
    pub fn is_inobject(self) -> bool {
        (self.0 >> IS_INOBJECT_SHIFT) & 1 != 0
    }

    #[inline]
    pub fn is_double(self) -> bool {
        (self.0 >> IS_DOUBLE_SHIFT) & 1 != 0
    }

    #[inline]
    pub fn descriptor_index(self) -> u32 {
        (self.0 >> DESCRIPTOR_INDEX_SHIFT) & mask(DESCRIPTOR_INDEX_BITS)
    }

    #[inline]
    pub fn as_smi(self) -> Tagged {
        Smi::from_int(self.0 as i32)
    }

    #[inline]
    pub fn from_tagged(value: Tagged) -> Option<Handler> {
        if value.is_smi() {
            Some(Handler(Smi::to_int(value) as u32))
        } else {
            None
        }
    }

    #[inline]
    pub fn raw(self) -> u32 {
        self.0
    }
}

impl std::fmt::Debug for Handler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Handler {{ {:?}, offset: {} words, {}, descriptor: {} }}",
            self.kind(),
            self.storage_offset_in_words(),
            if self.is_inobject() {
                "inobject"
            } else {
                "backing store"
            },
            self.descriptor_index()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_handler_round_trips_through_a_smi() {
        let handler = Handler::field(3, true, false, 7);
        let smi = handler.as_smi();
        assert!(smi.is_smi(), "a handler must be free to store");
        let decoded = Handler::from_tagged(smi).unwrap();
        assert_eq!(decoded, handler);
        assert_eq!(decoded.kind(), HandlerKind::Field);
        assert_eq!(decoded.storage_offset_in_words(), 3);
        assert!(decoded.is_inobject());
        assert!(!decoded.is_double());
        assert_eq!(decoded.descriptor_index(), 7);
    }

    #[test]
    fn the_bit_fields_do_not_overlap() {
        let handler = Handler::field(
            mask(STORAGE_OFFSET_BITS),
            true,
            true,
            mask(DESCRIPTOR_INDEX_BITS),
        );
        assert_eq!(handler.kind(), HandlerKind::Field);
        assert_eq!(handler.storage_offset_in_words(), mask(STORAGE_OFFSET_BITS));
        assert!(handler.is_inobject());
        assert!(handler.is_double());
        assert_eq!(handler.descriptor_index(), mask(DESCRIPTOR_INDEX_BITS));
    }

    #[test]
    fn inobject_and_backing_store_handlers_are_distinguishable() {
        let inline = Handler::field(3, true, false, 0);
        let spilled = Handler::field(3, false, false, 0);
        assert_ne!(inline, spilled);
        assert!(inline.is_inobject());
        assert!(!spilled.is_inobject());
        assert_eq!(
            inline.storage_offset_in_words(),
            spilled.storage_offset_in_words()
        );
    }

    #[test]
    fn a_handler_always_fits_in_a_smi() {
        let widest = Handler::field(
            mask(STORAGE_OFFSET_BITS),
            true,
            true,
            mask(DESCRIPTOR_INDEX_BITS),
        );
        assert!(
            crate::objects::smi::Smi::is_valid(widest.raw() as i64),
            "the widest handler must still be a Smi"
        );
    }

    #[test]
    fn kinds_are_distinguishable() {
        assert_eq!(Handler::field(0, true, false, 0).kind(), HandlerKind::Field);
        assert_eq!(Handler::global(2).kind(), HandlerKind::Global);
        assert_eq!(Handler::global(2).descriptor_index(), 2);
    }
}
