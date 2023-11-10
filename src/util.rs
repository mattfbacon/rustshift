/// `t` should be in the range `0.0..=1.0` for a typical lerp,
/// but does not strictly have to be.
pub fn lerp(from: f32, to: f32, t: f32) -> f32 {
	from * (1.0 - t) + to * t
}
