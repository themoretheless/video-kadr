//! Aspect-ratio layout selection from Filmcraft ui-egui scopes::layout (MIT).
use serde::Serialize;
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Layout {
    pub columns: usize,
    pub rows: usize,
}
pub fn choose(width: f32, height: f32, count: usize) -> Layout {
    if count == 0 {
        return Layout {
            columns: 0,
            rows: 0,
        };
    }
    let columns = (1..=count)
        .min_by(|a, b| {
            let score = |cols: usize| {
                let rows = count.div_ceil(cols);
                let aspect = (width / cols as f32) / (height / rows as f32).max(1.0);
                (aspect / 1.33).ln().abs() + if cols * rows > count { 0.15 } else { 0.0 }
            };
            score(*a).total_cmp(&score(*b))
        })
        .unwrap_or(1);
    Layout {
        columns,
        rows: count.div_ceil(columns),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adapts_to_portrait_landscape_and_incomplete_rows() {
        assert_eq!(choose(200.0, 600.0, 4).columns, 1);
        assert_eq!(choose(1000.0, 200.0, 4).columns, 4);
        assert_eq!(choose(600.0, 440.0, 4).columns, 2);
        let three = choose(600.0, 440.0, 3);
        assert_eq!((three.columns, three.rows), (2, 2));
        assert_eq!(choose(600.0, 440.0, 0).rows, 0);
    }
}
