use comemo::{Prehashed, Track, Tracked};

#[test]
fn test_layout() {
    let par = Paragraph(vec![
        TextRun {
            font: "Helvetica".into(),
            text: "HELLO ".into(),
        },
        TextRun {
            font: "Futura".into(),
            text: "WORLD!".into(),
        },
    ]);

    let mut fonts = Fonts::default();
    fonts.insert("Helvetica", Style::Normal, vec![110; 75398]);
    fonts.insert("Futura", Style::Italic, vec![55; 12453]);

    // [Miss] The cache is empty.
    par.layout(fonts.track());
    fonts.insert("Verdana", Style::Normal, vec![99; 12554]);

    // [Hit] Verdana isn't used.
    par.layout(fonts.track());
    fonts.insert("Helvetica", Style::Bold, vec![120; 98532]);

    // [Miss] Helvetica changed.
    par.layout(fonts.track());
}

/// A paragraph composed from text runs.
#[derive(Debug, Hash)]
struct Paragraph(Vec<TextRun>);

impl Paragraph {
    /// A memoized method.
    #[comemo::memoize]
    fn layout(&self, fonts: Tracked<Fonts>) -> String {
        let mut result = String::new();
        for run in &self.0 {
            let font = fonts.select(&run.font).unwrap();
            for c in run.text.chars() {
                result.push(font.map(c));
            }
        }
        result
    }
}

/// A run of text with consistent font.
#[derive(Debug, Hash)]
struct TextRun {
    font: String,
    text: String,
}

/// Holds all fonts.
///
/// As font data is large and costly to hash, we use the `Prehashed` wrapper.
/// Otherwise, every call to `Fonts::select` would hash the returned font from
/// scratch.
#[derive(Default)]
struct Fonts(Vec<Prehashed<Font>>);

impl Fonts {
    /// Insert a new with name and data.
    fn insert(&mut self, name: impl Into<String>, style: Style, data: Vec<u8>) {
        let name = name.into();
        self.0.retain(|font| font.name != name);
        self.0.push(Prehashed::new(Font { name, style, data }))
    }
}

#[comemo::track]
impl Fonts {
    /// Select a font by name.
    fn select(&self, name: &str) -> Option<&Prehashed<Font>> {
        self.0.iter().find(|font| font.name == name)
    }
}

/// A large binary font.
#[derive(Hash)]
struct Font {
    name: String,
    data: Vec<u8>,
    style: Style,
}

impl Font {
    /// Map a character.
    fn map(&self, c: char) -> char {
        let base = match self.style {
            Style::Normal => 0x41,
            Style::Bold => 0x1D400,
            Style::Italic => 0x1D434,
        };

        if c.is_ascii_alphabetic() {
            std::char::from_u32(base + (c as u32 - 0x41)).unwrap()
        } else {
            c
        }
    }
}

/// A font style.
#[derive(Hash)]
enum Style {
    Normal,
    Italic,
    Bold,
}
