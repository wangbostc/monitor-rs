use crate::sample::Sample;

/// Substitute `{cpu}`, `{gpu}`, `{mem}`, `{swap}` in the template with
/// integer percentages from the latest sample.
pub fn render_menu_bar(template: &str, s: &Sample) -> String {
    let cpu = s.cpu_total.round() as u32;
    let gpu = s.gpu_pct.map(|p| format!("{}", p.round() as u32)).unwrap_or_else(|| "—".to_string());
    let mem = s.mem.used_pct().round() as u32;
    let swap_total = s.swap.total_bytes.max(1);
    let swap = (s.swap.used_bytes as f64 / swap_total as f64 * 100.0).round() as u32;
    template
        .replace("{cpu}", &format!("{cpu}"))
        .replace("{gpu}", &gpu)
        .replace("{mem}", &format!("{mem}"))
        .replace("{swap}", &format!("{swap}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::{MemInfo, MemPressure, Sample, SwapInfo};
    use std::time::Instant;

    fn s(cpu: f32, gpu: Option<f32>, used: u64, total: u64) -> Sample {
        Sample {
            ts: Instant::now(),
            cpu_total: cpu,
            cpu_per_core: vec![cpu],
            gpu_pct: gpu,
            mem: MemInfo { used_bytes: used, total_bytes: total, pressure: MemPressure::Normal },
            swap: SwapInfo { used_bytes: 0, total_bytes: 0 },
            top_procs: vec![],
        }
    }

    #[test]
    fn substitutes_cpu_gpu_mem() {
        let out = render_menu_bar("C {cpu} G {gpu} M {mem}", &s(42.4, Some(18.2), 64, 100));
        assert_eq!(out, "C 42 G 18 M 64");
    }

    #[test]
    fn gpu_none_renders_dash() {
        let out = render_menu_bar("G {gpu}", &s(0.0, None, 0, 1));
        assert_eq!(out, "G —");
    }
}
