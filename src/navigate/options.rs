use mlua::Either;

pub type OptionValue = Either<String, Either<isize, bool>>;

trait OptionValueConvert: Sized {
    fn my_into(&self) -> OptionValue;
    fn my_from(value: OptionValue) -> Option<Self>;
}

macro_rules! make_options {
    ($vis:vis struct $ty:ident {
        $(
            #[$doc:meta]
            $fvis:vis $field:ident|$fld:ident: $fty:ty = $default:expr
        ),*$(,)?
    }) => {
        $vis struct $ty {
            $($fvis $field: $fty),*
        }

        impl Default for $ty {
            fn default() -> Self {
                Self {
                    $($field: $default.try_into().unwrap()),*
                }
            }
        }

        impl $ty {
            pub fn get(&self, name: &str) -> Option<OptionValue> {
                match name {
                    $(stringify!($field) | stringify!($fld) => Some(self.$field.my_into()),)*
                    _ => None,
                }
            }

            pub fn set(&mut self, name: &str, value: OptionValue) -> Option<()> {
                match name {
                    $(stringify!($field) | stringify!($fld) => self.$field = <$fty>::my_from(value)?,)*
                    _ => return None,
                }
                Some(())
            }

            pub fn help(name: &str) -> Option<&'static str> {
                match name {
                    $(stringify!($field) | stringify!($fld) => Some(stringify!($doc)),)*
                    _ => None,
                }
            }

            pub fn list() -> &'static [&'static str] {
                &[$(stringify!($field)),*]
            }
        }
    }
}

make_options! {
    pub struct Options {
        /// "pretty" to use box-drawing characters, or "ascii" for more limited font/terminal/render/..
        pub appearance|appea: String = "pretty",
        /// put single-child on the same line as parent
        pub singlechildline|sch: bool = true,
        /// height of the message window
        pub messageheight|msh: u16 = 12,
        /// show a scollbar-like position indicator for long messages
        pub messagescrollbar|msba: bool = true,
        /// only when singlechildline is set; >n are shortened to letters eg. "a/b/coucou"
        pub pathshorten|psh: u8 = 2,
        /// highlight search matches
        pub hlsearch|hls: bool = false,
    }
}

impl OptionValueConvert for String {
    fn my_into(&self) -> OptionValue {
        Either::Left(self.clone())
    }

    fn my_from(value: OptionValue) -> Option<Self> {
        value.left()
    }
}

impl OptionValueConvert for bool {
    fn my_into(&self) -> OptionValue {
        Either::Right(Either::Right(*self))
    }

    fn my_from(value: OptionValue) -> Option<Self> {
        value.right()?.right()
    }
}

impl OptionValueConvert for u16 {
    fn my_into(&self) -> OptionValue {
        Either::Right(Either::Left(*self as isize))
    }

    fn my_from(value: OptionValue) -> Option<Self> {
        Some(value.right()?.left()? as Self)
    }
}

impl OptionValueConvert for u8 {
    fn my_into(&self) -> OptionValue {
        Either::Right(Either::Left(*self as isize))
    }

    fn my_from(value: OptionValue) -> Option<Self> {
        Some(value.right()?.left()? as Self)
    }
}
