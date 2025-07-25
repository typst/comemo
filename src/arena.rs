use std::alloc::Layout;
use std::cell::{Cell, UnsafeCell};
use std::mem::MaybeUninit;

pub struct Arena<const C: usize = 256> {
    storage: UnsafeCell<[MaybeUninit<u8>; C]>,
    offset: Cell<usize>,
}

impl<const C: usize> Arena<C> {
    pub fn new() -> Self {
        Self {
            storage: UnsafeCell::new([MaybeUninit::uninit(); C]),
            offset: Cell::new(0),
        }
    }

    pub fn alloc<T>(&self, value: T) -> &T {
        unsafe {
            let layout = Layout::for_value(&value).pad_to_align();
            let offset = round_up_to(self.offset.get(), layout.size());
            let end = offset + layout.size();
            if end > C {
                panic!("out of capacity");
            }
            let ptr = self.storage.get().byte_offset(offset as isize).cast::<T>();
            ptr.write(value);
            self.offset.set(end);
            &*ptr
        }
    }
}

fn round_up_to(n: usize, divisor: usize) -> usize {
    (n + (divisor - 1)) & !(divisor - 1)
}

impl<const C: usize> Default for Arena<C> {
    fn default() -> Self {
        Self::new()
    }
}
