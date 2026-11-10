//! 输入会话：按键 → 编辑区 / 候选 / 上屏 / 学习，和平台无关，各平台前端共用一份。
//!
//! 行为照 fcitx5 插件（fcitx5-addon/src/wowtype_engine.cpp 的 keyEvent 及其调用的函数），
//! 候选翻页和光标照 fcitx5 的 CommonCandidateList（插件用的设置：光标不限在本页、
//! 翻页后光标回到这页第一个）。按键测试在 tests/session_keys.rs，逐条对应插件的 key_harness.cpp。
//! 这一版不含互译、语音、表情面板。
//!
//! 引擎每次调用时传入，可为 None（词库还在加载）：字母照样进编辑区，只是没有候选。

use crate::config::{self, Config, Layout, SelectKeys, ShiftToggle};
use crate::datetime::LocalTime;
use crate::Engine;

/// 按键时按着的修饰键（位掩码，和 C ABI 的 WOWTYPE_MOD_* 一致）。
pub mod mods {
    pub const SHIFT: u32 = 1;
    pub const CONTROL: u32 = 2;
    /// macOS 的 Option。
    pub const ALT: u32 = 4;
    /// macOS 的 Command。
    pub const SUPER: u32 = 8;
    /// 小键盘上的键。
    pub const KEYPAD: u32 = 16;
    pub const CAPS_LOCK: u32 = 32;
}

/// 具名键。打出字符的键（字母、数字、标点、空格）用 `Key::None`，字符放在 `KeyEvent::unicode`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    None,
    Backspace,
    /// 小键盘回车是 Return + `mods::KEYPAD`。
    Return,
    Escape,
    Tab,
    /// 向前删除。
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    /// 单独的修饰键按下/松开。
    ShiftL,
    ShiftR,
    ControlL,
    ControlR,
    AltL,
    AltR,
    SuperL,
    SuperR,
    CapsLock,
    /// 别的功能键（F1……）：编辑中吞掉，没在编辑时交给应用。
    Other,
}

/// 键和 C ABI 的 WOWTYPE_KEY_* 数值。
const KEY_CODES: &[(Key, u32)] = &[
    (Key::None, 0),
    (Key::Backspace, 1),
    (Key::Return, 2),
    (Key::Escape, 3),
    (Key::Tab, 4),
    (Key::Delete, 5),
    (Key::Left, 6),
    (Key::Right, 7),
    (Key::Up, 8),
    (Key::Down, 9),
    (Key::Home, 10),
    (Key::End, 11),
    (Key::PageUp, 12),
    (Key::PageDown, 13),
    (Key::ShiftL, 20),
    (Key::ShiftR, 21),
    (Key::ControlL, 22),
    (Key::ControlR, 23),
    (Key::AltL, 24),
    (Key::AltR, 25),
    (Key::SuperL, 26),
    (Key::SuperR, 27),
    (Key::CapsLock, 28),
    (Key::Other, 99),
];

impl Key {
    /// C ABI 里的数值（WOWTYPE_KEY_*）。
    pub fn code(self) -> u32 {
        KEY_CODES
            .iter()
            .find(|(k, _)| *k == self)
            .map(|(_, c)| *c)
            .unwrap_or(0)
    }

    /// 由 C ABI 的数值得到键，不认识的返回 None。
    pub fn from_code(code: u32) -> Option<Key> {
        KEY_CODES.iter().find(|(_, c)| *c == code).map(|(k, _)| *k)
    }
}

/// 一个按键事件（按下或松开）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    /// 这个键在当前修饰键下打出的字符（Shift+a 是 'A'），没有时 None。
    pub unicode: Option<char>,
    /// `mods::*` 的组合。
    pub modifiers: u32,
    pub is_release: bool,
    /// 本地时区和 UTC 的差（秒），日期时间候选用。
    pub utc_offset_seconds: i32,
    /// 按键时刻（Unix 毫秒），单按 Shift 的计时和日期时间候选用。
    pub time_ms: i64,
}

/// 中文还是英文（英文时按键直接交给应用）。数值和 C ABI 的 WOWTYPE_MODE_* 一致。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Chinese = 0,
    Ascii = 1,
}

/// 记给引擎当上文的最近上屏文字（字节）。
const CONTEXT_BYTES: usize = 24;
/// 学习多少次后请前端存盘。
const SAVE_EVERY: u32 = 20;
/// 打字推荐的 emoji 插在第几个（从 0 数）。
const EMOJI_SLOT: usize = 2;
/// 展开的网格每行几个、一屏几行。
const GRID_COLS: usize = 6;
const GRID_ROWS: usize = 6;
/// 按住 Shift 超过这么久就不算单按（毫秒）。
const SHIFT_TAP_MS: i64 = 500;
/// 打字途中按标点时，最多连着选几次第一个候选把整句上屏。
const PUNCT_COMMIT_GUARD: usize = 16;
const LABELS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];

