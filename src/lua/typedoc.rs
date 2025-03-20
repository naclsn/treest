pub trait LuaTypeDoc {
    fn lua_type_doc() -> String;
}

pub trait LuaTypeAliasDoc {
    fn lua_type_doc_alias_to() -> String;
}

macro_rules! impl_lua_type_doc {
    ($str:literal for $(< $($T:ident),* $(;$($W:ident),*)? $(;;$(const $N:ident: $nty:ty),*)? > $ty:ty),*$(,)*) => {
        $(impl<$($T: LuaTypeDoc),*$($(,$W)*)?$($(,const $N:$nty)*)?> LuaTypeDoc for $ty {
            #[inline]
            fn lua_type_doc() -> String {
                format!($str, $($T::lua_type_doc()),*)
            }
        })*
    };
    (($str:literal | $strwrap:literal) for $(< $T:ident $(;$($W:ident),*)? $(;;$(const $N:ident: $nty:ty),*)? > $ty:ty),*$(,)*) => {
        $(impl<$T: LuaTypeDoc$($(,$W)*)?$($(,const $N:$nty)*)?> LuaTypeDoc for $ty {
            #[inline]
            fn lua_type_doc() -> String {
                let ar = $T::lua_type_doc();
                if std::any::type_name::<$T>().contains("Either") {
                    format!($strwrap, ar)
                } else {
                    format!($str, ar)
                }
            }
        })*
    };
    ($str:literal for $($ty:ty),*$(,)*) => {
        $(impl LuaTypeDoc for $ty {
            #[inline]
            fn lua_type_doc() -> String {
                format!($str)
            }
        })*
    };
    //(any for $(<$T:ident: $sty:path> $ty:ty),*$(,)*) => {
    //    $(impl<$T: $sty> LuaTypeDoc for $ty {
    //        #[inline]
    //        fn lua_type_doc() -> String {
    //            format!("{}", std::any::type_name::<T>())
    //        }
    //    })*
    //};
}

macro_rules! impl_lua_type_doc_tuples {
    ($($l:ident)+ @) => { };
    ($($l:ident)+ @ $h:ident $($t:ident)*) => {
        impl<$($l: LuaTypeDoc),+> LuaTypeDoc for ($($l,)+) {
            #[inline]
            fn lua_type_doc() -> String {
                format!("[{}]", [$($l::lua_type_doc()),+].join(", "))
            }
        }
        impl_lua_type_doc_tuples! { $($l)+ $h @ $($t)* }
    };
    ($h:ident $($t:ident)+) => {
        impl_lua_type_doc_tuples! { $h @ $($t)* __ }
    };
}

impl_lua_type_doc! { "any" for
    mlua::Value,
}
impl_lua_type_doc! { "boolean" for
    bool,
}
impl_lua_type_doc! { "error" for
    mlua::Error,
}
impl_lua_type_doc! { "function" for
    mlua::Function,
}
impl_lua_type_doc! { "integer" for
    i8, i16, i32, i64, i128, isize,
    u8, u16, u32, u64, u128, usize,
}
impl_lua_type_doc! { "lightuserdata" for
    mlua::LightUserData,
}
impl_lua_type_doc! { "nil" for
    (),
}
impl_lua_type_doc! { "number" for
    f32, f64,
}
//impl_lua_type_doc! { "userdata" for
//    <T: mlua::UserData> T,
//}
impl_lua_type_doc! { "string" for
    str, Box<str>, String, std::borrow::Cow<'_, str>,
    std::path::Path, std::path::PathBuf,
    std::ffi::CStr, std::ffi::CString, std::borrow::Cow<'_, std::ffi::CStr>,
    std::ffi::OsStr, std::ffi::OsString,
    //bstr::BStr,
    mlua::String,
    mlua::BString,
}
impl_lua_type_doc! { "table" for
    mlua::Table,
}
impl_lua_type_doc! { "{} | {}" for
    <L, R> mlua::Either<L, R>,
}
impl_lua_type_doc! { ("{}?" | "({})?") for // means: add () when T is Either
    <T> Option<T>,
}
impl_lua_type_doc! { ("{}[]" | "({})[]") for // means: add () when T is Either
    <T> &[T],
    <T;; const N: usize> [T; N],
    <T> Box<[T]>,
    <T> Vec<T>,
}
impl_lua_type_doc_tuples! { A B C D E F G H I J K L M N O P }
impl_lua_type_doc! { "table<{}, {}>" for
    <K, V; S> std::collections::HashMap<K, V, S>,
    <K, V> std::collections::BTreeMap<K, V>,
}
impl_lua_type_doc! { "{{ [{}]: boolean }}" for
    <T; S> std::collections::HashSet<T, S>,
    <T> std::collections::BTreeSet<T>,
}
//impl_lua_type_doc! { any for
//    <T: mlua::UserData> T,
//    <T: mlua::IntoLua> T,
//    <T: mlua::FromLua> T,
//}
impl<T: LuaTypeDoc + ?Sized> LuaTypeDoc for &T {
    fn lua_type_doc() -> String {
        T::lua_type_doc()
    }
}
