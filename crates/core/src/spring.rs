//! Critically damped smoothing, shared with ScreenCraft's camera.

/// Critically damped smoothing toward `target` (Unity-style SmoothDamp).
/// `smooth_time` is roughly the time in seconds to reach the target.
pub fn smooth_damp(current: f64, target: f64, velocity: &mut f64, smooth_time: f64, dt: f64) -> f64 {
    let st = smooth_time.max(1e-3);
    let omega = 2.0 / st;
    let x = omega * dt;
    let e = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
    let change = current - target;
    let temp = (*velocity + omega * change) * dt;
    *velocity = (*velocity - omega * temp) * e;
    target + (change + temp) * e
}

/// Smootherstep easing, 0..1 -> 0..1 with zero velocity at both ends.
pub fn ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}