#[derive(Clone, Debug)]
struct Cand {
    text: String,
    /// 显示用：换行画成 ⏎、制表符画成 ⇥。
    display: String,
    pinyin: String,
    /// 用掉了剩余输入的前多少字节。
    consumed: usize,
    /// 不学习（计算结果、日期时间、带换行的自定义词）。
    plain: bool,
    /// emoji 候选选中后上屏的文字（句尾推荐是整句 + emoji）。
    emoji: Option<String>,
}

impl Cand {
    fn new(text: String, pinyin: String, consumed: usize) -> Self {
        Self {
            display: shown_text(&text),
            text,
            pinyin,
            consumed,
            plain: false,
            emoji: None,
        }
    }

    fn plain(text: String, consumed: usize) -> Self {
        Self {
            plain: true,
            ..Self::new(text, String::new(), consumed)
        }
    }

    fn emoji(text: String, commit: String, consumed: usize) -> Self {
        Self {
            emoji: Some(commit),
            ..Self::new(text, String::new(), consumed)
        }
    }
}

/// 已选的句首词。
#[derive(Clone, Debug)]
struct Selection {
    text: String,
    pinyin: String,
    /// 它用掉的输入字节数。
    bytes: usize,
}

/// 候选列表的翻页和光标，照 fcitx5 的 CommonCandidateList。
#[derive(Clone, Copy, Debug, Default)]
struct List {
    len: usize,
    page_size: usize,
    page: usize,
    /// 全局光标位置；可以不在当前页。
    cursor: usize,
}

impl List {
    fn total_pages(&self) -> usize {
        ceil_div(self.len, self.page_size)
    }

    fn page_start(&self) -> usize {
        self.page * self.page_size
    }

    fn page_len(&self) -> usize {
        self.len
            .saturating_sub(self.page_start())
            .min(self.page_size)
    }

    /// 光标在这页的第几个；不在这页时 None。
    fn cursor_on_page(&self) -> Option<usize> {
        (self.len > 0 && self.cursor / self.page_size == self.page)
            .then(|| self.cursor % self.page_size)
    }

    fn has_prev(&self) -> bool {
        self.page > 0
    }

    fn has_next(&self) -> bool {
        self.page + 1 < self.total_pages()
    }

    /// 翻页：光标原来在这页时回到新页的第一个，不在时不动。
    fn set_page(&mut self, page: usize) {
        if page != self.page && page < self.total_pages() {
            let was_here = self.cursor_on_page().is_some();
            self.page = page;
            if was_here {
                self.cursor = self.page_start();
            }
        }
    }

    fn prev(&mut self) {
        if self.has_prev() {
            self.set_page(self.page - 1);
        }
    }

    fn next(&mut self) {
        if self.has_next() {
            self.set_page(self.page + 1);
        }
    }

    /// 上一个/下一个候选：到头绕到另一头，并翻到光标所在的页。
    fn move_cursor(&mut self, prev: bool) {
        if self.len == 0 {
            return;
        }
        if self.cursor_on_page().is_none() {
            self.cursor = self.page_start() + if prev { self.page_len() - 1 } else { 0 };
            return;
        }
        self.cursor = match (prev, self.cursor) {
            (true, 0) => self.len - 1,
            (true, c) => c - 1,
            (false, c) if c + 1 >= self.len => 0,
            (false, c) => c + 1,
        };
        self.set_page(self.cursor / self.page_size);
    }
}

/// 一个输入框的输入状态。
pub struct Session {
    config: Config,
    mode: Mode,
    /// 打的全部字母（含已选句首词用掉的部分）。
    buffer: String,
    selections: Vec<Selection>,
    candidates: Vec<Cand>,
    /// 按 Down 展开成网格。
    expanded: bool,
    grid_cursor: usize,
    /// 重建列表时的光标（网格收起时带回来）。
    panel_cursor: usize,
    list: List,
    /// 最近上屏的文字，给引擎当上文。
    recent: String,
    /// 还没交给前端的上屏文字。
    commit: String,
    preedit: String,
    plain_len: usize,
    unsaved: u32,
    save_requested: bool,
    shift_down: Option<Key>,
    shift_down_at: i64,
    /// 最近一次按键的时刻和时区，日期时间候选用。
    time_ms: i64,
    utc_offset_seconds: i32,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// 按键里会话关心的部分。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sym {
    Named(Key),
    /// 主键盘区打出的字符（含空格）。
    Char(char),
    /// 小键盘打出的字符。
    Keypad(char),
}

impl Sym {
    fn of(ev: &KeyEvent) -> Self {
        match (ev.key, ev.unicode) {
            (Key::None, Some(c)) if ev.modifiers & mods::KEYPAD != 0 => Sym::Keypad(c),
            (Key::None, Some(c)) => Sym::Char(c),
            (key, _) => Sym::Named(key),
        }
    }

    fn char(self) -> Option<char> {
        match self {
            Sym::Char(c) => Some(c),
            _ => None,
        }
    }

    /// 数字键（主键盘或小键盘，和 fcitx 的 Key::isDigit 一样）的数字。
    fn digit(self) -> Option<usize> {
        match self {
            Sym::Char(c) | Sym::Keypad(c) => c.to_digit(10).map(|d| d as usize),
            Sym::Named(_) => None,
        }
    }
}

