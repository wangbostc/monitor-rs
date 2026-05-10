use egui::Ui;

use crate::sample::ProcInfo;

pub fn process_list(ui: &mut Ui, procs: &[ProcInfo]) {
    egui::Grid::new("monitor-rs-procs")
        .num_columns(3)
        .spacing([12.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            for p in procs {
                let name = truncate(&p.name, 22);
                ui.label(name);
                ui.label(format!("{:>4.0}%", p.cpu_pct));
                ui.label(format_bytes(p.rss_bytes));
                ui.end_row();
            }
        });
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else {
        let mut t: String = s.chars().take(max - 1).collect();
        t.push('…');
        t
    }
}

fn format_bytes(b: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = b as f64;
    if b >= GB { format!("{:.1}G", b / GB) }
    else if b >= MB { format!("{:.0}M", b / MB) }
    else { format!("{:.0}K", (b / KB).max(0.0)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string() {
        assert_eq!(truncate("abcdefghij", 5), "abcd…");
    }

    #[test]
    fn format_bytes_units() {
        assert!(format_bytes(512).ends_with("K"));
        assert!(format_bytes(2 * 1024 * 1024).ends_with("M"));
        assert!(format_bytes(3 * 1024 * 1024 * 1024).ends_with("G"));
    }
}
