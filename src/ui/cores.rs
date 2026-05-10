use egui::{Color32, Sense, Ui};

/// A row of small blocks, one per core, color-mapped by usage (0..=100).
pub fn core_grid(ui: &mut Ui, per_core: &[f32]) {
    let n = per_core.len().max(1);
    let total_w = ui.available_width().min(260.0);
    let gap = 2.0;
    let block_w = ((total_w - gap * (n as f32 - 1.0)) / n as f32).max(4.0);
    let block_h = 14.0;

    ui.horizontal(|ui| {
        for &p in per_core {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(block_w, block_h), Sense::hover());
            ui.painter().rect_filled(rect, 2.0, color_for_pct(p));
        }
    });
}

fn color_for_pct(p: f32) -> Color32 {
    let p = p.clamp(0.0, 100.0) / 100.0;
    // green -> yellow -> red
    let (r, g) = if p < 0.5 {
        (lerp(40.0, 200.0, p / 0.5), 200.0)
    } else {
        (220.0, lerp(200.0, 40.0, (p - 0.5) / 0.5))
    };
    Color32::from_rgb(r as u8, g as u8, 60)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t.clamp(0.0, 1.0) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_for_pct_endpoints() {
        let lo = color_for_pct(0.0);
        let hi = color_for_pct(100.0);
        assert_ne!(lo, hi);
    }

    #[test]
    fn lerp_basic() {
        assert!((lerp(0.0, 10.0, 0.5) - 5.0).abs() < 0.001);
        assert!((lerp(0.0, 10.0, -1.0) - 0.0).abs() < 0.001);
        assert!((lerp(0.0, 10.0, 2.0) - 10.0).abs() < 0.001);
    }
}