impl Session {
    pub fn new() -> Self {
        Self {
            config: Config::default(),
            mode: Mode::Chinese,
            buffer: String::new(),
            selections: Vec::new(),
            candidates: Vec::new(),
            expanded: false,
            grid_cursor: 0,
            panel_cursor: 0,
            list: List::default(),
            recent: String::new(),
            commit: String::new(),
            preedit: String::new(),
            plain_len: 0,
            unsaved: 0,
            save_requested: false,
            shift_down: None,
            shift_down_at: 0,
            time_ms: 0,
            utc_offset_seconds: 0,
        }
    }

    /// 读入设置文本，只用和会话有关的键（候选排列、每页几个、选择键、单按 Shift、日期时间、
    /// 计算器、emoji 推荐、双拼）。不清空编辑区，新设置在下一次算候选时生效。返回认不出的行数。
    pub fn configure(&mut self, ini: &str) -> usize {
        let (config, bad) = config::parse(ini);
        self.config = config;
        if self.config.shift_toggle == ShiftToggle::Off {
            self.mode = Mode::Chinese;
        }
        bad
    }

    /// 处理一个按键。返回 true 表示会话用掉了这个键（前端不要再交给应用）。
    pub fn process_key(&mut self, engine: Option<&mut Engine>, ev: &KeyEvent) -> bool {
        let engine = engine.map(|e| &*e);
        self.time_ms = ev.time_ms;
        self.utc_offset_seconds = ev.utc_offset_seconds;
        if self.shift_toggle(engine, ev) {
            return true;
        }
        if ev.is_release {
            return false;
        }
        if self.mode == Mode::Ascii && !self.composing() {
            return false;
        }
        // 带 Ctrl/Alt/Super 的组合键交给应用，编辑区保留
        if ev.modifiers & (mods::CONTROL | mods::ALT | mods::SUPER) != 0 {
            return false;
        }
        let sym = Sym::of(ev);
        if self.composing() {
            if let Some(handled) = self.composing_key(engine, sym, ev.modifiers) {
                return handled;
            }
        } else if let Some(punct) = sym.char().and_then(crate::symbols::fullwidth_punct) {
            // 没在打拼音时 ASCII 标点转成中文标点；没收录的（/ " + 等）交给应用
            self.commit_plain(engine, punct);
            return true;
        }
        match sym.char() {
            // 大写字母原样保留：Hello、iPhone 这类英文候选要用到
            Some(c) if c.is_ascii_alphabetic() => self.push(engine, c),
            // 隔音符（xi'an）；开头的隔音符没有意义
            Some('\'') if self.composing() => self.push(engine, '\''),
            _ => return false,
        }
        true
    }

    /// 正在编辑时的按键。返回 None 表示接着按字母规则处理。
    fn composing_key(&mut self, engine: Option<&Engine>, sym: Sym, modifiers: u32) -> Option<bool> {
        if self.expanded && self.grid_key(engine, sym) {
            return Some(true);
        }
        match sym {
            Sym::Named(Key::Backspace) => {
                if self.remaining().is_empty() && !self.selections.is_empty() {
                    // 撤销最后一次选词，拼音回到编辑区（已经记下的那次选词不撤销）
                    self.selections.pop();
                } else {
                    self.buffer.pop();
                }
                if self.buffer.is_empty() {
                    self.clear();
                }
                self.refresh_candidates(engine);
                return Some(true);
            }
            Sym::Named(Key::Escape) => {
                self.clear();
                return Some(true);
            }
            Sym::Char(' ') => {
                self.commit_current(engine);
                return Some(true);
            }
            Sym::Named(Key::Return) => {
                // 回车：已选的词 + 原始字母上屏
                let tail = self.remaining();
                self.commit_all(engine, &tail, "");
                return Some(true);
            }
            _ => {}
        }
        if self.config.calculator && !self.config.shuangpin {
            // V 模式：v 后面的数字、运算符进编辑区（不选词、不翻页、不当标点）
            if sym == Sym::Char('=') && self.calc_active() && self.buffer.len() > 1 {
                self.commit_current(engine);
                return Some(true);
            }
            if let Some(c) = calc_char(sym).filter(|_| self.calc_active()) {
                self.push(engine, c);
                return Some(true);
            }
        }
        if let Some(nth) = sym.char().and_then(|c| self.select_key_index(c)) {
            // 分号选第 2 个、引号选第 3 个；没这么多候选就什么都不做
            if nth <= self.list.page_len() {
                self.commit_candidate(engine, self.list.page_start() + nth - 1);
            }
            return Some(true);
        }
        if let Some(digit) = sym.digit() {
            // 数字（含小键盘）按当前页选词
            if digit >= 1 && digit <= self.list.page_len() {
                self.commit_candidate(engine, self.list.page_start() + digit - 1);
            }
            return Some(true);
        }
        if sym == Sym::Char(';') && self.semicolon_is_key() {
            // 微软、搜狗、紫光双拼用分号当 ing：等第二个键时分号进编辑区
            self.push(engine, ';');
            return Some(true);
        }
        if self.list.len > 0 && self.list_key(sym, modifiers) {
            return Some(true);
        }
        let symbol = match sym.char() {
            Some('\'') => None,
            Some(c) => crate::symbols::fullwidth_punct(c)
                .map(str::to_string)
                .or_else(|| {
                    // 其他可见字符（/ " + _ 等）原样上屏
                    (c.is_ascii_graphic() && !c.is_ascii_alphabetic()).then(|| c.to_string())
                }),
            None => None,
        };
        if let Some(symbol) = symbol {
            // 打字途中按标点：先上屏当前候选，再上屏标点。光标停在句首词上时，
            // 选完它还剩半句：接着选第一个候选，直到整句上屏
            for _ in 0..PUNCT_COMMIT_GUARD {
                if !self.composing() {
                    break;
                }
                self.commit_current(engine);
            }
            self.commit_plain(engine, &symbol);
            return Some(true);
        }
        // 编辑区里光标没法移动，这些键吃掉，免得应用里的光标乱跑
        if let Sym::Named(key) = sym {
            if matches!(
                key,
                Key::Left
                    | Key::Right
                    | Key::Up
                    | Key::Down
                    | Key::Home
                    | Key::End
                    | Key::PageUp
                    | Key::PageDown
                    | Key::Tab
                    | Key::Delete
                    | Key::Other
            ) {
                return Some(true);
            }
        }
        None
    }

