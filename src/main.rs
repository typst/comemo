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
    ::comemo::internal::assert_hashable_or_trackable::<Tracked<Image>>();
    ::comemo::internal::cached(
        "describe",
        ::comemo::internal::Args((image,)),
        |(image,)| {
            if image.width() > 50 || image.height() > 50 {
                "The image is big!"
            } else {
                "The image is small!"
            }
        },
    )
}

const _: () = {
    mod private {
        use super::*;

        impl ::comemo::Track for Image {}
        impl ::comemo::internal::Trackable for Image {
            type Constraint = Constraint;
            type Surface = SurfaceFamily;

            fn valid(&self, constraint: &Self::Constraint) -> bool {
                constraint.width.valid(&self.width())
                    && constraint.height.valid(&self.height())
            }

            fn surface<'a, 'r>(tracked: &'r Tracked<'a, Image>) -> &'r Surface<'a> {
                // Safety: Surface is repr(transparent).
                unsafe { &*(tracked as *const _ as *const _) }
            }
        }

        pub enum SurfaceFamily {}
        impl<'a> ::comemo::internal::Family<'a> for SurfaceFamily {
            type Out = Surface<'a>;
        }

        #[repr(transparent)]
        pub struct Surface<'a>(Tracked<'a, Image>);

        impl<'a> Surface<'a> {
            pub(super) fn width(&self) -> u32 {
                let (inner, constraint) = ::comemo::internal::to_parts(self.0);
                let output = inner.width();
                if let Some(constraint) = &constraint {
                    constraint.width.set(&output);
                }
                output
            }

            pub(super) fn height(&self) -> u32 {
                let (inner, constraint) = ::comemo::internal::to_parts(self.0);
                let output = inner.height();
                if let Some(constraint) = &constraint {
                    constraint.height.set(&output);
                }
                output
            }
        }

        #[derive(Default)]
        pub struct Constraint {
            width: ::comemo::internal::HashConstraint<u32>,
            height: ::comemo::internal::HashConstraint<u32>,
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
