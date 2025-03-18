use mlua::{Value, Lua, IntoLua};

pub struct Options {
    pub appearance: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            appearance: "pretty".into(),
        }
    }
}

impl Options {
    pub fn get(&self, name: &str, lua: &Lua) -> Value {
        match name {
            "appearance" | "appea" => self.appearance.clone().into_lua(lua).unwrap(),
            _ => Value::Nil,
        }
    }

    // TODO: return a LuaResult<()>
    pub fn set(&mut self, name: &str, value: Value) {
        match name {
            "appearance" | "appea" => self.appearance = value.as_string().unwrap().to_string_lossy(),
            _ => (),
        }
    }
}