    /// 候选列表的移动和翻页。横排时左右键移动、上下键翻页（下键展开网格）；竖排反过来。
    fn list_key(&mut self, sym: Sym, modifiers: u32) -> bool {
        // fcitx5 的默认热键：Tab 下一个候选，Shift+Tab 上一个
        if sym == Sym::Named(Key::Tab) {
            self.list.move_cursor(modifiers & mods::SHIFT != 0);
            return true;
        }
        let horizontal = self.config.candidate_layout == Layout::Horizontal;
        let (prev_key, next_key, prev_page_key, next_page_key) = if horizontal {
            (Key::Left, Key::Right, Key::Up, Key::Down)
        } else {
            (Key::Up, Key::Down, Key::Left, Key::Right)
        };
        if horizontal && sym == Sym::Named(Key::Down) && self.candidates.len() > 1 {
            // 横排时下键展开网格，光标停在当前选中的词上
            self.expanded = true;
            self.grid_cursor = self.list.cursor;
            return true;
        }
        if sym == Sym::Named(prev_key) {
            self.list.move_cursor(true);
        } else if sym == Sym::Named(next_key) {
            self.list.move_cursor(false);
        } else if matches!(sym, Sym::Named(Key::PageUp) | Sym::Char('-'))
            || sym == Sym::Named(prev_page_key)
        {
            if !self.list.has_prev() {
                return false;
            }
            self.list.prev();
        } else if matches!(sym, Sym::Named(Key::PageDown) | Sym::Char('='))
            || sym == Sym::Named(next_page_key)
        {
            if !self.list.has_next() {
                return false;
            }
            self.list.next();
        } else {
            return false;
        }
        true
    }

    /// 展开的网格里的按键。返回 false 表示已收起，按键接着照常处理。
    fn grid_key(&mut self, engine: Option<&Engine>, sym: Sym) -> bool {
        let n = self.candidates.len();
        if n == 0 {
            self.expanded = false;
            return false;
        }
        let screen = GRID_COLS * GRID_ROWS;
        let cursor = self.grid_cursor;
        if let Some(digit) = sym.digit() {
            // 数字（含小键盘）选光标所在行的第几个；别的数字吃掉
            let index = cursor / GRID_COLS * GRID_COLS + digit;
            if (1..=GRID_COLS).contains(&digit) && index <= n {
                self.commit_candidate(engine, index - 1);
            }
            return true;
        }
        match sym {
            Sym::Named(Key::Left) => self.grid_cursor = cursor.saturating_sub(1),
            Sym::Named(Key::Right) => self.grid_cursor = (cursor + 1).min(n - 1),
            Sym::Named(Key::Up) if cursor < GRID_COLS => self.collapse(), // 第一行再往上：收起
            Sym::Named(Key::Up) => self.grid_cursor = cursor - GRID_COLS,
            Sym::Named(Key::Down) => {
                if cursor + GRID_COLS < n {
                    self.grid_cursor = cursor + GRID_COLS;
                } else if cursor / GRID_COLS + 1 < ceil_div(n, GRID_COLS) {
                    self.grid_cursor = n - 1; // 最后一行不满时落到最后一个
                }
            }
            Sym::Named(Key::PageDown) | Sym::Char('=') => {
                self.grid_cursor = (cursor + screen).min(n - 1);
            }
            Sym::Named(Key::PageUp) | Sym::Char('-') => {
                self.grid_cursor = if cursor >= screen {
                    cursor - screen
                } else {
                    cursor % GRID_COLS
                };
            }
            Sym::Named(Key::Escape) => self.collapse(),
            Sym::Char(' ') => self.commit_candidate(engine, cursor),
            _ => {
                // 打字、退格、回车、标点：先收起，再照常处理
                self.collapse();
                return false;
            }
        }
        true
    }

