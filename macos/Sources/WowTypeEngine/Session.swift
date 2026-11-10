// 输入会话的 Swift 封装：一个输入控制器一个会话，只在主线程上用。
// 行为全在核心的 session.rs 里，这里只转调用、把输出拷成 Swift 值。
import CWowTypeCore
import WowTypeKit

/// 会话这一刻的样子（编辑区、候选、模式），拷出来的值，不引用核心里的内存。
public struct SessionSnapshot: Sendable, Equatable {
    public struct Candidate: Sendable, Equatable {
        public var text: String
        public var label: String
    }

    public var composing: Bool
    public var preedit: MarkedTextLayout
    public var aux: String
    public var candidates: [Candidate]
    public var cursor: Int
    /// 0 表示列表，大于 0 表示展开的网格每行几个。
    public var columns: Int
    public var vertical: Bool
    public var hasPrevPage: Bool
    public var hasNextPage: Bool
    public var asciiMode: Bool

    public static let empty = SessionSnapshot(
        composing: false, preedit: MarkedTextLayout(text: "", plainBytes: 0, cursorBytes: 0), aux: "",
        candidates: [], cursor: 0, columns: 0, vertical: false, hasPrevPage: false, hasNextPage: false,
        asciiMode: false)
}

@MainActor
public final class Session {
    // deinit 不在主线程隔离里（isolated deinit 要 macOS 15.4），所以标成 nonisolated(unsafe)；
    // 会话只被这个对象持有，deinit 时没有别的地方在用它。
    nonisolated(unsafe) private let raw: OpaquePointer

    public init() {
        guard let s = wowtype_session_new() else { fatalError("wowtype_session_new 返回空") }
        raw = s
    }

    deinit { wowtype_session_free(raw) }

    /// 读入设置全文。返回认不出的行数，出错 -1。
    @discardableResult
    public func configure(_ ini: String) -> Int {
        Int(wowtype_session_configure(raw, ini))
    }

    /// 返回 true 表示会话用掉了这个键。
    public func process(_ event: WowKeyEvent, engine: Engine?) -> Bool {
        var c = event.cValue
        return wowtype_session_process_key(raw, engine?.raw, &c) != 0
    }

    public func refresh(engine: Engine?) {
        wowtype_session_refresh(raw, engine?.raw)
    }

    @discardableResult
    public func select(_ index: Int, engine: Engine?) -> Bool {
        guard index >= 0 else { return false }
        return wowtype_session_select(raw, engine?.raw, index) != 0
    }

    @discardableResult
    public func page(_ delta: Int) -> Bool {
        wowtype_session_page(raw, Int32(clamping: delta)) != 0
    }

    public func reset() { wowtype_session_reset(raw) }

    public func commitComposition(engine: Engine?) {
        wowtype_session_commit_composition(raw, engine?.raw)
    }

    /// 要上屏的文字，取一次就清空。
    public func takeCommit() -> String? {
        wowtype_session_take_commit(raw).map { String(cString: $0) }
    }

    public func takeSaveRequest() -> Bool {
        wowtype_session_take_save_request(raw) != 0
    }

    public var isComposing: Bool { wowtype_session_is_composing(raw) != 0 }

    public func snapshot() -> SessionSnapshot {
        let text = string(wowtype_session_preedit(raw))
        let layout = MarkedTextLayout(text: text,
                                      plainBytes: Int(wowtype_session_preedit_plain_len(raw)),
                                      cursorBytes: Int(wowtype_session_preedit_cursor(raw)))
        let count = Int(wowtype_session_candidate_count(raw))
        var items: [SessionSnapshot.Candidate] = []
        items.reserveCapacity(count)
        for i in 0..<count {
            items.append(.init(text: string(wowtype_session_candidate_text(raw, i)),
                               label: string(wowtype_session_candidate_label(raw, i))))
        }
        return SessionSnapshot(
            composing: isComposing,
            preedit: layout,
            aux: string(wowtype_session_aux(raw)),
            candidates: items,
            cursor: Int(wowtype_session_candidate_cursor(raw)),
            columns: Int(wowtype_session_columns(raw)),
            vertical: wowtype_session_vertical(raw) != 0,
            hasPrevPage: wowtype_session_has_prev_page(raw) != 0,
            hasNextPage: wowtype_session_has_next_page(raw) != 0,
            asciiMode: wowtype_session_mode(raw) == Int32(WOWTYPE_MODE_ASCII))
    }

    private func string(_ p: UnsafePointer<CChar>?) -> String {
        p.map { String(cString: $0) } ?? ""
    }
}
