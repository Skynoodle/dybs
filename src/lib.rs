//! An experiment in dynamic single-owner, multiple-borrow smart pointers
//!
//! ## Why?
//!
//! Rust's Borrow Checker usually statically enforces single-owner, multiple-borrow
//! semantics. Standard library smart pointers such as `Rc` extend this to provide
//! a form of _dynamic_ borrow checking, but they do so in a way that also results
//! in allowing multiple owners while also subtly shifting responsibility for
//! checking the lifetimes.
//!
//! In the static borrow checker case, accessing a borrowed value (reference) can't
//! fail: lifetimes enforce that the owner must keep the value alive long enough
//! to satisfy any outstanding borrows. In the dynamic case, this is reversed:
//! it becomes the responsibility of the holder of a borrowed value (a weak pointer)
//! to handle the possibility that the underlying value has been invalidated by its
//! owner(s).
//!
//! Dybs investigates a model closer to a dynamic version of the borrow checker's
//! behaviour: Values retain exactly one owner, which can provide runtime-checked
//! borrows of that value and which takes on responsibility for ensuring the value
//! remains valid for the duration of any borrows. This has the consequence that
//! dropping the owning pointer can _fail_ at runtime if there exist any outstanding
//! borrows.

use std::{
    ops::Deref,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Inner<T: ?Sized> {
    count: AtomicUsize,
    data: T,
}

/// A dynamic exclusively-owning smart pointer to a value
pub struct My<T: ?Sized> {
    inner: NonNull<Inner<T>>,
}

/// A dynamic borrow of a value
pub struct Dyb<'l, T: ?Sized> {
    inner: &'l Inner<T>,
}

impl<T> My<T> {
    /// Construct a new exclusively-owning `My` pointer from a value
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
    /// Dynamically borrow the value
    ///
    /// This borrow may live for any lifetime less than that of the bounds
    /// on the value itself - notable, this implies that (as far as the
    /// borrow-checker is concerned) it may outlive the owner itself!
    ///
    /// This actually shifts the responsibility for ensuring the value lives
    /// long enough back onto the owner (just like the static borrow checker)
    /// with the caveat that dropping the value early will _fail at runtime_.
    ///
    /// Dybs satisfies this requirement by leaking the value and panicking
    /// in this case. The leaking ensures soundness as in the worst case the
    /// value will just live forever, while panicking ensures the error is
    /// reported at the appropriate time rather than unfairly blaming a `Dyb`
    /// for holding a borrow for "too long" or entirely ignoring the problem.
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
