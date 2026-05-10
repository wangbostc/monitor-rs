use std::sync::Arc;

use egui::{Color32, Vec2};
use parking_lot::RwLock;

use crate::sample::{MemPressure, Sample};
use crate::settings::Settings;
use crate::store::SampleStore;
use crate::ui::cores::core_grid;
use crate::ui::procs::process_list;
use crate::ui::sparkline::{normalize, sparkline};

pub struct PopoverState {
    pub store: Arc<RwLock<SampleStore>>,
    pub settings: Settings,
}

pub fn show(ui: &mut egui::Ui, state: &PopoverState) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        ui.set_width(280.0);
        let store = state.store.read();
        let Some(latest) = store.latest().cloned() else {
            ui.label("Sampling…");
            return;
        };
        let recent_n = (60.0 * state.settings.sample_rate_hz).ceil() as usize;

        // CPU
        row(ui, "CPU", latest.cpu_total, Color32::from_rgb(80, 200, 120),
            &collect(&store, recent_n, |s| s.cpu_total));
        core_grid(ui, &latest.cpu_per_core);

        ui.add_space(6.0);

        // GPU
        let gpu_label = match latest.gpu_pct {
            Some(p) => format!("{:>3.0}%", p),
            None => "n/a".into(),
        };
        row_label(ui, "GPU", &gpu_label, Color32::from_rgb(120, 160, 240),
            &collect(&store, recent_n, |s| s.gpu_pct.unwrap_or(0.0)),
            latest.gpu_pct.is_some());

        ui.add_space(6.0);

        // MEM
        let mem_pct = latest.mem.used_pct();
        let mem_color = match latest.mem.pressure {
            MemPressure::Normal => Color32::from_rgb(200, 180, 80),
            MemPressure::Warning => Color32::from_rgb(220, 140, 60),
            MemPressure::Critical => Color32::from_rgb(220, 80, 80),
        };
        row(ui, "MEM", mem_pct, mem_color,
            &collect(&store, recent_n, |s| s.mem.used_pct()));

        ui.add_space(8.0);
        ui.separator();
        ui.label(egui::RichText::new("Top processes").strong());
        process_list(ui, &latest.top_procs);

        ui.separator();
        ui.horizontal(|ui| {
            ui.label(format!("swap {}", format_swap(&latest.swap)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
    });
}

fn row(ui: &mut egui::Ui, label: &str, pct: f32, color: Color32, history: &[f32]) {
    ui.horizontal(|ui| {
        ui.label(format!("{label} {:>3.0}%", pct));
        let normed = normalize(history, 100.0);
        sparkline(ui, Vec2::new(180.0, 18.0), &normed, color);
    });
}

fn row_label(ui: &mut egui::Ui, label: &str, value: &str, color: Color32, history: &[f32], have_data: bool) {
    ui.horizontal(|ui| {
        ui.label(format!("{label} {value}"));
        if have_data {
            let normed = normalize(history, 100.0);
            sparkline(ui, Vec2::new(180.0, 18.0), &normed, color);
        }
    });
}

fn collect<F: Fn(&Sample) -> f32>(store: &SampleStore, n: usize, f: F) -> Vec<f32> {
    store.recent(n).map(f).collect()
}

pub fn format_swap(swap: &crate::sample::SwapInfo) -> String {
    if swap.total_bytes == 0 { return "off".into(); }
    let used_g = swap.used_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    format!("{:.2}G", used_g)
}
