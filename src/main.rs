use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use monitor_rs::sampler::SamplerHandle;
use monitor_rs::settings::Settings;
use monitor_rs::ui::popover::{self, PopoverState};

#[cfg(target_os = "macos")]
use monitor_rs::ui::tray::Tray;

struct App {
    state: PopoverState,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    visible: Arc<AtomicBool>,
    #[cfg(target_os = "macos")]
    tray: Option<Tray>,
    #[cfg(target_os = "macos")]
    last_refresh: std::time::Instant,
    _sampler: SamplerHandle,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Lazy-create the tray on the first frame — by now NSApplication is initialized.
        #[cfg(target_os = "macos")]
        if self.tray.is_none()
            && let Some(mtm) = objc2_foundation::MainThreadMarker::new()
        {
            let visible = self.visible.clone();
            let ctx_clone = ctx.clone();
            let cb: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                let was = visible.fetch_xor(true, Ordering::Relaxed);
                let now_visible = !was;
                ctx_clone.send_viewport_cmd(egui::ViewportCommand::Visible(now_visible));
            });
            self.tray = Some(Tray::new(cb, mtm));
        }

        // Refresh the tray title up to 4×/sec.
        #[cfg(target_os = "macos")]
        if self.last_refresh.elapsed() >= std::time::Duration::from_millis(250) {
            if let (Some(tray), Some(mtm)) = (
                self.tray.as_ref(),
                objc2_foundation::MainThreadMarker::new(),
            ) {
                tray.refresh(&self.state.store.read(), &self.state.settings, mtm);
            }
            self.last_refresh = std::time::Instant::now();
        }

        popover::show(ui, &self.state);
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

fn main() -> eframe::Result<()> {
    let _log_guard = monitor_rs::logging::init();
    tracing::info!("monitor-rs starting");

    let settings = Settings::load();
    let handle = SamplerHandle::spawn(settings.clone());
    let store = handle.store.clone();
    let visible = Arc::new(AtomicBool::new(true));

    let app = App {
        state: PopoverState { store, settings },
        visible,
        #[cfg(target_os = "macos")]
        tray: None,
        #[cfg(target_os = "macos")]
        last_refresh: std::time::Instant::now(),
        _sampler: handle,
    };

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([300.0, 360.0])
            .with_min_inner_size([280.0, 320.0])
            .with_title("monitor-rs")
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top(),
        ..Default::default()
    };

    eframe::run_native("monitor-rs", opts, Box::new(|_cc| Ok(Box::new(app))))
}
