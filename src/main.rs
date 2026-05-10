use monitor_rs::sampler::SamplerHandle;
use monitor_rs::settings::Settings;
use monitor_rs::ui::popover::{self, PopoverState};

struct App {
    state: PopoverState,
    _sampler: SamplerHandle,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        popover::show(&ctx, &self.state);
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

fn main() -> eframe::Result<()> {
    let _log_guard = monitor_rs::logging::init();
    tracing::info!("monitor-rs starting");

    let settings = Settings::load();
    let handle = SamplerHandle::spawn(settings.clone());
    let store = handle.store.clone();
    let app = App {
        state: PopoverState { store, settings },
        _sampler: handle,
    };

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([300.0, 360.0])
            .with_min_inner_size([280.0, 320.0])
            .with_title("monitor-rs"),
        ..Default::default()
    };

    eframe::run_native("monitor-rs", opts, Box::new(|_cc| Ok(Box::new(app))))
}
