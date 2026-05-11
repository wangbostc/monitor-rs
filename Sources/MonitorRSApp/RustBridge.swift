import Foundation
import MonitorRSC

/// Safe Swift façade over the Rust FFI. Owns the opaque handle for its
/// lifetime; the deinit calls `monitor_rs_stop` which joins the sampler
/// thread.
final class RustBridge {
    private let handle: OpaquePointer

    init?() {
        guard let h = monitor_rs_start() else { return nil }
        handle = h
    }

    deinit {
        monitor_rs_stop(handle)
    }

    /// Returns the latest sample if one exists.
    func latest() -> MrsSample? {
        var out = MrsSample()
        return monitor_rs_latest(handle, &out) == 1 ? out : nil
    }

    /// Returns up to `n` recent samples (newest last).
    func recent(_ n: Int) -> [MrsSample] {
        guard n > 0 else { return [] }
        var buf = Array<MrsSample>(repeating: MrsSample(), count: n)
        let written = buf.withUnsafeMutableBufferPointer { ptr -> Int in
            Int(monitor_rs_recent(handle, UInt(n), ptr.baseAddress))
        }
        return Array(buf.prefix(written))
    }

    /// Returns the current settings JSON, or `{}` on failure.
    func settingsJSON() -> String {
        guard let cstr = monitor_rs_settings_get(handle) else { return "{}" }
        defer { monitor_rs_string_free(cstr) }
        return String(cString: cstr)
    }

    /// Persists the given settings JSON. Returns true on success.
    @discardableResult
    func setSettingsJSON(_ json: String) -> Bool {
        json.withCString { cstr in
            monitor_rs_settings_set(handle, cstr) == 1
        }
    }
}

/// Helpers for converting MrsSample C-array fields into Swift Arrays.
extension MrsSample {
    var perCoreUsage: [Float] {
        let count = Int(core_count)
        return withUnsafeBytes(of: cpu_per_core_pct) { bytes -> [Float] in
            let ptr = bytes.bindMemory(to: Float.self).baseAddress!
            return Array(UnsafeBufferPointer(start: ptr, count: count))
        }
    }

    var topProcesses: [MrsProcInfo] {
        let count = Int(proc_count)
        return withUnsafeBytes(of: procs) { bytes -> [MrsProcInfo] in
            let ptr = bytes.bindMemory(to: MrsProcInfo.self).baseAddress!
            return Array(UnsafeBufferPointer(start: ptr, count: count))
        }
    }

    var gpuUsage: Float? {
        gpu_present == 1 ? gpu_pct : nil
    }
}

extension MrsProcInfo {
    var nameString: String {
        withUnsafeBytes(of: name) { bytes -> String in
            // bytes is a buffer of CChar; find the NUL terminator (guaranteed
            // because the FFI always NUL-terminates names).
            let ptr = bytes.bindMemory(to: CChar.self).baseAddress!
            return String(cString: ptr)
        }
    }
}
