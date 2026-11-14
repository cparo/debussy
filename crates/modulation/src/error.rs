//! The errors this crate returns.

use thiserror::Error;

/// Why a list of points is not a valid [`Curve`](crate::Curve)
/// (`docs/architecture/automation.md` "Curves").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CurveError {
    /// The point's frame is not after the frame of the point before it.
    #[error("point {point} is not after the point before it")]
    NotIncreasing {
        /// The point's index.
        point: usize,
    },
    /// The point's value is not a finite number from 0 to 1.
    #[error("point {point} has a value outside 0 to 1")]
    ValueOutOfRange {
        /// The point's index.
        point: usize,
    },
    /// One of the point's handle offsets is not finite.
    #[error("point {point} has a handle that is not finite")]
    NonFiniteHandle {
        /// The point's index.
        point: usize,
    },
    /// The segment's handles make it turn back in time, so it is not a function of time.
    #[error("segment {segment} turns back in time")]
    NotFunctionOfTime {
        /// The segment's index: segment `i` joins point `i` to point `i + 1`.
        segment: usize,
    },
}
