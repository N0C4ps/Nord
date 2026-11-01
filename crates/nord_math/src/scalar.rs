//! Scalar helpers with explicit behavior around invalid floating-point values.

/// Linear interpolation. `factor` is intentionally not clamped to `[0, 1]`.
#[must_use]
pub fn lerp(start: f32, end: f32, factor: f32) -> f32 {
    start + (end - start) * factor
}

/// Return the normalized position of `value` in `[start, end]`.
///
/// Returns `None` for a zero-width interval or non-finite inputs. The result is
/// not clamped and therefore supports extrapolation.
#[allow(clippy::float_cmp)] // Exact equality is the correct zero-width interval check.
#[must_use]
pub fn inverse_lerp(start: f32, end: f32, value: f32) -> Option<f32> {
    if !start.is_finite() || !end.is_finite() || !value.is_finite() || start == end {
        return None;
    }
    let factor = (value - start) / (end - start);
    factor.is_finite().then_some(factor)
}

/// Map a value from one interval to another, allowing extrapolation.
#[must_use]
pub fn remap(value: f32, input: (f32, f32), output: (f32, f32)) -> Option<f32> {
    let factor = inverse_lerp(input.0, input.1, value)?;
    let mapped = lerp(output.0, output.1, factor);
    mapped.is_finite().then_some(mapped)
}

/// Cubic Hermite interpolation between zero and one.
///
/// The input is clamped to `[0, 1]`; non-finite values produce `NaN` so invalid
/// simulation state remains visible to diagnostics instead of being hidden.
#[must_use]
pub fn smoothstep(value: f32) -> f32 {
    if !value.is_finite() {
        return f32::NAN;
    }
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Compare two `f32` values using combined absolute and relative tolerances.
#[must_use]
pub fn approx_eq_f32(left: f32, right: f32, absolute: f32, relative: f32) -> bool {
    if !left.is_nan() && left.to_bits() == right.to_bits() {
        return true;
    }
    if !left.is_finite()
        || !right.is_finite()
        || !absolute.is_finite()
        || !relative.is_finite()
        || absolute < 0.0
        || relative < 0.0
    {
        return false;
    }
    (left - right).abs() <= absolute.max(relative * left.abs().max(right.abs()))
}

/// Compare two `f64` values using combined absolute and relative tolerances.
#[must_use]
pub fn approx_eq_f64(left: f64, right: f64, absolute: f64, relative: f64) -> bool {
    if !left.is_nan() && left.to_bits() == right.to_bits() {
        return true;
    }
    if !left.is_finite()
        || !right.is_finite()
        || !absolute.is_finite()
        || !relative.is_finite()
        || absolute < 0.0
        || relative < 0.0
    {
        return false;
    }
    (left - right).abs() <= absolute.max(relative * left.abs().max(right.abs()))
}

/// Wrap an angle in radians to the half-open range `[-π, π)`.
#[must_use]
pub fn wrap_angle_pi(angle: f32) -> Option<f32> {
    if !angle.is_finite() {
        return None;
    }
    let wrapped =
        (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    wrapped.is_finite().then_some(wrapped)
}
