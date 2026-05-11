import SwiftUI
import MonitorRSC
import MonitorRSLogic

struct PopoverView: View {
    @Bindable var model: MonitorViewModel
    let onQuit: () -> Void

    @Environment(\.accessibilityVoiceOverEnabled) private var voiceoverEnabled
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HeaderStrip(onQuit: onQuit)

            if let latest = model.latest {
                heroSection(latest: latest)

                PillsRow(
                    hero: model.hero,
                    sample: latest,
                    onPin: { kind in
                        withAnimation(swapAnimation) { model.pin(kind) }
                    }
                )

                if model.hero == .cpu {
                    CoreGrid(perCoreUsage: latest.perCoreUsage)
                }

                Divider()

                Text("TOP PROCESSES")
                    .font(.system(.caption, design: .rounded).weight(.medium))
                    .tracking(0.5)
                    .foregroundStyle(.secondary)
                ProcessList(procs: latest.topProcesses)

                Divider()

                FooterStrip(
                    swapUsedBytes: latest.swap_used_bytes,
                    swapTotalBytes: latest.swap_total_bytes,
                    sampleRateHz: 1.0,
                    batteryPresent: latest.battery_present == 1,
                    batteryPct: latest.battery_pct,
                    batteryCharging: latest.battery_charging == 1,
                    cpuTempC: latest.cpu_temp_present == 1 ? latest.cpu_temp_c : nil,
                    gpuTempC: latest.gpu_temp_present == 1 ? latest.gpu_temp_c : nil
                )
            } else {
                VStack {
                    Spacer()
                    Text("Sampling…")
                        .foregroundStyle(.secondary)
                    Spacer()
                }
                .frame(maxWidth: .infinity, minHeight: 200)
            }
        }
        .padding(14)
        .frame(width: 300)
        .dynamicTypeSize(...DynamicTypeSize.xLarge)
        .onAppear { model.voiceoverEnabled = voiceoverEnabled }
        .onChange(of: voiceoverEnabled) { _, new in
            model.voiceoverEnabled = new
        }
    }

    @ViewBuilder
    private func heroSection(latest: MrsSample) -> some View {
        HeroCard(
            kind: model.hero,
            sample: latest,
            history: history(for: model.hero),
            isPinned: model.isHeroPinned,
            onTap: {
                guard model.isHeroPinned else { return }
                withAnimation(swapAnimation) { model.unpinHero() }
            }
        )
        .id(model.hero)
        .transition(.opacity.combined(with: .scale(scale: 0.98)))
    }

    private func history(for kind: MetricKind) -> [Float] {
        switch kind {
        case .cpu:  return model.cpuHistory
        case .gpu:  return model.gpuHistory
        case .mem:  return model.memHistory
        case .net:  return model.netHistory
        case .disk: return model.diskHistory
        }
    }

    private var swapAnimation: Animation? {
        reduceMotion ? nil : .snappy(duration: 0.22)
    }
}
