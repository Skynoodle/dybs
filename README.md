# Dybs

An experiment in dynamic single-owner, multiple-borrow smart pointers

## Why?

Rust's Borrow Checker usually statically enforces single-owner, multiple-borrow
semantics. Standard library smart pointers such as `Rc` extend this to provide
a form of _dynamic_ borrow checking, but they do so in a way that also results
in allowing multiple owners while also subtly shifting responsibility for
checking the lifetimes.

In the static borrow checker case, accessing a borrowed value (reference) can't
fail: lifetimes enforce that the owner must keep the value alive long enough
to satisfy any outstanding borrows. In the dynamic case, this is reversed:
it becomes the responsibility of the holder of a borrowed value (a weak pointer)
to handle the possibility that the underlying value has been invalidated by its
owner(s).

Dybs investigates a model closer to a dynamic version of the borrow checker's
behaviour: Values retain exactly one owner, which can provide runtime-checked
borrows of that value and which takes on responsibility for ensuring the value
remains valid for the duration of any borrows. This has the consequence that
dropping the owning pointer can _fail_ at runtime if there exist any outstanding
borrows.