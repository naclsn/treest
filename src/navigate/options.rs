pub struct Options {
    pub appearance: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            appearance: "pretty".into(),
            //singlechildline: "true".into(),
            //messageheight: "12".into(),
            //scrollbar: "true".into(),
            //foldlong: "2".into(), // > 2, components are shortened to letters eg. "a/b/file.txt"
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
            "appearance" | "appea" => self.appearance = value,
            _ => return None,
        }
        Some(())
    }
}
