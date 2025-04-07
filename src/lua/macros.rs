#[macro_export]
macro_rules! struct_lua_conversion {
    ($($vis:vis struct $ty:ident {
        $($fvis:vis $field:ident: $fty:ty $({$from:expr; $into:expr})?),*$(,)?
    })*) => {$(
        #[derive(Clone, Default, Debug)]
        $vis struct $ty {
            $($fvis $field: $fty),*
        }

        impl ::mlua::FromLua for $ty {
            fn from_lua(value: ::mlua::Value, _: &Lua) -> ::mlua::Result<Self> {
                if let ::mlua::Value::Table(table) = value {
                    Ok(Self {
                        $($field: table
                            .get(stringify!($field))
                            $(.map($from as fn($fty) -> $fty))?
                            .map_err(|err| ::mlua::Error::WithContext {
                                context: format!(
                                    "field {} required on type {}",
                                    stringify!($field),
                                    stringify!($ty),
                                ),
                                cause: err.into(),
                            })?,)*
                    })
                } else {
                    Err(::mlua::Error::FromLuaConversionError {
                        from: value.type_name(),
                        to: stringify!($ty).to_string(),
                        message: Some("expected table".to_string()),
                    })
                }
            }
        }

        impl ::mlua::IntoLua for $ty {
            fn into_lua(self, lua: &::mlua::Lua) -> ::mlua::Result<::mlua::Value> {
                let t = lua.create_table()?;
                $(t.raw_set(stringify!($field), $(($into as fn($fty) -> $fty))?(self.$field))?;)*
                Ok(::mlua::Value::Table(t))
            }
        }

        impl $crate::lua::typedoc::LuaTypeDoc for $ty {
            fn lua_type_doc() -> String {
                stringify!($ty).to_string()
            }
        }

        impl $crate::lua::typedoc::LuaTypeAliasDoc for $ty {
            fn lua_type_doc_alias_to() -> String {
                format!(
                    "{{ {} }}",
                    [$(format!(
                        "{}: {}",
                        stringify!($field),
                        <$fty as $crate::lua::typedoc::LuaTypeDoc>::lua_type_doc(),
                    )),*].join(", "),
                )
            }
        }
    )*};
}

#[macro_export]
macro_rules! flags_lua_conversion {
    ($($vis:vis struct $ty:ident {
        $($fvis:vis $flag:ident: $($val:literal)|+),*$(,)?
    })*) => {$(
        #[derive(Clone, Default, Debug)]
        $vis struct $ty {
            $($fvis $flag: &'static str),*
        }

        impl ::mlua::IntoLua for $ty {
            fn into_lua(self, lua: &::mlua::Lua) -> ::mlua::Result<::mlua::Value> {
                let t = lua.create_table()?;
                $(t.raw_push(self.$flag)?;)*
                Ok(::mlua::Value::Table(t))
            }
        }

        impl ::mlua::FromLua for $ty {
            fn from_lua(value: ::mlua::Value, lua: &Lua) -> ::mlua::Result<Self> {
                let type_name = value.type_name();
                let flags: Vec<String> = match value {
                    Value::String(s) => vec![s.to_string_lossy()],
                    _ => Vec::from_lua(value, lua)?,
                };

                let mut r = Self::default();
                for f in flags {
                    match f.as_str() {
                        $($($val => {
                            if !r.$flag.is_empty() {
                                return Err(::mlua::Error::FromLuaConversionError {
                                    from: type_name,
                                    to: stringify!($ty).to_string(),
                                    message: Some(format!(
                                        "multiple values given for {}: flags {:?} and {:?}",
                                        stringify!($flag),
                                        r.$flag,
                                        $val,
                                    )),
                                });
                            }
                            r.$flag = $val;
                        })+)*
                        _ => {
                            return Err(::mlua::Error::FromLuaConversionError {
                                from: type_name,
                                to: stringify!($ty).to_string(),
                                message: Some(format!("unexpected value {:?}", f)),
                            });
                        },
                    }
                }

                $(if r.$flag.is_empty() {
                    return Err(::mlua::Error::FromLuaConversionError {
                        from: type_name,
                        to: stringify!($ty).to_string(),
                        message: Some(format!(
                            "missing value for {}; one of these flag should be present: {}",
                            stringify!($flag),
                            [$($val),*].join(", "),
                        )),
                    });
                })*

                Ok(r)
            }
        }

        impl $crate::lua::typedoc::LuaTypeDoc for $ty {
            fn lua_type_doc() -> String {
                stringify!($ty).to_string()
            }
        }

        impl $crate::lua::typedoc::LuaTypeAliasDoc for $ty {
            fn lua_type_doc_alias_to() -> String {
                let mut r = format!(
                    "({})[]",
                    [$(format!(
                        "{}",
                        [$(format!("'{}'", $val)),*].join("|")
                    )),*].join(" | "),
                );
                if 1 == [$(stringify!($flag)),*].len() {
                    r.push_str(&format!(
                        " | ({})",
                        [$($(format!("'{}'", $val)),*),*].join("|")
                    ));
                }
                r
            }
        }
    )*};
}

#[macro_export]
macro_rules! lua_aliased_function {
    ($($vis:vis $ty:ident: fn($($ar:ident: $aty:ty),*$(,)?)$( -> $rty:ty)?;)*) => {$(
        #[derive(Debug, Clone)]
        $vis struct $ty(::mlua::Function);

        impl $ty {
            #[allow(unused_parens)]
            pub fn call(&self, $($ar: $aty),*) -> ::mlua::Result<($($rty)?)> {
                self.0.call(($($ar),*))
            }
        }

        impl From<$ty> for ::mlua::Function {
            fn from(value: $ty) -> Self {
                value.0
            }
        }

        impl ::mlua::FromLua for $ty {
            fn from_lua(value: Value, lua: &Lua) -> Result<Self> {
                Function::from_lua(value, lua).map(Self)
            }
        }

        impl $crate::lua::typedoc::LuaTypeDoc for $ty {
            fn lua_type_doc() -> String {
                format!(
                    "fun({})",
                    <[String]>::join(&[$(format!(
                        "{}:{}",
                        stringify!($ar),
                        <$aty>::lua_type_doc(),
                    )),*], ", "),
                )$(+ &if std::any::type_name::<$rty>().contains("Either") {
                    format!(": ({})", <$rty>::lua_type_doc())
                } else {
                    format!(": {}", <$rty>::lua_type_doc())
                })?
            }
        }
    )*};
}
