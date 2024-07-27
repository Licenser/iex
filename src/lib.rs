//! Idiomatic exceptions.
//!
//! Speed up the happy path of your [`Result`]-based functions by seamlessly using exceptions for
//! error propagation.
//!
//! # Crash course
//!
//! Stick [`#[iex]`](macro@iex) on all the functions that return [`Result`] to make them return an
//! efficiently propagatable `#[iex] Result`, apply `?` just like usual, and occasionally call
//! [`.into_result()`](Outcome::into_result) when you need a real [`Result`]. It's that intuitive.
//!
//! Compared to an algebraic [`Result`], `#[iex] Result` is asymmetric: it sacrifices the
//! performance of error handling, and in return:
//! - Gets rid of branching in the happy path,
//! - Reduces memory usage by never explicitly storing the error or the enum discriminant,
//! - Enables the compiler to use registers instead of memory when wrapping small objects in [`Ok`],
//! - Cleanly separates the happy and unhappy paths in the machine code, resulting in better
//!   instruction locality.
//!
//! # Benchmark
//!
//! As a demonstration, we have rewritten [serde](https://serde.rs) and
//! [serde_json](https://crates.io/crates/serde_json) to use `#[iex]` in the deserialization path
//! and used the [Rust JSON Benchmark](https://github.com/serde-rs/json-benchmark) to compare
//! performance. These are the results:
//!
//! <table width="100%">
//!     <thead>
//!         <tr>
//!             <td rowspan="2">Speed (MB/s)</td>
//!             <th colspan="2"><code>canada</code></th>
//!             <th colspan="2"><code>citm_catalog</code></th>
//!             <th colspan="2"><code>twitter</code></th>
//!         </tr>
//!         <tr>
//!             <th>DOM</th>
//!             <th>struct</th>
//!             <th>DOM</th>
//!             <th>struct</th>
//!             <th>DOM</th>
//!             <th>struct</th>
//!         </tr>
//!     </thead>
//!     <tbody>
//!         <tr>
//!             <td><a href="https://doc.rust-lang.org/nightly/core/result/enum.Result.html"><code>Result</code></a></td>
//!             <td align="center">282.4</td>
//!             <td align="center">404.2</td>
//!             <td align="center">363.8</td>
//!             <td align="center">907.8</td>
//!             <td align="center">301.2</td>
//!             <td align="center">612.4</td>
//!         </tr>
//!         <tr>
//!             <td><code>#[iex] Result</code></td>
//!             <td align="center">282.4</td>
//!             <td align="center">565.0</td>
//!             <td align="center">439.4</td>
//!             <td align="center">1025.4</td>
//!             <td align="center">317.6</td>
//!             <td align="center">657.8</td>
//!         </tr>
//!         <tr>
//!             <td>Performance increase</td>
//!             <td align="center">0%</td>
//!             <td align="center">+40%</td>
//!             <td align="center">+21%</td>
//!             <td align="center">+13%</td>
//!             <td align="center">+5%</td>
//!             <td align="center">+7%</td>
//!         </tr>
//!     </tbody>
//! </table>
//!
//! The data is averaged between 5 runs. The repositories for data reproduction are published
//! [on GitHub](https://github.com/orgs/iex-rs/repositories).
//!
//! Note that just blindly slapping [`#[iex]`](macro@iex) onto every single function might not
//! increase your performance at best and will decrease it at worst. Like with every other
//! optimization, it is critical to profile code and measure performance on realistic data.
//!
//! # Example
//!
//! ```
//! use iex::{iex, Outcome};
//!
//! #[iex]
//! fn checked_divide(a: u32, b: u32) -> Result<u32, &'static str> {
//!     if b == 0 {
//!         // Actually raises a custom panic
//!         Err("Cannot divide by zero")
//!     } else {
//!         // Actually returns a / b directly
//!         Ok(a / b)
//!     }
//! }
//!
//! #[iex]
//! fn checked_divide_by_many_numbers(a: u32, bs: &[u32]) -> Result<Vec<u32>, &'static str> {
//!     let mut results = Vec::new();
//!     for &b in bs {
//!         // Actually lets the panic bubble
//!         results.push(checked_divide(a, b)?);
//!     }
//!     Ok(results)
//! }
//!
//! fn main() {
//!     // Actually catches the panic
//!     let result = checked_divide_by_many_numbers(5, &[1, 2, 3, 0]).into_result();
//!     assert_eq!(result, Err("Cannot divide by zero"));
//! }
//! ```
//!
//! # All you need to know
//!
//! Functions marked [`#[iex]`](macro@iex) are supposed to return a [`Result<T, E>`] in their
//! definition. The macro rewrites them to return an opaque type `#[iex] Result<T, E>` instead. This
//! type implements [`Outcome`], so you can call methods like [`map_err`](Outcome::map_err), but
//! other than that, you must immediately propagate the error via `?`.
//!
//! Alternatively, you can cast it to a [`Result`] via [`.into_result()`](Outcome::into_result).
//! This is the only way to avoid immediate propagation.
//!
//! Doing anything else to the return value, e.g. storing it in a variable and using it later will
//! not cause UB, but will not work the way you think either. If you want to swallow the error, use
//! `let _ = func().into_result();` instead.
//!
//! Directly returning an `#[iex] Result` (obtained from a function call) from another
//! [`#[iex]`](macro@iex) function also works, provided that it's the only `return` statement in the
//! function. Use `Ok(..?)` if there are multiple returns.
//!
//! [`#[iex]`](macro@iex) works on methods. If applied to a function in an `impl Trait for Type`
//! block, the corresponding function in the `trait Trait` block should also be marked with
//! [`#[iex]`](macro@iex). Such traits are not object-safe, unless the method is restricted to
//! `where Self: Sized` (open an issue if you want me to spend time developing a workaround).

