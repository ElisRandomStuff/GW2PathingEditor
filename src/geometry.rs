//! Positions in MumbleLink map space: metres, X east, Y up, Z north (docs/01 §6.1).

/// Straight-line distance in metres.
pub fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = sub(a, b);
    dot(d, d).sqrt()
}

pub fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// The point a fraction `t` of the way from `a` to `b`.
pub fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    add(a, scale(sub(b, a), t))
}

pub fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// `a` scaled to length 1, if it has a length.
pub fn normalize(a: [f32; 3]) -> Option<[f32; 3]> {
    let len = dot(a, a).sqrt();
    (len > 1e-6 && len.is_finite()).then(|| scale(a, 1.0 / len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances() {
        assert_eq!(distance([0.0; 3], [3.0, 4.0, 0.0]), 5.0);
        assert_eq!(distance([1.0, 2.0, 3.0], [1.0, 2.0, 3.0]), 0.0);
    }

    #[test]
    fn vectors() {
        assert_eq!(lerp([0.0; 3], [2.0, 4.0, 6.0], 0.5), [1.0, 2.0, 3.0]);
        assert_eq!(cross([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), [0.0, 0.0, 1.0]);
        assert_eq!(normalize([0.0, 3.0, 4.0]), Some([0.0, 0.6, 0.8]));
        assert_eq!(normalize([0.0; 3]), None);
    }
}
