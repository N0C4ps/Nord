//! Shared application diagnostics: contextual errors and invariant assertions.
//!
//! Use [`Result<T>`] for failures that callers can
//! handle or propagate. Add operation context with [`Context`] at boundaries
//! that know what was being attempted. Keep reusable crates' public error types
//! concrete; use this dynamic error chain while composing application behavior.

pub use anyhow::{Context, Error, Result, anyhow, bail, ensure};

/// Return an error unless two values compare equal, evaluating each once.
#[macro_export]
macro_rules! ensure_eq {
    ($left:expr, $right:expr $(,)?) => {
        $crate::ensure_eq!($left, $right, "values must be equal")
    };
    ($left:expr, $right:expr, $($argument:tt)+) => {{
        let left = &$left;
        let right = &$right;
        if left != right {
            $crate::bail!(
                "assertion failed: `(left == right)`\n  left: `{:?}`\n right: `{:?}`\n  reason: {}",
                left,
                right,
                format_args!($($argument)+)
            );
        }
    }};
}

/// Return an error unless two values compare unequal, evaluating each once.
#[macro_export]
macro_rules! ensure_ne {
    ($left:expr, $right:expr $(,)?) => {
        $crate::ensure_ne!($left, $right, "values must differ")
    };
    ($left:expr, $right:expr, $($argument:tt)+) => {{
        let left = &$left;
        let right = &$right;
        if left == right {
            $crate::bail!(
                "assertion failed: `(left != right)`\n  left: `{:?}`\n right: `{:?}`\n  reason: {}",
                left,
                right,
                format_args!($($argument)+)
            );
        }
    }};
}

/// Assert a programmer invariant in every build configuration.
///
/// Use this only when continuing would mean the program is operating on an
/// invalid internal state. Use [`ensure!`] for expected failures that should be
/// returned to the caller instead of panicking.
#[macro_export]
macro_rules! invariant {
    ($condition:expr $(,)?) => {
        assert!(
            $condition,
            "Nord invariant failed: `{}` at {}:{}",
            stringify!($condition),
            file!(),
            line!()
        )
    };
    ($condition:expr, $($argument:tt)+) => {
        assert!(
            $condition,
            "Nord invariant failed at {}:{}: {}",
            file!(),
            line!(),
            format_args!($($argument)+)
        )
    };
}

/// Assert an internal invariant in debug builds only.
///
/// The condition and message are not evaluated in optimized builds without
/// debug assertions. Never put required side effects inside this macro.
#[macro_export]
macro_rules! debug_invariant {
    ($condition:expr $(,)?) => {
        debug_assert!(
            $condition,
            "Nord debug invariant failed: `{}` at {}:{}",
            stringify!($condition),
            file!(),
            line!()
        )
    };
    ($condition:expr, $($argument:tt)+) => {
        debug_assert!(
            $condition,
            "Nord debug invariant failed at {}:{}: {}",
            file!(),
            line!(),
            format_args!($($argument)+)
        )
    };
}
