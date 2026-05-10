use egui::{Color32, Pos2, Response, Sense, Stroke, Ui};

/// Normalize a series to 0..=1 against a fixed max (e.g. 100.0 for percentages).
pub fn normalize(values: &[f32], max: f32) -> Vec<f32> {
    if max <= 0.0 {
        return vec![0.0; values.len()];
    }
    values.iter().map(|v| (v / max).clamp(0.0, 1.0)).collect()
}

/// Render a sparkline filling the allocated rect.
/// `values` should already be normalized to 0..=1.
pub fn sparkline(ui: &mut Ui, size: egui::Vec2, values: &[f32], color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);

    // Background.
    painter.rect_filled(rect, 4.0, ui.style().visuals.faint_bg_color);

    if values.is_empty() {
        return response;
    }

    let n = values.len();
    let dx = if n > 1 { rect.width() / (n - 1) as f32 } else { 0.0 };
    let points: Vec<Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = rect.left() + dx * i as f32;
            let y = rect.bottom() - rect.height() * v.clamp(0.0, 1.0);
            Pos2::new(x, y)
        })
        .collect();

    // Filled area under the line.
    if points.len() >= 2 {
        let mut poly = points.clone();
        poly.push(Pos2::new(rect.right(), rect.bottom()));
        poly.push(Pos2::new(rect.left(), rect.bottom()));
        painter.add(egui::Shape::convex_polygon(poly, color.linear_multiply(0.25), Stroke::NONE));
    }

    // Line.
    if points.len() >= 2 {
        painter.add(egui::Shape::line(points, Stroke::new(1.5, color)));
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_clamps_and_scales() {
        let r = normalize(&[0.0, 50.0, 100.0, 150.0, -10.0], 100.0);
        assert_eq!(r, vec![0.0, 0.5, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn normalize_handles_zero_max() {
        let r = normalize(&[10.0, 20.0], 0.0);
        assert_eq!(r, vec![0.0, 0.0]);
    }
}