    fn collapse(&mut self) {
        self.expanded = false;
        self.panel_cursor = self.grid_cursor;
        self.rebuild_list();
    }

    /// 重新算候选：词库刚载入完或改了设置之后调一次。没在编辑时什么也不做。
    pub fn refresh(&mut self, engine: Option<&mut Engine>) {
        if self.composing() {
            self.refresh_candidates(engine.map(|e| &*e));
        }
    }

    /// 鼠标点选这一屏的第 index 个候选。没有这个候选时返回 false。
    pub fn select(&mut self, engine: Option<&mut Engine>, index: usize) -> bool {
        let (start, count) = self.screen();
        if index >= count {
            return false;
        }
        self.commit_candidate(engine.map(|e| &*e), start + index);
        true
    }

    /// 鼠标滚轮/翻页按钮：delta < 0 上一页，> 0 下一页。已经到头时返回 false。
    pub fn page(&mut self, delta: i32) -> bool {
        if !self.composing() || self.candidates.is_empty() || delta == 0 {
            return false;
        }
        if self.expanded {
            let screen = GRID_COLS * GRID_ROWS;
            let n = self.candidates.len();
            if delta > 0 && (self.grid_cursor / screen + 1) * screen < n {
                self.grid_cursor = (self.grid_cursor + screen).min(n - 1);
                return true;
            }
            if delta < 0 && self.grid_cursor >= screen {
                self.grid_cursor -= screen;
                return true;
            }
            return false;
        }
        if delta < 0 && self.list.has_prev() {
            self.list.prev();
            return true;
        }
        if delta > 0 && self.list.has_next() {
            self.list.next();
            return true;
        }
        false
    }

    /// 焦点离开、应用要求重置：丢掉编辑中的内容，什么也不上屏。
    pub fn reset(&mut self) {
        self.clear();
    }

    /// 切换输入法、应用要求结束编辑：已选的词 + 剩下的原始字母上屏，然后清空。
    pub fn commit_composition(&mut self, engine: Option<&mut Engine>) {
        if self.composing() {
            let tail = self.remaining();
            self.commit_all(engine.map(|e| &*e), &tail, "");
        }
    }

    /// 自上次调用以来要上屏的文字，没有时 None。取一次就清空。
    pub fn take_commit(&mut self) -> Option<String> {
        (!self.commit.is_empty()).then(|| std::mem::take(&mut self.commit))
    }

    pub fn is_composing(&self) -> bool {
        self.composing()
    }

    /// 编辑区全文：前 `preedit_plain_len` 字节是已选的词，后面是还在打的拼音。
    pub fn preedit(&self) -> &str {
        &self.preedit
    }

    pub fn preedit_plain_len(&self) -> usize {
        self.plain_len
    }

    /// 光标的字节位置（总在末尾）。
    pub fn preedit_cursor(&self) -> usize {
        self.preedit.len()
    }

    /// 候选框顶上的提示，这一版总是空串。
    pub fn aux(&self) -> &str {
        ""
    }

    /// 这一屏的候选：列表是当前页，网格是光标所在的那一屏（最多 36 个）。
    fn screen(&self) -> (usize, usize) {
        if !self.composing() || self.candidates.is_empty() {
            return (0, 0);
        }
        if self.expanded {
            let screen = GRID_COLS * GRID_ROWS;
            let start = self.grid_cursor / screen * screen;
            return (start, (self.candidates.len() - start).min(screen));
        }
        (self.list.page_start(), self.list.page_len())
    }

    pub fn candidate_count(&self) -> usize {
        self.screen().1
    }

    /// 这一屏第 index 个候选显示的文字。越界返回 None。
    pub fn candidate_text(&self, index: usize) -> Option<&str> {
        let (start, count) = self.screen();
        (index < count).then(|| self.candidates[start + index].display.as_str())
    }

