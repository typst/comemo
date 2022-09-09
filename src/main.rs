use std::cell::{Cell, RefCell};

fn main() {
    let mut image = Image::new(20, 40);

    // [Miss]
    // This executes the code in `describe` as the cache is thus far empty.
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
fn describe(image: TrackedImage) -> &'static str {
    fn inner(image: TrackedImage) -> &'static str {
        if image.width() > 50 || image.height() > 50 {
            "The image is big!"
        } else {
            "The image is small!"
        }
    }

    thread_local! {
        static NR: Cell<usize> = Cell::new(0);
        static CACHE: RefCell<Vec<(ImageConstraint, &'static str)>> =
            RefCell::new(vec![]);
    }

    let mut hit = true;
    let output = CACHE.with(|cache| {
        cache
            .borrow()
            .iter()
            .find(|(ct, _)| ct.valid(image.inner))
            .map(|&(_, output)| output)
    });

    let output = output.unwrap_or_else(|| {
        let ct = RefCell::new(ImageConstraint::default());
        let image = TrackedImage { inner: image.inner, tracker: Some(&ct) };
        let output = inner(image);
        CACHE.with(|cache| cache.borrow_mut().push((ct.into_inner(), output)));
        hit = false;
        output
    });

    println!(
        "{} {} {} {}",
        "describe",
        NR.with(|nr| nr.replace(nr.get() + 1)),
        if hit { "[hit]: " } else { "[miss]:" },
        output,
    );

    output
}

#[derive(Copy, Clone)]
struct TrackedImage<'a> {
    inner: &'a Image,
    tracker: Option<&'a RefCell<ImageConstraint>>,
}

impl<'a> TrackedImage<'a> {
    fn width(&self) -> u32 {
        let output = self.inner.width();
        if let Some(tracker) = &self.tracker {
            tracker.borrow_mut().width = Some(output);
        }
        output
    }

    fn height(&self) -> u32 {
        let output = self.inner.height();
        if let Some(tracker) = &self.tracker {
            tracker.borrow_mut().height = Some(output);
        }
        output
    }
}

#[derive(Debug, Default)]
struct ImageConstraint {
    width: Option<u32>,
    height: Option<u32>,
}

impl ImageConstraint {
    fn valid(&self, image: &Image) -> bool {
        self.width.map_or(true, |v| v == image.width())
            && self.height.map_or(true, |v| v == image.height())
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

    fn track(&self) -> TrackedImage<'_> {
        TrackedImage { inner: self, tracker: None }
    }
}

impl Image {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }
}
