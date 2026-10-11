//! Stands in for `src/objects/object-macros.h`.
//!
//! V8 generates object accessors with macros; this is the one macro `vx`
//! needs. It declares a newtype over a [`HeapObject`](crate::objects::tagged::HeapObject)
//! -- the Rust stand-in for `Tagged<T>` -- and gives it the handful of
//! operations every object has. The per-type field accessors are written out
//! by hand in each object module, where the layout constants live next to
//! them.

/// Declares an object type: a typed view onto a heap address.
///
/// The type carries no data beyond the address, so it is `Copy` and costs
/// nothing to pass around, exactly like V8's `Tagged<T>`.
#[macro_export]
macro_rules! object_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name($crate::objects::tagged::HeapObject);

        impl $name {
            /// Views `object` as a `
            #[doc = stringify!($name)]
            /// ` without checking its instance type.
            ///
            /// The checked entry point is each type's `cast`, which the
            /// modules that have a map to consult provide.
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

            /// The strong tagged reference to this object.
            #[inline]
            pub const fn tagged(self) -> $crate::objects::tagged::Tagged {
                self.0.tagged()
            }

            /// The address of the field `offset` bytes in.
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
