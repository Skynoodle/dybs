use std::{
    ops::Deref,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Inner<T: ?Sized> {
    count: AtomicUsize,
    data: T,
}

pub struct My<T: ?Sized> {
    inner: NonNull<Inner<T>>,
}

pub struct Dyb<'l, T: ?Sized> {
    inner: &'l Inner<T>,
}

impl<T> My<T> {
    pub fn new(data: T) -> My<T> {
        let inner = Inner {
            count: AtomicUsize::new(0),
            data,
        };
        My {
            inner: Box::leak(Box::new(inner)).into(),
        }
    }
}
impl<T: ?Sized> My<T> {
    pub fn borrow<'l>(&self) -> Dyb<'l, T> {
        // # Safety
        // This deref + borrow yields a reference with lifetime 'l.
        // Lifetime 'l may, as the borrow checker is concerned, outlive
        // self. My<T> only drops inner if it is dropped itself while
        // the reference count is zero. Since we immediately increment
        // the reference count, and only decrement it when the Dyb, and
        // therefore also our extended lifetime reference, is dropped,
        // this is sound.
        let inner: &'l Inner<T> = unsafe { &*self.inner.as_ptr() };
        inner.count.fetch_add(1, Ordering::Release);

        Dyb { inner }
    }
}

impl<T: ?Sized> Deref for Dyb<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner.data
    }
}

impl<T: ?Sized> Drop for My<T> {
    fn drop(&mut self) {
        // # Safety
        // `inner` is only invalidated later in this drop implementation,
        // so it's always guaranteed to be valid here
        let count = unsafe { self.inner.as_ref() }.count.load(Ordering::Acquire);
        if count == 0 {
            // # Safety
            // `inner` is still fine as we haven't invalidated it yet,
            // and we know it was created from a `Box` in the first place
            // so satisfies `Box::from_raw`'s requirements
            drop(unsafe { Box::from_raw(self.inner.as_ptr()) });
        } else {
            panic!("My pointer dropped with outstanding Dybs - this will leak the resource")
        }
    }
}

impl<T: ?Sized> Drop for Dyb<'_, T> {
    fn drop(&mut self) {
        self.inner.count.fetch_sub(1, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn constructs() {
        let _my = My::new(());
    }

    #[test]
    fn borrow_works() {
        let my = My::new(());
        let dyb = my.borrow();
        assert_eq!(*dyb, ())
    }

    #[test]
    #[should_panic(
        expected = "My pointer dropped with outstanding Dybs - this will leak the resource"
    )]
    fn invalid_drop_order_panics() {
        let my = My::new(());
        let _dyb = my.borrow();
        drop(my);
    }
}
