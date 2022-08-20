use comemo::{Track, Tracked};

#[rustfmt::skip]
fn main() {
    let fonts = Fonts(vec![
        Face { name: "mathup", style: Style::Normal },
        Face { name: "mathbf", style: Style::Bold },
        Face { name: "mathit", style: Style::Italic },
    ]);

    let par = Paragraph(vec![
        Segment { text: "HELLO ".into(), font: "mathup" },
        Segment { text: "WORLD".into(), font: "mathit" },
    ]);

    let shaped = shape(&par, fonts.track());
    println!("{shaped}");

    let shaped = shape(&par, fonts.track());
    println!("{shaped}");
}

/// Shape a piece of text with fonts.
#[comemo::memoize]
fn shape(par: &Paragraph, fonts: Tracked<Fonts>) -> String {
    let mut shaped = String::new();
    for piece in &par.0 {
        let font = fonts.get(&piece.font).unwrap();
        for c in piece.text.chars() {
            shaped.push(font.map(c));
        }
    }
    shaped
}

/// A paragraph of text in different fonts.
#[derive(Debug, Clone, Hash)]
struct Paragraph(Vec<Segment>);

/// A segment of text with consistent font.
#[derive(Debug, Clone, Hash)]
struct Segment {
    text: String,
    font: &'static str,
}

/// A font database.
struct Fonts(Vec<Face>);

#[comemo::track]
impl Fonts {
    /// Select a face by family name.
    fn get(&self, family: &str) -> Option<&Face> {
        self.0.iter().find(|font| font.name == family)
    }
}

/// A font face.
#[derive(Debug, Clone, Hash)]
struct Face {
    name: &'static str,
    style: Style,
}

/// The style of a font face.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
enum Style {
    Normal,
    Bold,
    Italic,
}

impl Face {
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
