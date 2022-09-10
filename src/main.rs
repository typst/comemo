use std::cell::{Cell, RefCell};
use std::fmt::{self, Debug, Formatter};
use std::hash::Hash;
use std::marker::PhantomData;
use std::num::NonZeroU128;

use siphasher::sip128::{Hasher128, SipHasher};

// TODO
// - Nested tracked call
// - Tracked return value from tracked method
// - Tracked methods with arguments

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
        static CACHE: RefCell<Vec<(ImageTracker, &'static str)>> =
            RefCell::new(vec![]);
    }

    let mut hit = true;
    let output = CACHE.with(|cache| {
        cache
            .borrow()
            .iter()
            .find(|(tracker, _)| tracker.valid(image.inner))
            .map(|&(_, output)| output)
    });

    let output = output.unwrap_or_else(|| {
        let tracker = ImageTracker::default();
        let image = TrackedImage {
            inner: image.inner,
            tracker: Some(&tracker),
        };
        let output = inner(image);
        CACHE.with(|cache| cache.borrow_mut().push((tracker, output)));
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
    tracker: Option<&'a ImageTracker>,
}

impl<'a> TrackedImage<'a> {
    fn width(&self) -> u32 {
        let output = self.inner.width();
        if let Some(tracker) = &self.tracker {
            tracker.width.track(&output);
        }
        output
    }

    fn height(&self) -> u32 {
        let output = self.inner.height();
        if let Some(tracker) = &self.tracker {
            tracker.height.track(&output);
        }
        output
    }
}

#[derive(Debug, Default)]
struct ImageTracker {
    width: HashTracker<u32>,
    height: HashTracker<u32>,
}

impl ImageTracker {
    fn valid(&self, image: &Image) -> bool {
        self.width.valid(&image.width()) && self.height.valid(&image.height())
    }
}

#[derive(Default)]
struct HashTracker<T: Hash>(Cell<Option<NonZeroU128>>, PhantomData<T>);

impl<T: Hash> HashTracker<T> {
    fn valid(&self, value: &T) -> bool {
        self.0.get().map_or(true, |v| v == siphash(value))
    }

    fn track(&self, value: &T) {
        self.0.set(Some(siphash(value)));
    }
}

impl<T: Hash> Debug for HashTracker<T> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "HashTracker({:?})", self.0)
    }
}

/// Produce a non zero 128-bit hash of the value.
fn siphash<T: Hash>(value: &T) -> NonZeroU128 {
    let mut state = SipHasher::new();
    value.hash(&mut state);
    state
        .finish128()
        .as_u128()
        .try_into()
        .unwrap_or(NonZeroU128::new(u128::MAX).unwrap())
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
