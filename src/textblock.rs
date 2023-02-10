use tui::{buffer::Buffer, layout::Rect, style::Style, widgets::Widget};

pub struct TextBlock {
    text: Vec<String>,
    style: Style,
}

impl<'me> Widget for &'me TextBlock {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (k, vline) in self.text.iter().take(area.height as usize).enumerate() {
            buf.set_string(area.x, area.y + k as u16, vline, self.style);
        }
    }
}

impl TextBlock {
    pub fn raw(text: impl IntoIterator<Item = String>) -> TextBlock {
        TextBlock {
            text: text.into_iter().collect(),
            style: Style::default(),
        }
    }

    pub fn styled(text: impl IntoIterator<Item = String>, style: Style) -> TextBlock {
        TextBlock {
            text: text.into_iter().collect(),
            style,
        }
    }

    pub fn wrapped(text: &str, width: usize, style: Style) -> TextBlock {
        let mut vlines = Vec::new();
        for line in text.lines() {
            let mut chs = line.chars().peekable();
            let niw = chs.by_ref().take(width).collect::<String>();
            let indent = niw.chars().take_while(char::is_ascii_whitespace).count();
            vlines.push(niw);
            while chs.peek().is_some() {
                vlines.push(
                    " ".repeat(indent)
                        .chars()
                        .chain(['\u{21aa}', ' '].into_iter())
                        .chain(chs.by_ref().take(width - indent - 2))
                        .collect::<String>(),
                );
            }
        }
        TextBlock {
            text: vlines,
            style,
        }
    }

    pub fn height(&self) -> u16 {
        self.text.len() as u16
    }
}
