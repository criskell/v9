#[macro_export]
macro_rules! object_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name($crate::objects::tagged::HeapObject);

        impl $name {
            #[doc = stringify!($name)]
            #[inline]
            pub const fn unchecked_cast(
                object: $crate::objects::tagged::HeapObject,
            ) -> $name {
                $name(object)
            }

            #[inline]
            pub const fn object(self) -> $crate::objects::tagged::HeapObject {
                self.0
            }

            #[inline]
            pub const fn address(self) -> $crate::objects::tagged::Address {
                self.0.address()
            }

            #[inline]
            pub const fn tagged(self) -> $crate::objects::tagged::Tagged {
                self.0.tagged()
            }

            #[inline]
            pub const fn field(self, offset: i32) -> $crate::objects::tagged::Address {
                self.0.field_address(offset)
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, concat!(stringify!($name), "(0x{:x})"), self.0.address())
            }
        }
    };
}
