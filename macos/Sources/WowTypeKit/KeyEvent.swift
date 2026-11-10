// 会话要的按键（WowtypeKeyEvent）在 Swift 这边的样子。
//
// 这里只有名字，没有数值：WOWTYPE_KEY_* / WOWTYPE_MOD_* 的数值只在 core/include/wowtype_core.h 里，
// WowTypeEngine 换成 C 结构时逐个对到头文件里的常量。

/// 具名键。打出字符的键（字母、数字、标点、空格）是 `.none`，字符放在 `unicode` 里。
public enum WowKey: Sendable, Hashable, CaseIterable {
    case none
    case backspace
    case returnKey   // 小键盘回车：returnKey + .keypad
    case escape
    case tab
    case delete      // 向前删除
    case left, right, up, down
    case home, end, pageUp, pageDown
    case shiftL, shiftR
    case controlL, controlR
    case altL, altR       // Option
    case superL, superR   // Command
    case capsLock
    case other            // 别的功能键（F1……）
}

/// 按键时按着的修饰键。
public struct WowModifiers: OptionSet, Sendable, Hashable {
    public let rawValue: UInt8
    public init(rawValue: UInt8) { self.rawValue = rawValue }

    public static let shift = WowModifiers(rawValue: 1 << 0)
    public static let control = WowModifiers(rawValue: 1 << 1)
    public static let alt = WowModifiers(rawValue: 1 << 2)      // Option
    public static let `super` = WowModifiers(rawValue: 1 << 3)  // Command
    public static let keypad = WowModifiers(rawValue: 1 << 4)   // 小键盘上的键
    public static let capsLock = WowModifiers(rawValue: 1 << 5)
}

/// 交给会话的一个按键。
public struct WowKeyEvent: Sendable, Hashable {
    public var key: WowKey
    /// 这个键在当前修饰键下打出的字符，没有填 0。
    public var unicode: UInt32
    public var modifiers: WowModifiers
    /// true 松开，false 按下（含自动重复）。
    public var isRelease: Bool
    /// 本地时区和 UTC 的差（秒），日期时间候选用。
    public var utcOffsetSeconds: Int32
    /// 按键时刻，Unix 毫秒。
    public var timeMs: Int64

    public init(key: WowKey, unicode: UInt32, modifiers: WowModifiers, isRelease: Bool,
                utcOffsetSeconds: Int32, timeMs: Int64) {
        self.key = key
        self.unicode = unicode
        self.modifiers = modifiers
        self.isRelease = isRelease
        self.utcOffsetSeconds = utcOffsetSeconds
        self.timeMs = timeMs
    }
}

/// 从 NSEvent 里取出的几个字段（WowTypeKit 不引 AppKit）。
public struct MacKeyInput: Sendable, Hashable {
    public enum Kind: Sendable, Hashable { case keyDown, keyUp, flagsChanged }

    public var kind: Kind
    public var keyCode: UInt16
    public var characters: String
    public var charactersIgnoringModifiers: String
    /// NSEvent.modifierFlags.rawValue，含左右设备位（NX_DEVICE*）。
    public var modifierFlags: UInt64

    public init(kind: Kind, keyCode: UInt16, characters: String, charactersIgnoringModifiers: String,
                modifierFlags: UInt64) {
        self.kind = kind
        self.keyCode = keyCode
        self.characters = characters
        self.charactersIgnoringModifiers = charactersIgnoringModifiers
        self.modifierFlags = modifierFlags
    }
}
