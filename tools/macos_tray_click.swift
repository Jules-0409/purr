// SPDX-License-Identifier: GPL-3.0-only
// Send a real status-item click without hard-coding display coordinates.
import ApplicationServices
import Foundation

guard (3...4).contains(CommandLine.arguments.count),
      let pid = Int32(CommandLine.arguments[1]),
      ["left", "right"].contains(CommandLine.arguments[2]),
      AXIsProcessTrusted() else {
    fputs("Usage: macos_tray_click PID left|right (requires Accessibility permission)\n", stderr)
    exit(1)
}
let app = AXUIElementCreateApplication(pid)
var menu: CFTypeRef?
for _ in 0..<60 {
    if AXUIElementCopyAttributeValue(app, "AXExtrasMenuBar" as CFString, &menu) == .success, menu != nil { break }
    Thread.sleep(forTimeInterval: 0.05)
}
guard let menu else { exit(2) }
var children: CFTypeRef?
AXUIElementCopyAttributeValue(menu as! AXUIElement, kAXChildrenAttribute as CFString, &children)
guard let item = (children as? [AXUIElement])?.first else { exit(3) }
var position: CFTypeRef?
var size: CFTypeRef?
AXUIElementCopyAttributeValue(item, kAXPositionAttribute as CFString, &position)
AXUIElementCopyAttributeValue(item, kAXSizeAttribute as CFString, &size)
guard let position, let size else { exit(4) }
var point = CGPoint.zero
var bounds = CGSize.zero
AXValueGetValue(position as! AXValue, .cgPoint, &point)
AXValueGetValue(size as! AXValue, .cgSize, &bounds)
point.x += bounds.width / 2
point.y += bounds.height / 2
let right = CommandLine.arguments[2] == "right"
let button: CGMouseButton = right ? .right : .left
CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: point, mouseButton: button)?.post(tap: .cghidEventTap)
Thread.sleep(forTimeInterval: 0.05)
CGEvent(mouseEventSource: nil, mouseType: right ? .rightMouseDown : .leftMouseDown, mouseCursorPosition: point, mouseButton: button)?.post(tap: .cghidEventTap)
Thread.sleep(forTimeInterval: CommandLine.arguments.count == 4 ? (Double(CommandLine.arguments[3]) ?? 0.05) : 0.05)
CGEvent(mouseEventSource: nil, mouseType: right ? .rightMouseUp : .leftMouseUp, mouseCursorPosition: point, mouseButton: button)?.post(tap: .cghidEventTap)
print("\(CommandLine.arguments[2]) click at \(point.x),\(point.y)")
