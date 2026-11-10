// WowKeyEvent -> C 的 WowtypeKeyEvent。数值只从头文件里的常量来，这里不写数字。
import CWowTypeCore
import WowTypeKit

extension WowKey {
    var cValue: UInt32 {
        let v: Int32
        switch self {
        case .none: v = Int32(WOWTYPE_KEY_NONE)
        case .backspace: v = Int32(WOWTYPE_KEY_BACKSPACE)
        case .returnKey: v = Int32(WOWTYPE_KEY_RETURN)
        case .escape: v = Int32(WOWTYPE_KEY_ESCAPE)
        case .tab: v = Int32(WOWTYPE_KEY_TAB)
        case .delete: v = Int32(WOWTYPE_KEY_DELETE)
        case .left: v = Int32(WOWTYPE_KEY_LEFT)
        case .right: v = Int32(WOWTYPE_KEY_RIGHT)
        case .up: v = Int32(WOWTYPE_KEY_UP)
        case .down: v = Int32(WOWTYPE_KEY_DOWN)
        case .home: v = Int32(WOWTYPE_KEY_HOME)
        case .end: v = Int32(WOWTYPE_KEY_END)
        case .pageUp: v = Int32(WOWTYPE_KEY_PAGE_UP)
        case .pageDown: v = Int32(WOWTYPE_KEY_PAGE_DOWN)
        case .shiftL: v = Int32(WOWTYPE_KEY_SHIFT_L)
        case .shiftR: v = Int32(WOWTYPE_KEY_SHIFT_R)
        case .controlL: v = Int32(WOWTYPE_KEY_CONTROL_L)
        case .controlR: v = Int32(WOWTYPE_KEY_CONTROL_R)
        case .altL: v = Int32(WOWTYPE_KEY_ALT_L)
        case .altR: v = Int32(WOWTYPE_KEY_ALT_R)
        case .superL: v = Int32(WOWTYPE_KEY_SUPER_L)
        case .superR: v = Int32(WOWTYPE_KEY_SUPER_R)
        case .capsLock: v = Int32(WOWTYPE_KEY_CAPS_LOCK)
        case .other: v = Int32(WOWTYPE_KEY_OTHER)
        }
        return UInt32(v)
    }
}

extension WowModifiers {
    var cValue: UInt32 {
        var m: Int32 = 0
        if contains(.shift) { m |= Int32(WOWTYPE_MOD_SHIFT) }
        if contains(.control) { m |= Int32(WOWTYPE_MOD_CONTROL) }
        if contains(.alt) { m |= Int32(WOWTYPE_MOD_ALT) }
        if contains(.super) { m |= Int32(WOWTYPE_MOD_SUPER) }
        if contains(.keypad) { m |= Int32(WOWTYPE_MOD_KEYPAD) }
        if contains(.capsLock) { m |= Int32(WOWTYPE_MOD_CAPS_LOCK) }
        return UInt32(m)
    }
}

extension WowKeyEvent {
    var cValue: WowtypeKeyEvent {
        WowtypeKeyEvent(
            struct_size: UInt32(MemoryLayout<WowtypeKeyEvent>.size),
            key: key.cValue,
            unicode: unicode,
            modifiers: modifiers.cValue,
            is_release: isRelease ? 1 : 0,
            utc_offset_seconds: utcOffsetSeconds,
            time_ms: timeMs)
    }
}
