//! Squarified treemap layout (Bruls, Huizing, van Wijk, 1999).

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Lays out `items` (already sorted largest-first, as `(id, size)` pairs)
/// inside `rect`, returning one rectangle per item in the same order.
/// Items with a size of 0 are skipped.
pub fn layout(items: &[(i32, u64)], rect: Rect) -> Vec<(i32, Rect)> {
    let mut out = Vec::with_capacity(items.len());
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return out;
    }
    let total: f64 = items.iter().map(|&(_, s)| s as f64).sum();
    if total <= 0.0 {
        return out;
    }
    let area = rect.w as f64 * rect.h as f64;
    let scaled: Vec<(i32, f64)> = items
        .iter()
        .filter(|&&(_, s)| s > 0)
        .map(|&(id, s)| (id, s as f64 / total * area))
        .collect();

    squarify(&scaled, rect, &mut out);
    out
}

fn squarify(items: &[(i32, f64)], rect: Rect, out: &mut Vec<(i32, Rect)>) {
    if items.is_empty() {
        return;
    }
    if items.len() == 1 || rect.w <= 0.0 || rect.h <= 0.0 {
        out.push((items[0].0, rect));
        out.extend(items[1..].iter().map(|&(id, _)| (id, Rect { x: rect.x, y: rect.y, w: 0.0, h: 0.0 })));
        return;
    }

    let side = rect.w.min(rect.h) as f64;

    let mut split = 1;
    while split < items.len() {
        let with_next = worst_ratio(&items[..split + 1], side);
        let without_next = worst_ratio(&items[..split], side);
        if with_next > without_next {
            break;
        }
        split += 1;
    }
    let row = &items[..split];
    let rest = &items[split..];
    let row_sum: f64 = row.iter().map(|&(_, s)| s).sum();

    if rect.w >= rect.h {
        let col_w = (row_sum / rect.h as f64) as f32;
        let mut y = rect.y;
        for &(id, s) in row {
            let h = (s / row_sum) as f32 * rect.h;
            out.push((id, Rect { x: rect.x, y, w: col_w, h }));
            y += h;
        }
        squarify(
            rest,
            Rect { x: rect.x + col_w, y: rect.y, w: (rect.w - col_w).max(0.0), h: rect.h },
            out,
        );
    } else {
        let row_h = (row_sum / rect.w as f64) as f32;
        let mut x = rect.x;
        for &(id, s) in row {
            let w = (s / row_sum) as f32 * rect.w;
            out.push((id, Rect { x, y: rect.y, w, h: row_h }));
            x += w;
        }
        squarify(
            rest,
            Rect { x: rect.x, y: rect.y + row_h, w: rect.w, h: (rect.h - row_h).max(0.0) },
            out,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_full_area_and_preserves_ids() {
        let items = [(0, 400u64), (1, 300), (2, 200), (3, 100)];
        let rect = Rect { x: 0.0, y: 0.0, w: 200.0, h: 100.0 };
        let rects = layout(&items, rect);

        assert_eq!(rects.len(), items.len());
        let mut ids: Vec<i32> = rects.iter().map(|&(id, _)| id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![0, 1, 2, 3]);

        let total_area: f64 = rects.iter().map(|&(_, r)| r.w as f64 * r.h as f64).sum();
        let expected_area = rect.w as f64 * rect.h as f64;
        assert!((total_area - expected_area).abs() < 1.0, "total_area={total_area} expected={expected_area}");

        for (_, r) in &rects {
            assert!(r.x >= rect.x - 0.01 && r.y >= rect.y - 0.01);
            assert!(r.x + r.w <= rect.x + rect.w + 0.5);
            assert!(r.y + r.h <= rect.y + rect.h + 0.5);
        }
    }

    #[test]
    fn empty_and_zero_size_inputs_are_safe() {
        let rect = Rect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 };
        assert!(layout(&[], rect).is_empty());
        assert!(layout(&[(0, 0)], rect).is_empty());
        assert!(layout(&[(0, 10)], Rect { x: 0.0, y: 0.0, w: 0.0, h: 100.0 }).is_empty());
    }

    #[test]
    fn single_item_fills_rect() {
        let rect = Rect { x: 5.0, y: 5.0, w: 50.0, h: 40.0 };
        let rects = layout(&[(7, 123)], rect);
        assert_eq!(rects.len(), 1);
        let (id, r) = rects[0];
        assert_eq!(id, 7);
        assert_eq!(r.x, rect.x);
        assert_eq!(r.y, rect.y);
        assert_eq!(r.w, rect.w);
        assert_eq!(r.h, rect.h);
    }
}

fn worst_ratio(row: &[(i32, f64)], side: f64) -> f64 {
    let sum: f64 = row.iter().map(|&(_, s)| s).sum();
    if sum <= 0.0 || side <= 0.0 {
        return f64::INFINITY;
    }
    let max = row.iter().map(|&(_, s)| s).fold(f64::MIN, f64::max);
    let min = row.iter().map(|&(_, s)| s).fold(f64::MAX, f64::min);
    let side2 = side * side;
    let sum2 = sum * sum;
    ((side2 * max) / sum2).max(sum2 / (side2 * min))
}
