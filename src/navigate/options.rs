use rhai::Dynamic;

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
    pub fn get(&self, name: &str) -> Dynamic {
        match name {
            "appearance" | "appea" => self.appearance.clone().into(),
            _ => Dynamic::UNIT,
        }
    }

    pub fn set(&mut self, name: &str, value: Dynamic) -> Result<(), &str> {
        match name {
            "appearance" | "appea" => self.appearance = value.into_string()?,
            _ => (),
        }
        Ok(())
    }
}
