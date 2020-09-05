pub struct Foo<T> {
    count: std::sync::atomic::AtomicUsize,
    _pinned: std::marker::PhantomPinned,
    data: T,
}

pub struct Dyb<'l, T> {
    foo: &'l Foo<T>,
}

impl<T> Foo<T> {
    fn borrow<'l>(self: std::pin::Pin<&Self>) -> Dyb<'l, T> {
        let this: &Foo<T> = &*self;
        self.count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        // Transmuting here to (potentially) extend the lifetime of our
        // reference to self here
        // Safety: From the atomic increment above, dropping this Foo will
        // abort. This will only be decremented when the Dyb, and thus the
        // 'l reference lifetime, is over. If foo _doesn't_ really outlive
        // the 'l, we'll therefore abort on the early drop.
        // XXX: This assumes Drop runs. This is not sound.
        let foo: &'l Foo<T> = unsafe { std::mem::transmute(this) };
        Dyb { foo }
    }
}

impl<T> Drop for Foo<T> {
    fn drop(&mut self) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
