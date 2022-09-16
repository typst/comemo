#![allow(unused)]

use comemo::{Track, Tracked};

// TODO
// - Reporting and evicting

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
    struct __ComemoUnique;
    ::comemo::internal::assert_hashable_or_trackable(&image);
    ::comemo::internal::memoized(
        "describe",
        ::core::any::TypeId::of::<__ComemoUnique>(),
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

        impl Track for Image {}
        impl ::comemo::internal::Trackable for Image {
            type Constraint = Constraint;
            type Surface = SurfaceFamily;

            fn valid(&self, constraint: &Self::Constraint) -> bool {
                constraint.width.valid(|()| ::comemo::internal::hash(&self.width()))
                    && constraint
                        .height
                        .valid(|()| ::comemo::internal::hash(&self.height()))
            }

            fn surface<'a, 'r>(
                tracked: &'r ::comemo::Tracked<'a, Image>,
            ) -> &'r Surface<'a> {
                // Safety: Surface is repr(transparent).
                unsafe { &*(tracked as *const _ as *const _) }
            }
        }

        pub enum SurfaceFamily {}
        impl<'a> ::comemo::internal::Family<'a> for SurfaceFamily {
            type Out = Surface<'a>;
        }

        #[repr(transparent)]
        pub struct Surface<'a>(::comemo::Tracked<'a, Image>);

        impl Surface<'_> {
            pub(super) fn width(&self) -> u32 {
                let input = ();
                let (inner, constraint) = ::comemo::internal::to_parts(self.0);
                let output = inner.width();
                if let Some(constraint) = &constraint {
                    constraint.width.set(input, ::comemo::internal::hash(&output));
                }
                output
            }

            pub(super) fn height(&self) -> u32 {
                let input = ();
                let (inner, constraint) = ::comemo::internal::to_parts(self.0);
                let output = inner.height();
                if let Some(constraint) = &constraint {
                    constraint.height.set(input, ::comemo::internal::hash(&output));
                }
                output
            }
        }

        #[derive(Default)]
        pub struct Constraint {
            width: ::comemo::internal::SoloConstraint,
            height: ::comemo::internal::SoloConstraint,
        }

        impl ::comemo::internal::Join for Constraint {
            fn join(&self, outer: &Self) {
                self.width.join(&outer.width);
                self.height.join(&outer.height);
            }
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
