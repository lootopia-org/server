pub mod contants;
pub mod db;
pub mod json;
pub mod types;

#[macro_export]
macro_rules! impl_from {
    ($from:ty => $to:ty { $($field:ident),+ $(,)? }) => {
        impl From<$from> for $to {
            fn from(val: $from) -> Self {
                Self {
                    $($field: val.$field),+
                }
            }
        }
    };
}

#[macro_export]
macro_rules! define_topics {
    (
        $( $topic_ident:ident : $topic_str:literal => [ $( $action:ident ),* $(,)? ] ),*
        $(,)?
    ) => {
        pub mod topics {
            $(
                pub const $topic_ident: &str = $topic_str;
            )*
        }

        pub mod event_types {
            $(
                $(
                    $crate::__paste_event_const!($topic_ident, $topic_str, $action);
                )*
            )*
        }
    };
}

#[macro_export]
macro_rules! cache_key {
    ($($arg:tt)*) => {
        &format!($($arg)*)
    };
}

#[macro_export]
macro_rules! define_roles {
    ($($name:ident => $roles:expr),* $(,)?) => {
        $(
            #[derive(Debug)]
            pub struct $name;

            impl crate::auth::session::RequiredRole for $name {
                fn roles() -> &'static [Role] {
                    &$roles
                }
            }
        )*
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __paste_event_const {
    ($topic_ident:ident, $topic_str:literal, $action:ident) => {
        ::paste::paste! {
            pub const [< $topic_ident _ $action:upper >]: &str =
                concat!($topic_str, ".", stringify!($action));
        }
    };
}
