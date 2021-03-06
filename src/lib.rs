#![cfg_attr(feature = "unstable", feature(unsize, coerce_unsized, doc_cfg))]

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
pub struct Dyb<T: ?Sized> {
    inner: NonNull<Inner<T>>,
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
    /// on the value itself - notably, this implies that (as far as the
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
    pub fn borrow(&self) -> Dyb<T> {
        // # Safety
        // This deref + borrow yields a reference with lifetime 'l.
        // Lifetime 'l may, as the borrow checker is concerned, outlive
        // self. My<T> only drops inner if it is dropped itself while
        // the reference count is zero. Since we immediately increment
        // the reference count, and only decrement it when the Dyb, and
        // therefore also our extended lifetime reference, is dropped,
        // this is sound.
        unsafe { self.inner.as_ref() }
            .count
            .fetch_add(1, Ordering::Release);

        Dyb { inner: self.inner }
    }
}

impl<T: ?Sized> Deref for My<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // # Safety
        // `inner` is valid for at least the full lifetime of self
        &unsafe { self.inner.as_ref() }.data
    }
}

impl<T: ?Sized> Deref for Dyb<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // # Safety
        // We maintain a nonzero refcount in the inner for the full lifetime
        // of self, so the owning `My` won't have dropped inner.
        &unsafe { self.inner.as_ref() }.data
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

impl<T: ?Sized> Drop for Dyb<T> {
    fn drop(&mut self) {
        // # Safety
        // We maintain a nonzero refcount in the inner for the full lifetime
        // of self, so the owning `My` won't have dropped inner.
        unsafe { self.inner.as_ref() }
            .count
            .fetch_sub(1, Ordering::Release);
    }
}

impl<T: Clone> Clone for My<T> {
    fn clone(&self) -> Self {
        // # Safety
        // `inner` is valid for at least the full lifetime of self
        Self::new(unsafe { self.inner.as_ref() }.data.clone())
    }
}

impl<T: ?Sized> Clone for Dyb<T> {
    fn clone(&self) -> Self {
        // # Safety
        // We maintain a nonzero refcount in the inner for the full lifetime
        // of self, so the owning `My` won't have dropped inner.
        // After this point, the count has incremented once more for the new
        // Dyb we return, upholding the invariant for that too.
        unsafe { self.inner.as_ref() }
            .count
            .fetch_add(1, Ordering::Release);

        Self { inner: self.inner }
    }
}

#[cfg(feature = "unstable")]
#[doc(cfg(feature = "unstable"))]
mod corce_unsized {
    use super::*;
    use std::{marker::Unsize, ops::CoerceUnsized};

    impl<U: ?Sized, T: Unsize<U>> CoerceUnsized<My<U>> for My<T> {}
    impl<U: ?Sized, T: Unsize<U>> CoerceUnsized<Dyb<U>> for Dyb<T> {}
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
        let dyb = my.borrow();
        drop(my);
        drop(dyb);
    }

    #[test]
    fn dyb_clone_works() {
        let my = My::new(());
        let dyb = my.borrow();
        let dyb_2 = dyb.clone();
        drop(dyb);
        drop(dyb_2);
    }
    #[test]
    #[should_panic(
        expected = "My pointer dropped with outstanding Dybs - this will leak the resource"
    )]
    fn invalid_drop_order_with_dyb_clone_panics() {
        let my = My::new(());
        let dyb = my.borrow();
        let dyb_2 = dyb.clone();
        drop(dyb);
        drop(my);
        drop(dyb_2);
    }

    #[test]
    fn my_clone_really_clones() {
        struct CloneTrack(usize);
        impl Clone for CloneTrack {
            fn clone(&self) -> Self {
                Self(self.0 + 1)
            }
        }

        let my_1 = My::new(CloneTrack(1));
        let my_2 = my_1.clone();
        assert_eq!(my_1.borrow().0, 1);
        assert_eq!(my_2.borrow().0, 2);
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn can_coerce_unsized() {
        use std::any::Any;
        let my = My::new(());
        let dyb = my.borrow();
        let my_unsized: My<dyn Any> = my;
        let dyb_unsized: Dyb<dyn Any> = dyb;

        assert_eq!(my_unsized.downcast_ref(), Some(&()));
        assert_eq!(dyb_unsized.downcast_ref(), Some(&()));
    }
}
