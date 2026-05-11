// Resources/icon/gen_icon.swift
// Single-file Swift script. Usage: swift gen_icon.swift <out.png>
// Renders a 1024x1024 placeholder app icon: SF Symbol on a tinted rounded square.
import SwiftUI
import AppKit

let size: CGFloat = 1024
let symbolName = "gauge.with.dots.needle.50percent"

guard CommandLine.arguments.count == 2 else {
    FileHandle.standardError.write("usage: gen_icon.swift <out.png>\n".data(using: .utf8)!)
    exit(2)
}
let outURL = URL(fileURLWithPath: CommandLine.arguments[1])

let view = ZStack {
    RoundedRectangle(cornerRadius: size * 0.22, style: .continuous)
        .fill(LinearGradient(
            colors: [Color(red: 0.10, green: 0.55, blue: 0.95),
                     Color(red: 0.05, green: 0.30, blue: 0.75)],
            startPoint: .topLeading,
            endPoint: .bottomTrailing))
    Image(systemName: symbolName)
        .font(.system(size: size * 0.55, weight: .regular))
        .foregroundStyle(.white)
}
.frame(width: size, height: size)

// ImageRenderer is @MainActor-isolated. Top-level script code runs synchronously
// on the main thread, so assumeIsolated is safe here.
let pngData: Data? = MainActor.assumeIsolated {
    let renderer = ImageRenderer(content: view)
    renderer.scale = 1
    guard let nsImage = renderer.nsImage,
          let tiff = nsImage.tiffRepresentation,
          let rep  = NSBitmapImageRep(data: tiff),
          let png  = rep.representation(using: .png, properties: [:])
    else { return nil }
    return png
}

guard let png = pngData else {
    FileHandle.standardError.write("icon render failed\n".data(using: .utf8)!)
    exit(1)
}

try png.write(to: outURL)
print("wrote \(outURL.path) (\(png.count) bytes)")
