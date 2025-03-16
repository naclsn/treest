use mlua::Value;
use mlua::IntoLua;

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
    pub fn get(&self, name: &str) -> Value {
        match name {
            "appearance" | "appea" => self.appearance.clone().into_lua(todo!()).unwrap(),
            _ => Value::Nil,
        }
    }

    pub fn set(&mut self, name: &str, value: Value) -> Result<(), &str> {
        match name {
            "appearance" | "appea" => self.appearance = value.as_string().unwrap().to_string_lossy(),
            _ => (),
        }
        Ok(())
    }
}
