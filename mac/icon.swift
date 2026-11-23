// Draws the app icon: a little keyboard on a pink-to-violet tile, one key a
// heart, with sparkles. `swift mac/icon.swift <out.png>` writes it at 1024px;
// mac/build turns that into the .icns.

import AppKit

let size = 1024.0
let out = CommandLine.arguments.dropFirst().first ?? "icon.png"
let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: Int(size), pixelsHigh: Int(size), bitsPerSample: 8,
    samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
    bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)

func rgb(_ r: Double, _ g: Double, _ b: Double, _ a: Double = 1) -> NSColor {
    NSColor(deviceRed: r / 255, green: g / 255, blue: b / 255, alpha: a)
}

// the tile, on the macOS icon grid, with a soft drop shadow
let tile = NSRect(x: 100, y: 100, width: 824, height: 824)
let tilePath = NSBezierPath(roundedRect: tile, xRadius: 185, yRadius: 185)
NSGraphicsContext.saveGraphicsState()
let shadow = NSShadow()
shadow.shadowColor = rgb(60, 0, 60, 0.35)
shadow.shadowOffset = NSSize(width: 0, height: -12)
shadow.shadowBlurRadius = 28
shadow.set()
rgb(255, 120, 190).setFill()
tilePath.fill()
NSGraphicsContext.restoreGraphicsState()
NSGradient(colors: [rgb(255, 128, 196), rgb(214, 102, 255), rgb(140, 92, 255)])!
    .draw(in: tilePath, angle: -60)

// a soft shine across the top
NSGraphicsContext.saveGraphicsState()
tilePath.addClip()
NSGradient(colors: [rgb(255, 255, 255, 0.28), rgb(255, 255, 255, 0)])!
    .draw(in: NSRect(x: 100, y: 560, width: 824, height: 364), angle: -90)
NSGraphicsContext.restoreGraphicsState()

// the keyboard
let board = NSRect(x: 188, y: 262, width: 648, height: 318)
NSGraphicsContext.saveGraphicsState()
let lift = NSShadow()
lift.shadowColor = rgb(70, 0, 110, 0.35)
lift.shadowOffset = NSSize(width: 0, height: -14)
lift.shadowBlurRadius = 24
lift.set()
rgb(255, 248, 252).setFill()
NSBezierPath(roundedRect: board, xRadius: 64, yRadius: 64).fill()
NSGraphicsContext.restoreGraphicsState()

func key(_ r: NSRect, _ color: NSColor) {
    rgb(232, 210, 240).setFill()
    NSBezierPath(roundedRect: r.offsetBy(dx: 0, dy: -7), xRadius: 18, yRadius: 18).fill()
    color.setFill()
    NSBezierPath(roundedRect: r, xRadius: 18, yRadius: 18).fill()
}

let keyColor = rgb(255, 255, 255)
let pad = 46.0, gap = 16.0, keyH = 64.0
let rowsY = [board.maxY - pad - keyH, board.maxY - pad - 2 * keyH - gap - 4]
let width = board.width - 2 * pad
for (row, y) in rowsY.enumerated() {
    let n = 7 - row
    let inset = Double(row) * 34
    let w = (width - 2 * inset - Double(n - 1) * gap) / Double(n)
    for i in 0..<n {
        let r = NSRect(x: board.minX + pad + inset + Double(i) * (w + gap), y: y, width: w, height: keyH)
        let heart = row == 1 && i == 3
        key(r, heart ? rgb(255, 92, 168) : keyColor)
        if heart {
            // a little heart on the key
            let c = NSPoint(x: r.midX, y: r.midY - 2)
            let h = NSBezierPath()
            h.move(to: NSPoint(x: c.x, y: c.y - 17))
            h.curve(to: NSPoint(x: c.x - 22, y: c.y + 8), controlPoint1: NSPoint(x: c.x - 10, y: c.y - 8), controlPoint2: NSPoint(x: c.x - 22, y: c.y - 3))
            h.curve(to: NSPoint(x: c.x, y: c.y + 10), controlPoint1: NSPoint(x: c.x - 22, y: c.y + 22), controlPoint2: NSPoint(x: c.x - 4, y: c.y + 22))
            h.curve(to: NSPoint(x: c.x + 22, y: c.y + 8), controlPoint1: NSPoint(x: c.x + 4, y: c.y + 22), controlPoint2: NSPoint(x: c.x + 22, y: c.y + 22))
            h.curve(to: NSPoint(x: c.x, y: c.y - 17), controlPoint1: NSPoint(x: c.x + 22, y: c.y - 3), controlPoint2: NSPoint(x: c.x + 10, y: c.y - 8))
            rgb(255, 255, 255).setFill()
            h.fill()
        }
    }
}
// the space bar
let spaceY = rowsY[1] - keyH - gap - 4
key(NSRect(x: board.minX + 200, y: spaceY, width: board.width - 400, height: keyH), keyColor)

// a text cursor blinking above the keyboard
rgb(255, 255, 255).setFill()
NSBezierPath(roundedRect: NSRect(x: 497, y: 650, width: 30, height: 150), xRadius: 15, yRadius: 15).fill()

// sparkles
func sparkle(_ c: NSPoint, _ r: Double, _ color: NSColor) {
    let p = NSBezierPath()
    let k = r * 0.22
    p.move(to: NSPoint(x: c.x, y: c.y + r))
    p.curve(to: NSPoint(x: c.x + r, y: c.y), controlPoint1: NSPoint(x: c.x + k, y: c.y + k), controlPoint2: NSPoint(x: c.x + k, y: c.y + k))
    p.curve(to: NSPoint(x: c.x, y: c.y - r), controlPoint1: NSPoint(x: c.x + k, y: c.y - k), controlPoint2: NSPoint(x: c.x + k, y: c.y - k))
    p.curve(to: NSPoint(x: c.x - r, y: c.y), controlPoint1: NSPoint(x: c.x - k, y: c.y - k), controlPoint2: NSPoint(x: c.x - k, y: c.y - k))
    p.curve(to: NSPoint(x: c.x, y: c.y + r), controlPoint1: NSPoint(x: c.x - k, y: c.y + k), controlPoint2: NSPoint(x: c.x - k, y: c.y + k))
    color.setFill()
    p.fill()
}
sparkle(NSPoint(x: 330, y: 760), 62, rgb(255, 255, 255))
sparkle(NSPoint(x: 700, y: 800), 40, rgb(255, 236, 140))
sparkle(NSPoint(x: 760, y: 690), 24, rgb(255, 255, 255, 0.9))
sparkle(NSPoint(x: 245, y: 655), 20, rgb(255, 236, 140, 0.9))

NSGraphicsContext.current = nil
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