    /// 选择键（"1"……"9"）；网格里只有光标所在行有，其余是空串。越界返回 None。
    pub fn candidate_label(&self, index: usize) -> Option<&'static str> {
        let (start, count) = self.screen();
        if index >= count {
            return None;
        }
        if !self.expanded {
            return Some(LABELS[index % LABELS.len()]);
        }
        let at = start + index;
        Some(if at / GRID_COLS == self.grid_cursor / GRID_COLS {
            LABELS[at % GRID_COLS]
        } else {
            ""
        })
    }

    /// 高亮的候选在这一屏的位置。光标不在这一页时（网格收起后可能这样）是 0，和空格选的一致。
    pub fn candidate_cursor(&self) -> usize {
        let (start, count) = self.screen();
        if count == 0 {
            0
        } else if self.expanded {
            self.grid_cursor - start
        } else {
            self.list.cursor_on_page().unwrap_or(0)
        }
    }

    /// 0 表示列表，大于 0 表示展开的网格每行几个。
    pub fn columns(&self) -> usize {
        if self.expanded && self.candidate_count() > 0 {
            GRID_COLS
        } else {
            0
        }
    }

    /// 列表竖排。
    pub fn vertical(&self) -> bool {
        self.config.candidate_layout == Layout::Vertical
    }

    pub fn has_prev_page(&self) -> bool {
        if self.screen().1 == 0 {
            false
        } else if self.expanded {
            self.grid_cursor >= GRID_COLS * GRID_ROWS
        } else {
            self.list.has_prev()
        }
    }

    pub fn has_next_page(&self) -> bool {
        let (start, count) = self.screen();
        if count == 0 {
            false
        } else if self.expanded {
            start + count < self.candidates.len()
        } else {
            self.list.has_next()
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// 学够 20 次时返回 true 并清零，前端这时把用户词库和输入习惯存盘。
    pub fn take_save_request(&mut self) -> bool {
        std::mem::take(&mut self.save_requested)
    }

    // ---- 内部 ----

    fn composing(&self) -> bool {
        !self.buffer.is_empty()
    }

    fn selected_bytes(&self) -> usize {
        self.selections.iter().map(|s| s.bytes).sum()
    }

    /// 还没选词的那部分输入。
    fn remaining(&self) -> String {
        self.buffer
            .get(self.selected_bytes()..)
            .unwrap_or_default()
            .to_string()
    }

    fn selected_text(&self) -> String {
        self.selections.iter().map(|s| s.text.as_str()).collect()
    }

    fn clear(&mut self) {
        self.buffer.clear();
        self.selections.clear();
        self.candidates.clear();
        self.expanded = false;
        self.grid_cursor = 0;
        self.panel_cursor = 0;
        self.list = List::default();
        self.preedit.clear();
        self.plain_len = 0;
    }

    fn push(&mut self, engine: Option<&Engine>, c: char) {
        self.buffer.push(c);
        self.refresh_candidates(engine);
    }

    /// 新的候选列表：第一页，光标在 panel_cursor（网格收起时可能不在第一页）。
    fn rebuild_list(&mut self) {
        let len = self.candidates.len();
        self.list = if len == 0 {
            List::default()
        } else {
            List {
                len,
                page_size: self.config.page_size.max(1),
                page: 0,
                cursor: self.panel_cursor.min(len - 1),
            }
        };
    }

    fn calc_active(&self) -> bool {
        if !self.config.calculator || self.config.shuangpin || !self.selections.is_empty() {
            return false;
        }
        match self.buffer.strip_prefix('v') {
            Some(rest) => rest
                .chars()
                .all(|c| c.is_ascii_digit() || "+-*/%^.()".contains(c)),
            None => false,
        }
    }

    fn select_key_index(&self, c: char) -> Option<usize> {
        match (self.config.select_keys, c) {
            (SelectKeys::Off, _) => None,
            // 微软、搜狗、紫光双拼里分号是 ing 键，不能拿来选词
            (_, ';') if self.shuangpin_semicolon() => None,
            (_, ';') => Some(2),
            (SelectKeys::SemicolonQuote, '\'') => Some(3),
            _ => None,
        }
    }

    fn shuangpin_semicolon(&self) -> bool {
        self.config.shuangpin && self.config.shuangpin_scheme.uses_semicolon()
    }

    fn semicolon_is_key(&self) -> bool {
        if !self.shuangpin_semicolon() || !self.composing() {
            return false;
        }
        self.remaining().chars().filter(|c| *c != '\'').count() % 2 == 1
    }

    fn refresh_candidates(&mut self, engine: Option<&Engine>) {
        self.candidates.clear();
        self.expanded = false;
        self.grid_cursor = 0;
        self.panel_cursor = 0;
        let input = self.remaining();
        if let (false, Some(engine)) = (input.is_empty(), engine) {
            if self.calc_active() && input.len() > 1 {
                // V 模式：v 后面是算式，候选是结果和「算式=结果」
                self.candidates = crate::calc::candidates(&input[1..])
                    .into_iter()
                    .map(|text| Cand::plain(text, input.len()))
                    .collect();
            } else {
                self.fill_candidates(engine, &input);
            }
        }
        self.rebuild_list();
        self.update_preedit(engine);
    }

    fn fill_candidates(&mut self, engine: &Engine, input: &str) {
        // 已经选了开头的词时，把它当上文，剩下的部分排序更准
        engine.set_context(&utf8_tail(
            &(self.recent.clone() + &self.selected_text()),
            CONTEXT_BYTES,
        ));
        self.candidates = engine
            .input(input)
            .into_iter()
            .map(|c| {
                let consumed = if c.consumed == 0 || c.consumed > input.len() {
                    input.len()
                } else {
                    c.consumed
                };
                // 自定义词里带换行的，不往用户词库里记（词库是一行一条）
                let plain = c.text.contains(['\n', '\t']);
                Cand {
                    plain,
                    ..Cand::new(c.text, c.pinyin, consumed)
                }
            })
            .collect();
        if self.config.emoji_suggest {
            self.add_emoji(input);
            self.add_keycap(input);
        }
        if self.config.date_time && self.selections.is_empty() {
            self.add_datetime(input);
        }
    }

    /// 首选用完整个输入时，按它推荐一个 emoji，插在第三个。
    fn add_emoji(&mut self, input: &str) {
        let Some(top) = self
            .candidates
            .first()
            .filter(|c| c.consumed >= input.len())
        else {
            return;
        };
        let top = top.text.clone();
        let emoji: Vec<Cand> = crate::emoji::recommend(&top, 1)
            .into_iter()
            .map(|s| Cand::emoji(s.display, s.commit, input.len()))
            .collect();
        let at = EMOJI_SLOT.min(self.candidates.len());
        self.candidates.splice(at..at, emoji);
    }

    /// 第一页里有数字字就在它后面跟按键 emoji（ba：吧 八 8️⃣ 把）。
    fn add_keycap(&mut self, input: &str) {
        let window = self.candidates.len().min(self.config.page_size);
        for i in 0..window {
            let c = &self.candidates[i];
            if c.emoji.is_some() || c.consumed < input.len() {
                continue;
            }
            if let Some(keycap) = crate::emoji::keycap(&c.text) {
                let cand = Cand::emoji(keycap.to_string(), keycap.to_string(), input.len());
                self.candidates.insert(i + 1, cand);
                return;
            }
        }
    }

    /// riqi / shijian / xingqi：首选还是那个词，日期时间的写法从第二个开始。
    fn add_datetime(&mut self, input: &str) {
        let Some(top) = self.candidates.first() else {
            return;
        };
        // 全拼看打的字母（rq、sj 这类简拼不算）；双拼看首选的拼音
        let key = if self.config.shuangpin {
            if top.consumed < input.len() {
                return;
            }
            top.pinyin.clone()
        } else {
            input.to_string()
        };
        let time_ms = if self.time_ms > 0 {
            self.time_ms
        } else {
            now_ms()
        };
        let now = LocalTime::from_unix_ms(time_ms, self.utc_offset_seconds);
        let extra: Vec<Cand> = crate::datetime::candidates(&key, &now)
            .into_iter()
            .map(|text| Cand::plain(text, input.len()))
            .collect();
        self.candidates.splice(1..1, extra);
    }

    /// 编辑区：已选的词 + 还没选的拼音（按第一个用完它的候选分好音节，双拼显示成全拼）。
    fn update_preedit(&mut self, engine: Option<&Engine>) {
        if !self.composing() {
            self.preedit.clear();
            self.plain_len = 0;
            return;
        }
        let selected = self.selected_text();
        let input = self.remaining();
        let shown = if self.config.shuangpin {
            engine
                .and_then(|e| e.shuangpin_display(&input))
                .filter(|s| !s.is_empty())
        } else {
            self.candidates
                .iter()
                .find(|c| c.consumed >= input.len())
                .and_then(|c| crate::preedit::segment_input(&input, &c.pinyin))
        };
        self.plain_len = selected.len();
        self.preedit = selected + shown.as_deref().unwrap_or(&input);
    }

    /// 空格：上屏光标处的候选；光标不在这页时上屏这页第一个；没有候选时上屏原始字母。
    fn commit_current(&mut self, engine: Option<&Engine>) {
        if self.expanded && !self.candidates.is_empty() {
            self.commit_candidate(engine, self.grid_cursor);
        } else if self.list.page_len() > 0 {
            let at = self.list.page_start() + self.list.cursor_on_page().unwrap_or(0);
            self.commit_candidate(engine, at);
        } else {
            let tail = self.remaining();
            self.commit_all(engine, &tail, "");
        }
    }

    fn commit_candidate(&mut self, engine: Option<&Engine>, index: usize) {
        let Some(chosen) = self.candidates.get(index).cloned() else {
            return;
        };
        let input_len = self.remaining().len();
        if let Some(commit) = chosen.emoji {
            // emoji：只上屏（句尾推荐是整句 + emoji），不学习
            let text = self.selected_text() + &commit;
            self.clear();
            self.commit_plain(engine, &text);
        } else if chosen.plain && chosen.consumed >= input_len {
            let text = self.selected_text() + &chosen.text;
            self.clear();
            self.commit_plain(engine, &text);
        } else if chosen.consumed < input_len {
            // 句首词：先放进已选栈，剩下的拼音继续选
            if let Some(engine) = engine {
                let pinyin = Some(chosen.pinyin.as_str()).filter(|p| !p.is_empty());
                engine.record_selection_with_pinyin(&chosen.text, pinyin);
            }
            self.selections.push(Selection {
                text: chosen.text,
                pinyin: chosen.pinyin,
                bytes: chosen.consumed,
            });
            self.refresh_candidates(engine);
        } else {
            self.commit_all(engine, &chosen.text, &chosen.pinyin);
        }
    }

    /// 已选的词 + tail 上屏并学习：tail 记一次，分几次选出来的整句也记一次。
    fn commit_all(&mut self, engine: Option<&Engine>, tail: &str, tail_pinyin: &str) {
        let text = self.selected_text() + tail;
        self.emit(&text);
        if let (Some(engine), false) = (engine, text.is_empty()) {
            if !tail.is_empty() {
                let pinyin = Some(tail_pinyin).filter(|p| !p.is_empty());
                engine.record_selection_with_pinyin(tail, pinyin);
            }
            if !self.selections.is_empty() && !tail.is_empty() && !tail_pinyin.is_empty() {
                let mut pinyin: String = self
                    .selections
                    .iter()
                    .map(|s| format!("{} ", s.pinyin))
                    .collect();
                pinyin.push_str(tail_pinyin);
                engine.record_selection_with_pinyin(&text, Some(&pinyin));
            }
            self.unsaved += 1;
        }
        self.clear();
        self.remember_context(engine, &text);
        if self.unsaved >= SAVE_EVERY {
            self.unsaved = 0;
            self.save_requested = true;
        }
    }

    /// 上屏不学习的文字（标点、emoji、计算结果……），只记进上文。
    fn commit_plain(&mut self, engine: Option<&Engine>, text: &str) {
        if !text.is_empty() {
            self.emit(text);
            self.remember_context(engine, text);
        }
    }

    fn emit(&mut self, text: &str) {
        self.commit.push_str(text);
    }

    fn remember_context(&mut self, engine: Option<&Engine>, text: &str) {
        self.recent = utf8_tail(&(self.recent.clone() + text), CONTEXT_BYTES);
        if let Some(engine) = engine {
            engine.set_context(&self.recent);
        }
    }

    /// 单按 Shift 切中英文：按下记下来，松开时中间没按别的键、也没按太久就切。
    /// 切了返回 true（这个松开事件算用掉）。
    fn shift_toggle(&mut self, engine: Option<&Engine>, ev: &KeyEvent) -> bool {
        let is_shift = matches!(ev.key, Key::ShiftL | Key::ShiftR);
        let wanted = is_shift
            && match self.config.shift_toggle {
                ShiftToggle::Both => true,
                ShiftToggle::Left => ev.key == Key::ShiftL,
                ShiftToggle::Right => ev.key == Key::ShiftR,
                ShiftToggle::Off => false,
            };
        if !ev.is_release {
            let other = ev.modifiers & (mods::CONTROL | mods::ALT | mods::SUPER) != 0;
            self.shift_down = (wanted && !other).then_some(ev.key);
            self.shift_down_at = ev.time_ms;
            return false;
        }
        if !wanted || self.shift_down != Some(ev.key) {
            if !is_shift {
                self.shift_down = None;
            }
            return false;
        }
        self.shift_down = None;
        // 按住 Shift 超过 0.5 秒多半是想打大写或者犹豫了，不切
        if ev.time_ms - self.shift_down_at > SHIFT_TAP_MS {
            return false;
        }
        if self.mode == Mode::Ascii {
            self.mode = Mode::Chinese;
        } else {
            self.mode = Mode::Ascii;
            if self.composing() {
                // 打到一半切英文：已选的词 + 打的字母原样上屏
                let tail = self.remaining();
                self.commit_all(engine, &tail, "");
            }
        }
        true
    }
}

/// V 模式计算器认的字符（数字、运算符、括号），小键盘的数字和运算符也算。
fn calc_char(sym: Sym) -> Option<char> {
    match sym {
        Sym::Char(c) if c.is_ascii_digit() || "+-*/.%^()".contains(c) => Some(c),
        Sym::Keypad(c) if c.is_ascii_digit() || "+-*/.".contains(c) => Some(c),
        _ => None,
    }
}

/// 向上取整的除法（MSRV 1.70 还没有 usize::div_ceil）。
fn ceil_div(a: usize, b: usize) -> usize {
    let b = b.max(1);
    a / b + usize::from(a % b != 0)
}

/// 候选窗里显示的样子：换行、制表符画成 ⏎、⇥，不把候选窗撑乱。
fn shown_text(text: &str) -> String {
    text.replace('\n', "⏎").replace('\t', "⇥")
}

/// 只留尾部 max 字节，起点对齐到字符边界。
fn utf8_tail(text: &str, max: usize) -> String {
    let mut start = text.len().saturating_sub(max);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_string()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_tail_keeps_whole_chars() {
        assert_eq!(utf8_tail("abc", 24), "abc");
        assert_eq!(utf8_tail("中国人民", 7), "人民");
        assert_eq!(utf8_tail("中国人民", 6), "人民");
        assert_eq!(utf8_tail("中国人民", 5), "民");
    }

    #[test]
    fn list_paging_like_fcitx() {
        let mut list = List {
            len: 20,
            page_size: 9,
            page: 0,
            cursor: 3,
        };
        list.next();
        assert_eq!((list.page, list.cursor), (1, 9), "翻页后光标回到这页第一个");
        list.next();
        assert_eq!((list.page, list.cursor, list.page_len()), (2, 18, 2));
        assert!(!list.has_next());
        list.move_cursor(false);
        list.move_cursor(false);
        assert_eq!((list.page, list.cursor), (0, 0), "最后一个再往后绕回第一个");
        // 光标不在这页时翻页不动光标；移动时先落到这页的头或尾
        let mut list = List {
            len: 20,
            page_size: 9,
            page: 0,
            cursor: 12,
        };
        assert_eq!(list.cursor_on_page(), None);
        list.move_cursor(true);
        assert_eq!((list.page, list.cursor), (0, 8));
    }
}
