import CoreGraphics
import CoreText
import Foundation

let args = CommandLine.arguments
guard args.count == 3 else {
    fputs("usage: generate-alien-block-coral.swift <font.ttf> <output.txt>\n", stderr)
    exit(2)
}

let fontURL = URL(fileURLWithPath: args[1])
let outputURL = URL(fileURLWithPath: args[2])
let env = ProcessInfo.processInfo.environment
let flipTextContext = env["CORAL_ALIEN_FLIP_CONTEXT"] != "0"
let topDownOutput = env["CORAL_ALIEN_TOP_DOWN"] != "0"
// Glyphs should retain their normal left-to-right orientation. Set this to
// 1 only when deliberately generating a horizontally mirrored variant.
let mirrorGlyphs = env["CORAL_ALIEN_MIRROR_GLYPHS"] == "1"
let fontData = try Data(contentsOf: fontURL) as CFData
guard
    let provider = CGDataProvider(data: fontData),
    let graphicsFont = CGFont(provider)
else {
    fputs("failed to load font\n", stderr)
    exit(1)
}

let fontSize: CGFloat = 96
let font = CTFontCreateWithGraphicsFont(graphicsFont, fontSize, nil, nil)
let attributes: CFDictionary = [
    kCTFontAttributeName: font,
    kCTForegroundColorAttributeName: CGColor(gray: 1, alpha: 1),
] as CFDictionary
let word = "coral"
let letters = Array(word).map(String.init)
let letterSpacing: CGFloat = 18
let letterLines = letters.map { letter -> CTLine in
    let attributed = CFAttributedStringCreate(nil, letter as CFString, attributes)!
    return CTLineCreateWithAttributedString(attributed)
}

var ascent: CGFloat = 0
var descent: CGFloat = 0
var leading: CGFloat = 0
var typographicWidth: CGFloat = 0
for (index, line) in letterLines.enumerated() {
    var lineAscent: CGFloat = 0
    var lineDescent: CGFloat = 0
    var lineLeading: CGFloat = 0
    typographicWidth += CTLineGetTypographicBounds(line, &lineAscent, &lineDescent, &lineLeading)
    ascent = max(ascent, lineAscent)
    descent = max(descent, lineDescent)
    leading = max(leading, lineLeading)
    if index + 1 < letterLines.count {
        typographicWidth += letterSpacing
    }
}
let padding = 16
let width = Int(ceil(typographicWidth)) + padding * 2
let height = Int(ceil(ascent + descent + leading)) + padding * 2

var pixels = [UInt8](repeating: 0, count: width * height)
let colorSpace = CGColorSpaceCreateDeviceGray()
guard
    let context = CGContext(
        data: &pixels,
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: width,
        space: colorSpace,
        bitmapInfo: CGImageAlphaInfo.none.rawValue
    )
else {
    fputs("failed to create bitmap context\n", stderr)
    exit(1)
}

context.setFillColor(CGColor(gray: 0, alpha: 1))
context.fill(CGRect(x: 0, y: 0, width: width, height: height))
context.textMatrix = .identity
if flipTextContext {
    context.translateBy(x: 0, y: CGFloat(height))
    context.scaleBy(x: 1, y: -1)
}
let baselineY = CGFloat(padding) + descent
var cursorX = CGFloat(padding)
var letterBounds: [(start: Int, end: Int)] = []
for (index, line) in letterLines.enumerated() {
    context.textPosition = CGPoint(x: cursorX, y: baselineY)
    CTLineDraw(line, context)
    let advance = CTLineGetTypographicBounds(line, nil, nil, nil)
    letterBounds.append((start: Int(floor(cursorX)), end: Int(ceil(cursorX + advance))))
    cursorX += advance + (index + 1 < letterLines.count ? letterSpacing : 0)
}

let threshold: UInt8 = 64
var minX = width
var maxX = 0
var minY = height
var maxY = 0

for y in 0..<height {
    for x in 0..<width {
        if pixels[y * width + x] > threshold {
            minX = min(minX, x)
            maxX = max(maxX, x)
            minY = min(minY, y)
            maxY = max(maxY, y)
        }
    }
}

guard minX <= maxX, minY <= maxY else {
    fputs("font rendered no visible pixels\n", stderr)
    exit(1)
}

let targetWidth = 84
let cropWidth = maxX - minX + 1
let cropHeight = maxY - minY + 1
let scale = max(1, Int(ceil(Double(cropWidth) / Double(targetWidth))))
let outWidth = Int(ceil(Double(cropWidth) / Double(scale)))
let outHeight = Int(ceil(Double(cropHeight) / Double(scale * 2)))

func sample(x: Int, y: Int) -> Bool {
    var startX = minX + x * scale
    let logicalStartY = y * scale
    if startX > maxX || logicalStartY >= cropHeight {
        return false
    }
    var endX = min(maxX, startX + scale - 1)
    if mirrorGlyphs {
        let midpoint = (startX + endX) / 2
        if let bound = letterBounds.first(where: { midpoint >= $0.start && midpoint <= $0.end }) {
            let mirroredStart = bound.start + bound.end - endX
            let mirroredEnd = bound.start + bound.end - startX
            startX = max(minX, mirroredStart)
            endX = min(maxX, mirroredEnd)
        }
    }
    let logicalEndY = min(cropHeight - 1, logicalStartY + scale - 1)
    let startY: Int
    let endY: Int
    if topDownOutput {
        // Bitmap rows are bottom-up; map logical top-down rows back into the
        // source before converting them to half-blocks.
        startY = minY + cropHeight - 1 - logicalEndY
        endY = minY + cropHeight - 1 - logicalStartY
    } else {
        startY = minY + logicalStartY
        endY = minY + logicalEndY
    }
    var lit = 0
    var total = 0

    for py in startY...endY {
        for px in startX...endX {
            total += 1
            if pixels[py * width + px] > threshold {
                lit += 1
            }
        }
    }

    return lit * 3 >= max(1, total)
}

var lines: [String] = []
for row in 0..<outHeight {
    var lineText = ""
    for col in 0..<outWidth {
        let top = sample(x: col, y: row * 2)
        let bottom = sample(x: col, y: row * 2 + 1)

        if top && bottom {
            lineText.append("█")
        } else if top {
            lineText.append("▀")
        } else if bottom {
            lineText.append("▄")
        } else {
            lineText.append(" ")
        }
    }
    while lineText.last == " " {
        lineText.removeLast()
    }
    lines.append(lineText)
}

let output = lines.joined(separator: "\n") + "\n"
try output.write(to: outputURL, atomically: true, encoding: .utf8)
print("Wrote \(outputURL.path)")
