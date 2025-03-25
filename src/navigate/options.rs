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
    pub fn get(&self, name: &str) -> Option<String> {
        match name {
            "appearance" | "appea" => Some(self.appearance.clone()),
            _ => None,
        }
    }

    pub fn set(&mut self, name: &str, value: String) -> Option<()> {
        match name {
            "appearance" | "appea" => Some(self.appearance = value),
            _ => None,
        }
    }
}
