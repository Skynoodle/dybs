use std::{
    ops::Deref,
    sync::atomic::{AtomicUsize, Ordering},
};

pub struct Inner<T> {
    count: AtomicUsize,
    data: T,
}

pub struct My<T> {
    inner: *const Inner<T>,
}

pub struct Dyb<'l, T> {
    inner: &'l Inner<T>,
}

impl<T> My<T> {
    pub fn new(data: T) -> My<T> {
        let inner = Inner {
            count: AtomicUsize::new(0),
            data,
        };
        My {
            inner: Box::into_raw(Box::new(inner)),
        }
    }
    pub fn borrow<'l>(&self) -> Dyb<'l, T> {
        // Safety:
        // This deref + borrow yields a reference with lifetime 'l.
        // Lifetime 'l may, as the borrow checker is concerned, outlive
        // self. My<T> only drops inner if it is dropped itself while
        // the reference count is zero. Since we immediately increment
        // the reference count, and only decrement it when the Dyb, and
        // therefore also our extended lifetime reference, is dropped,
        // this is sound.
        let inner: &'l Inner<T> = unsafe { &*self.inner };
        inner.count.fetch_add(1, Ordering::Release);

        Dyb { inner }
    }
}

impl<T> Deref for Dyb<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner.data
    }
}

impl<T> Drop for My<T> {
    fn drop(&mut self) {
        if unsafe { &*self.inner }.count.load(Ordering::Acquire) == 0 {
            drop(unsafe { Box::from_raw(self.inner as *mut Inner<T>) });
        } else {
            panic!("My pointer dropped with outstanding Dybs - this will leak the resource")
        }
    }
}

impl<T> Drop for Dyb<'_, T> {
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
    #[should_panic(expected = "My pointer dropped with outstanding Dybs - this will leak the resource")]
    fn invalid_drop_order_panics() {
        let my = My::new(());
        let _dyb = my.borrow();
        drop(my);
    }
}
