use comemo::{Track, Tracked};

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

    // [Hit] The previous call only read the width and it stayed the same.
    describe(image.track());
}

/// Format the image's size humanly readable.
fn describe(image: Tracked<Image>) -> &'static str {
    fn inner(image: Tracked<Image>) -> &'static str {
        if image.width() > 50 || image.height() > 50 {
            "The image is big!"
        } else {
            "The image is small!"
        }
    }

    thread_local! {
        static NR: ::core::cell::Cell<usize> = ::core::cell::Cell::new(0);
        static CACHE: ::core::cell::RefCell<Vec<
            (<Image as comemo::internal::Trackable<'static>>::Tracker, &'static str)>
        > = ::core::cell::RefCell::new(vec![]);
    }

    let mut hit = true;
    let output = CACHE.with(|cache| {
        cache
            .borrow()
            .iter()
            .find(|(tracker, _)| {
                let (inner, _) = ::comemo::internal::to_parts(image);
                <Image as comemo::internal::Trackable>::valid(inner, tracker)
            })
            .map(|&(_, output)| output)
    });

    let output = output.unwrap_or_else(|| {
        let tracker = ::core::default::Default::default();
        let (image, _prev) = ::comemo::internal::to_parts(image);
        let image = ::comemo::internal::from_parts(image, Some(&tracker));
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

const _: () = {
    mod inner {
        use super::*;

        impl<'a> ::comemo::Track<'a> for Image {}
        impl<'a> ::comemo::internal::Trackable<'a> for Image {
            type Tracker = Tracker;
            type Surface = Surface<'a>;

            fn valid(&self, tracker: &Self::Tracker) -> bool {
                tracker.width.valid(&self.width()) && tracker.height.valid(&self.height())
            }

            fn surface<'s>(tracked: &'s Tracked<'a, Image>) -> &'s Self::Surface
            where
                Self: Track<'a>,
            {
                // Safety: Surface is repr(transparent).
                unsafe { &*(tracked as *const _ as *const Self::Surface) }
            }
        }

        #[repr(transparent)]
        pub struct Surface<'a>(Tracked<'a, Image>);

        impl<'a> Surface<'a> {
            pub(super) fn width(&self) -> u32 {
                let (inner, tracker) = ::comemo::internal::to_parts(self.0);
                let output = inner.width();
                if let Some(tracker) = &tracker {
                    tracker.width.track(&output);
                }
                output
            }

            pub(super) fn height(&self) -> u32 {
                let (inner, tracker) = ::comemo::internal::to_parts(self.0);
                let output = inner.height();
                if let Some(tracker) = &tracker {
                    tracker.height.track(&output);
                }
                output
            }
        }

        #[derive(Default)]
        pub struct Tracker {
            width: ::comemo::internal::AccessTracker<u32>,
            height: ::comemo::internal::AccessTracker<u32>,
        }
    }
};

// ---------------- Image ----------------

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

impl Image {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }
}
