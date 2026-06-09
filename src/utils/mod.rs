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
    ($($name:ident => $roles:expr),* $(,)?) => {
        $(
            pub struct $name;

            impl crate::auth::session::RequiredRole for $name {
                fn roles() -> &'static [Role] {
                    &$roles
                }
            }
        )*
    };
}