#![cfg_attr(doc, feature(doc_auto_cfg))]

mod macros;
pub use macros::{iex, try_block};

use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;

mod exception;
use exception::Exception;

mod outcome;
pub use outcome::Outcome;

#[cfg(feature = "anyhow")]
mod anyhow_compat;
#[cfg(feature = "anyhow")]
pub use anyhow_compat::AnyhowContext;

mod iex_result;
mod result;

pub mod example;

struct IexPanic;

thread_local! {
    static EXCEPTION: UnsafeCell<Exception> = const { UnsafeCell::new(Exception::new()) };
}

#[doc(hidden)]
pub mod imp {
    use super::*;

    pub use fix_hidden_lifetime_bug;

    pub trait _IexForward {
        type Output;
        fn _iex_forward(self) -> Self::Output;
    }

    pub struct Marker<E>(PhantomData<E>);

    impl<E> Marker<E> {
        pub(crate) unsafe fn new() -> Self {
            Self(PhantomData)
        }
    }

    impl<E, R: Outcome> _IexForward for &mut (Marker<E>, ManuallyDrop<R>)
    where
        R::Error: Into<E>,
    {
        type Output = R::Output;
        fn _iex_forward(self) -> R::Output {
            let outcome = unsafe { ManuallyDrop::take(&mut self.1) };
            if typeid::of::<E>() == typeid::of::<R::Error>() {
                // SAFETY: If we enter this conditional, E and R::Error differ only in lifetimes.
                // Lifetimes are erased in runtime, so `impl Into<E> for R::Error` has the same
                // implementation as `impl Into<T> for T` for some `T`, and that blanket
                // implementation is a no-op. Therefore, no conversion needs to happen.
                outcome.get_value_or_panic(unsafe { Marker::new() })
            } else {
                let exception_mapper =
                    ExceptionMapper::new(self.0, (), |_, err| Into::<E>::into(err));
                let output = outcome.get_value_or_panic(exception_mapper.get_in_marker());
                exception_mapper.swallow();
                output
            }
        }
    }

    // Autoref specialization for conversion-less forwarding. This *must* be callable without taking
    // a (mutable) reference in user code, so that the LLVM optimizer has less work to do. This
    // actually matters for serde.
    impl<R: Outcome> _IexForward for (Marker<R::Error>, ManuallyDrop<R>) {
        type Output = R::Output;
        fn _iex_forward(self) -> R::Output {
            ManuallyDrop::into_inner(self.1).get_value_or_panic(self.0)
        }
    }

    impl<E> Clone for Marker<E> {
        fn clone(&self) -> Self {
            *self
        }
    }

    impl<E> Copy for Marker<E> {}

    pub struct ExceptionMapper<S, T, U, F: FnOnce(S, T) -> U> {
        state: ManuallyDrop<S>,
        f: ManuallyDrop<F>,
        phantom: PhantomData<fn(S, T) -> U>,
    }

    impl<S, T, U, F: FnOnce(S, T) -> U> ExceptionMapper<S, T, U, F> {
        pub fn new(_marker: Marker<U>, state: S, f: F) -> Self {
            Self {
                state: ManuallyDrop::new(state),
                f: ManuallyDrop::new(f),
                phantom: PhantomData,
            }
        }

        pub fn get_in_marker(&self) -> Marker<T> {
            unsafe { Marker::new() }
        }

        pub fn get_state(&mut self) -> &mut S {
            &mut self.state
        }

        pub fn swallow(mut self) {
            unsafe { ManuallyDrop::drop(&mut self.state) };
            unsafe { ManuallyDrop::drop(&mut self.f) };
            std::mem::forget(self);
        }
    }

    impl<S, T, U, F: FnOnce(S, T) -> U> Drop for ExceptionMapper<S, T, U, F> {
        fn drop(&mut self) {
            // Resolve TLS just once
            EXCEPTION.with(|exception| unsafe {
                let exception = exception.get();
                // Dereference twice instead of keeping a &mut around, because self.0() may call a
                // function that uses 'exception'.
                if let Some(error) = (*exception).read::<T>() {
                    let state = ManuallyDrop::take(&mut self.state);
                    let f = ManuallyDrop::take(&mut self.f);
                    (*exception).write::<U>(f(state, error));
                }
            })
        }
    }

    pub use iex_result::IexResult;

    pub struct NoCopy;
}

extern crate self as iex;

#[cfg(test)]
mod test {
    use super::*;

    #[iex]
    fn marker_and_no_copy(marker: i32, no_copy: i32) -> Result<i32, ()> {
        Ok(marker + no_copy)
    }
}
