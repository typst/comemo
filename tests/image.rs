use comemo::{Track, Tracked};

#[test]
fn test_image() {
    let mut image = Image::new(20, 40);

    describe(image.track()); // [Miss] The cache is empty.
    describe(image.track()); // [Hit] Everything stayed the same.

    image.resize(80, 30);

    describe(image.track()); // [Miss] Width and height changed.
    select(image.track(), "width"); // [Miss] First call.
    select(image.track(), "height"); // [Miss] Different 2nd argument.

    image.resize(80, 70);
    image.pixels.fill(255);

    describe(image.track()); // [Hit] Width is > 50 stayed the same.
    select(image.track(), "width"); // [Hit] Width stayed the same.
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

/// Select either width or height.
#[comemo::memoize]
fn select(image: Tracked<Image>, what: &str) -> u32 {
    match what {
        "width" => image.width(),
        "height" => image.height(),
        _ => panic!("there is nothing else!"),
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
