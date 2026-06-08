pub mod contants;
pub mod db;
pub mod json;

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
macro_rules! make_update_dto {
    (
        $update_name:ident from $base:ident {
            $($field:ident: $type:ty),+ $(,)?
        }
    ) => {
        use serde::Deserialize;
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct $update_name {
            $(pub $field: Option<$type>),+
        }
    };
}

#[macro_export]
macro_rules! define_roles {
    ($($struct:ident => $role:expr),* $(,)?) => {
        $(pub struct $struct;)*

        pub trait RequiredRole {
            fn role() -> Option<Role>;
        }

        $(
            impl RequiredRole for $struct {
                fn role() -> Option<Role> { $role }
            }
        )*
    };
}
