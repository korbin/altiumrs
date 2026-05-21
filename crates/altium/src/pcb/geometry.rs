//! Geometry helpers for PCB outlines (custom-shape pads, paste-mask
//! expansion, polygon-pour fragments).

use crate::coord::{Coord, CoordPoint};

/// Twice the signed area of a polygon. Positive = CCW, negative = CW.
/// Operates in raw `i32` units; the caller normalises units.
fn signed_area_x2(pts: &[(f64, f64)]) -> f64 {
    let n = pts.len();
    let mut a = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        a += pts[i].0 * pts[j].1 - pts[j].0 * pts[i].1;
    }
    a
}

/// Inflate (positive `offset`) or shrink (negative) a polygon by `offset`
/// raw coord units. Returns `None` for degenerate inputs (fewer than 3
/// vertices, zero signed area) or when a shrink collapses the polygon.
///
/// Corners stay sharp (miter join). Real Altium paste-mask expansion is
/// applied to rectilinear copper outlines where this matches the
/// reference output; arbitrary curved outlines would need a rounded
/// join, which is out of scope here.
///
/// Vertex order and count are preserved.
pub fn offset_polygon(outline: &[CoordPoint], offset: Coord) -> Option<Vec<CoordPoint>> {
    if outline.len() < 3 {
        return None;
    }
    let n = outline.len();
    let offset_f = offset.to_raw() as f64;

    let pts: Vec<(f64, f64)> = outline
        .iter()
        .map(|p| (p.x.to_raw() as f64, p.y.to_raw() as f64))
        .collect();

    let area_x2 = signed_area_x2(&pts);
    if area_x2.abs() < 1.0 {
        return None;
    }
    let ccw = area_x2 > 0.0;

    // Outward unit normal per edge. For a CCW polygon, rotating the edge
    // direction 90° CW points outward; CW polygons use the opposite.
    let normals: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            let j = (i + 1) % n;
            let dx = pts[j].0 - pts[i].0;
            let dy = pts[j].1 - pts[i].1;
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-6 {
                return (0.0, 0.0);
            }
            if ccw {
                (dy / len, -dx / len)
            } else {
                (-dy / len, dx / len)
            }
        })
        .collect();

    // For each vertex, intersect the two offset lines that bound it.
    // Line A is edge[prev] translated outward by `offset`; line B is
    // edge[i] translated the same way. Their intersection is the new
    // vertex position.
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = (i + n - 1) % n;
        let next = (i + 1) % n;

        let na = normals[prev];
        let ax = pts[i].0 + offset_f * na.0;
        let ay = pts[i].1 + offset_f * na.1;
        let adx = pts[i].0 - pts[prev].0;
        let ady = pts[i].1 - pts[prev].1;

        let nb = normals[i];
        let bx = pts[i].0 + offset_f * nb.0;
        let by = pts[i].1 + offset_f * nb.1;
        let bdx = pts[next].0 - pts[i].0;
        let bdy = pts[next].1 - pts[i].1;

        let cross = adx * bdy - ady * bdx;
        let (vx, vy) = if cross.abs() < 1e-9 {
            // Adjacent edges are parallel (collinear vertex / 180° fold).
            // Slide the vertex along the average of the two normals so
            // it tracks the offset distance.
            let mx = (na.0 + nb.0) / 2.0;
            let my = (na.1 + nb.1) / 2.0;
            (pts[i].0 + offset_f * mx, pts[i].1 + offset_f * my)
        } else {
            // Standard 2D line-line intersection. `t` is the parameter
            // along line A from (ax, ay) in direction (adx, ady).
            let t = ((bx - ax) * bdy - (by - ay) * bdx) / cross;
            (ax + t * adx, ay + t * ady)
        };
        out.push(CoordPoint::new(
            Coord::from_raw(vx.round() as i32),
            Coord::from_raw(vy.round() as i32),
        ));
    }

    // Shrink-too-far detection. If any output edge points opposite the
    // input edge, the offset crossed the medial axis and the result is
    // degenerate.
    if offset_f < 0.0 {
        for i in 0..n {
            let j = (i + 1) % n;
            let orig_dx = pts[j].0 - pts[i].0;
            let orig_dy = pts[j].1 - pts[i].1;
            let new_dx = out[j].x.to_raw() as f64 - out[i].x.to_raw() as f64;
            let new_dy = out[j].y.to_raw() as f64 - out[i].y.to_raw() as f64;
            if orig_dx * new_dx + orig_dy * new_dy < 0.0 {
                return None;
            }
        }
    }

    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(side: i32) -> Vec<CoordPoint> {
        // CCW square centred on the origin.
        vec![
            CoordPoint::new(Coord::from_raw(-side / 2), Coord::from_raw(-side / 2)),
            CoordPoint::new(Coord::from_raw(side / 2), Coord::from_raw(-side / 2)),
            CoordPoint::new(Coord::from_raw(side / 2), Coord::from_raw(side / 2)),
            CoordPoint::new(Coord::from_raw(-side / 2), Coord::from_raw(side / 2)),
        ]
    }

    #[test]
    fn outward_offset_grows_square() {
        let inflated = offset_polygon(&square(100), Coord::from_raw(10)).expect("offset ok");
        assert_eq!(inflated.len(), 4);
        // Each corner pushed outward by 10 raw units along both axes.
        assert_eq!(inflated[0].x.to_raw(), -60);
        assert_eq!(inflated[0].y.to_raw(), -60);
        assert_eq!(inflated[2].x.to_raw(), 60);
        assert_eq!(inflated[2].y.to_raw(), 60);
    }

    #[test]
    fn inward_offset_shrinks_square() {
        let shrunk = offset_polygon(&square(100), Coord::from_raw(-10)).expect("offset ok");
        assert_eq!(shrunk[0].x.to_raw(), -40);
        assert_eq!(shrunk[2].x.to_raw(), 40);
    }

    #[test]
    fn excessive_shrink_returns_none() {
        // Shrink by half the side length collapses the square to a point.
        // The dot-product check should trip.
        assert!(offset_polygon(&square(100), Coord::from_raw(-200)).is_none());
    }

    #[test]
    fn cw_winding_offsets_outward_too() {
        // Same square wound clockwise — outward direction flips, but the
        // result should be identical.
        let cw: Vec<_> = square(100).into_iter().rev().collect();
        let inflated = offset_polygon(&cw, Coord::from_raw(10)).expect("offset ok");
        let max_x = inflated.iter().map(|p| p.x.to_raw()).max().unwrap();
        let min_x = inflated.iter().map(|p| p.x.to_raw()).min().unwrap();
        assert_eq!(max_x - min_x, 120);
    }

    #[test]
    fn zero_offset_is_identity() {
        let s = square(100);
        let out = offset_polygon(&s, Coord::ZERO).expect("offset ok");
        for (a, b) in s.iter().zip(out.iter()) {
            assert_eq!(a.x.to_raw(), b.x.to_raw());
            assert_eq!(a.y.to_raw(), b.y.to_raw());
        }
    }

    #[test]
    fn too_few_vertices_returns_none() {
        assert!(offset_polygon(&[], Coord::from_raw(1)).is_none());
        assert!(
            offset_polygon(
                &[CoordPoint::ZERO, CoordPoint::new(Coord::from_raw(1), Coord::ZERO)],
                Coord::from_raw(1),
            )
            .is_none()
        );
    }

    #[test]
    fn l_shape_offsets_correctly() {
        // L-shape CCW:
        //   (0,0) → (20,0) → (20,10) → (10,10) → (10,20) → (0,20) → close
        let l = vec![
            CoordPoint::new(Coord::from_raw(0), Coord::from_raw(0)),
            CoordPoint::new(Coord::from_raw(20), Coord::from_raw(0)),
            CoordPoint::new(Coord::from_raw(20), Coord::from_raw(10)),
            CoordPoint::new(Coord::from_raw(10), Coord::from_raw(10)),
            CoordPoint::new(Coord::from_raw(10), Coord::from_raw(20)),
            CoordPoint::new(Coord::from_raw(0), Coord::from_raw(20)),
        ];
        let inflated = offset_polygon(&l, Coord::from_raw(2)).expect("offset ok");
        assert_eq!(inflated.len(), 6);
        // The concave corner at (10,10) moves *inward* (toward the
        // interior) — its post-offset position is at (12, 12).
        assert_eq!(inflated[3].x.to_raw(), 12);
        assert_eq!(inflated[3].y.to_raw(), 12);
        // The (0,0) corner moves outward by 2 on each axis.
        assert_eq!(inflated[0].x.to_raw(), -2);
        assert_eq!(inflated[0].y.to_raw(), -2);
    }
}
