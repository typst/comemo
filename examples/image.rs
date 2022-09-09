use comemo::{Track, Tracked};

fn main() {
    let mut image = Image::new(20, 40);

    // [Miss] The cache is empty.
    describe(image.track());

    // [Hit] Everything stayed the same.
    describe(image.track());

    image.resize(80, 30);

    // [Miss] The image's width and height are different.
    describe(image.track());

    image.resize(80, 70);
    image.pixels.fill(255);

    // [Hit] The last call only read the width and it stayed the same.
    describe(image.track());
}

/// Format the image's size humanly readable.
#[comemo::memoize]
fn describe(image: Tracked<Image>) -> &'static str {
    if image.width() > 50 || image.height() > 50 {
        "The image is big!"
    } else {
        "The image is small!"
    }
}

/// A raster image.
struct Image {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Image {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height) as usize],
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        // Resize the actual image ...
    }
}

#[comemo::track]
impl Image {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }
}
