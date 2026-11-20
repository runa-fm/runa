//! Macro definitions used across runa.
//!
//! This module contains macros that are used in multiple places in the codebase.
//! They are defined here to avoid code duplication and to keep the codebase clean.

/// Getter macro
///
/// Usage:
/// - `field: T`            -> returns by value (T copied)
/// - `field: &T`           -> returns by reference
/// - `method => field: T`  -> rename getter function
/// - [cfg(...)]            -> apply config specific setting to
#[doc(hidden)]
#[macro_export]
macro_rules! getters {
    ($( #[$meta:meta] )? $method:ident => $field:ident : &$type:ty, $($rest:tt)+) => {
        $crate::getters!($( #[$meta] )? $method => $field : &$type);
        $crate::getters!($($rest)+);
    };

    ($( #[$meta:meta] )? $method:ident => $field:ident : $type:ty, $($rest:tt)+) => {
        $crate::getters!($( #[$meta] )? $method => $field : $type);
        $crate::getters!($($rest)+);
    };

    ($( #[$meta:meta] )? $field:ident : &$type:ty, $($rest:tt)+) => {
        $crate::getters!($( #[$meta] )? $field : &$type);
        $crate::getters!($($rest)+);
    };

    ($( #[$meta:meta] )? $field:ident : $type:ty, $($rest:tt)+) => {
        $crate::getters!($( #[$meta] )? $field : $type);
        $crate::getters!($($rest)+);
    };

    ($( #[$meta:meta] )? $method:ident => $field:ident : &$type:ty $(,)?) => {
        $( #[$meta] )?
        #[inline]
        pub(crate) fn $method(&self) -> &$type {
            &self.$field
        }
    };

    ($( #[$meta:meta] )? $method:ident => $field:ident : $type:ty $(,)?) => {
        $( #[$meta] )?
        #[inline]
        pub(crate) const fn $method(&self) -> $type {
            self.$field
        }
    };

    ($( #[$meta:meta] )? $field:ident : &$type:ty $(,)?) => {
        $( #[$meta] )?
        #[inline]
        pub(crate) fn $field(&self) -> &$type {
            &self.$field
        }
    };

    ($( #[$meta:meta] )? $field:ident : $type:ty $(,)?) => {
        $( #[$meta] )?
        #[inline]
        pub(crate) const fn $field(&self) -> $type {
            self.$field
        }
    };

    () => {};
}

#[doc(hidden)]
#[macro_export]
macro_rules! option_arc_str_getters {
    ($($field:ident),* $(,)?) => {
        $(
            #[inline]
            pub(crate) fn $field(&self) -> &str {
                self.$field.as_deref().unwrap_or_default()
            }
        )*
    };
}
