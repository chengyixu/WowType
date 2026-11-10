//! 输入会话的按键行为：把 fcitx5-addon/test/key_harness.cpp 里的按键测试逐条搬过来，
//! 在测试自己写的小词库上跑（不读 dist/data，不碰真的 fcitx5）。
//!
//! 每条注明在 key_harness.cpp 里的行号。没搬的：
//!
//! - 137-158：起私有 fcitx5、等后台载入词库。会话直接拿到引擎，不用等。
//! - 283-287：WOWTYPE_HARNESS_SHOW 调试打印。
//! - 289-292 里「１〔词〕」这种拼好的行文字：那是 fcitx5 画网格的办法，会话只给每格的词和
//!   选择键，由前端画；这里改为检查每行 6 个、光标和选择键。
//! - 425-473：表情面板（Super+.），这一版不做（macOS 有系统表情面板）。
//! - 590-611：fcitx5 全局翻页热键（Hotkey/PrevPage 设成逗号等）。那是 fcitx5 的全局设置，
//!   不在会话的设置里；会话只照 fcitx5 的默认热键（Tab / Shift+Tab 移动候选）。
//! - 743-977：互译，这一版不做。
//! - 979-1036：语音，这一版不做。374-378、537-538 的右 Alt 照常搬（这一版右 Alt 就是普通修饰键，
//!   交给应用）。
//! - 1038-1063：main 里起实例的代码。
//!
//! research.json 的 test_vectors 里，除了上面这些，还有几条不是会话的（segment_input、
//! 引擎的上下文和学习、双拼组合、datetime 单测），分别在 core/src/preedit.rs、tests/session.rs、
//! tests/combinations.rs、core/src/datetime.rs 里测。
//!
//! 插件的两种事件在会话里对应：失去焦点 = `reset()`，切换输入法 = `commit_composition()`。

use std::sync::Arc;

use wowtype_core::config;
use wowtype_core::lm::BigramLm;
use wowtype_core::session::{mods, Key, KeyEvent, Mode, Session};
use wowtype_core::{Engine, Lexicon, MemoryLexicon};

/// 测试词库：只放这些用例要用到的词。
const LEXICON: &str = "\
ni hao\t你好\t100000
ni\t你\t60000
ni\t泥\t8000
ni\t尼\t6000
hao\t好\t70000
zhong guo\t中国\t90000
zhong\t中\t60000
zhong\t重\t30000
zhong\t种\t20000
guo\t国\t40000
zhong dian\t重点\t60000
zhong di\t种地\t2000
di\t地\t50000
ka\t卡\t20000
kan\t看\t90000
dan\t但\t40000
dan\t蛋\t9000
da\t大\t90000
heng\t横\t9000
heng xiang\t横向\t5000
xiang\t向\t40000
hou xuan\t候选\t5000
hou xuan ci\t候选词\t2000
ci\t词\t30000
ke yi\t可以\t90000
ke\t可\t40000
yi\t一\t90000
yong\t用\t50000
zhong wen\t中文\t40000
wen\t文\t30000
ji xu\t继续\t30000
ren min\t人民\t40000
kai xin\t开心\t30000
kai\t开\t40000
xin\t心\t30000
jin tian\t今天\t60000
ba\t吧\t50000
ba\t八\t40000
ba\t把\t30000
ba\t爸\t10000
ge\t个\t80000
shi\t是\t90000
shi\t事\t60000
shi\t时\t55000
shi\t十\t50000
shi\t市\t40000
shi\t使\t35000
shi\t世\t30000
shi\t式\t25000
shi\t试\t20000
shi\t实\t18000
shi\t师\t15000
shi\t诗\t12000
shi\t石\t10000
shi\t识\t9000
shi\t史\t8000
zhi\t之\t90000
zhi\t只\t80000
zhi\t知\t70000
zhi\t指\t60000
zhi\t直\t50000
zhi\t至\t40000
zhi\t制\t30000
zhi\t值\t20000
zhi\t支\t10000
zhi\t纸\t9000
ri qi\t日期\t30000
shi jian\t时间\t60000
xing qi\t星期\t30000
xing\t行\t50000
xing\t星\t30000
niu nai\t牛奶\t20000
liu\t流\t30000
nai\t耐\t20000
xi an\t西安\t20000
xi\t西\t30000
an\t安\t30000
wo\t我\t90000
de\t的\t90000
";

const ENGLISH: &str = "GitHub\nhello\nvery\n";

/// 一个会话 + 引擎，像插件那样把上屏和交给应用的字记下来。
struct Rig {
    engine: Option<Engine>,
    session: Session,
    committed: String,
    passed: String,
    now_ms: i64,
    utc_offset: i32,
}

fn engine() -> Engine {
    let lexicon: Arc<dyn Lexicon> = Arc::new(MemoryLexicon::from_tsv_str(LEXICON).unwrap());
    let mut e = Engine::new(lexicon, Arc::new(BigramLm::default()));
    e.load_english_str(ENGLISH);
    config::apply_to_engine(&mut e, &config::Config::default());
    e
}

impl Rig {
    fn new() -> Self {
        Self::with(Some(engine()))
    }

    fn with(engine: Option<Engine>) -> Self {
        Self {
            engine,
            session: Session::new(),
            committed: String::new(),
            passed: String::new(),
            // 2026-11-10 09:05:01（东八区），星期二
            now_ms: 1_794_272_701_000,
            utc_offset: 8 * 3600,
        }
    }

    /// 像插件的 reloadConfig：设置文本同时给会话和引擎。
    fn configure(&mut self, ini: &str) {
        assert_eq!(self.session.configure(ini), 0, "{ini}");
        if let Some(e) = self.engine.as_mut() {
            config::apply_to_engine(e, &config::parse(ini).0);
        }
    }

    fn custom(&mut self, tsv: &str) {
        self.engine.as_ref().unwrap().lexicon().set_custom_tsv(tsv);
    }

    fn event(&mut self, key: Key, unicode: Option<char>, modifiers: u32, release: bool) -> bool {
        self.now_ms += 10;
        let ev = KeyEvent {
            key,
            unicode,
            modifiers,
            is_release: release,
            utc_offset_seconds: self.utc_offset,
            time_ms: self.now_ms,
        };
        let handled = self.session.process_key(self.engine.as_mut(), &ev);
        self.collect();
        handled
    }

    fn collect(&mut self) {
        if let Some(text) = self.session.take_commit() {
            assert!(!text.is_empty(), "take_commit 不该给空串");
            self.committed += &text;
        }
    }

    /// 按下再松开（和 harness 的 send 一样）。没被接住的字符记进 passed。
    fn send(&mut self, key: Key, unicode: Option<char>, modifiers: u32) -> bool {
        let handled = self.event(key, unicode, modifiers, false);
        self.event(key, unicode, modifiers, true);
        if !handled {
            if let Some(c) = unicode {
                self.passed.push(c);
            }
        }
        handled
    }

    fn ch(&mut self, c: char) -> bool {
        let modifiers = if c.is_ascii_uppercase() {
            mods::SHIFT
        } else {
            0
        };
        self.send(Key::None, Some(c), modifiers)
    }

    fn key(&mut self, key: Key) -> bool {
        self.send(key, None, 0)
    }

    /// 按一串键。<sp> <bs> <cr> <esc> <down> <up> <left> <right> <pgdn> <pgup> <tab> 是特殊键。
    fn ty(&mut self, keys: &str) {
        let mut rest = keys;
        while let Some(c) = rest.chars().next() {
            if c == '<' {
                let end = rest.find('>').unwrap();
                let name = &rest[1..end];
                rest = &rest[end + 1..];
                match name {
                    "sp" => self.ch(' '),
                    "bs" => self.key(Key::Backspace),
                    "cr" => self.key(Key::Return),
                    "esc" => self.key(Key::Escape),
                    "down" => self.key(Key::Down),
                    "up" => self.key(Key::Up),
                    "left" => self.key(Key::Left),
                    "right" => self.key(Key::Right),
                    "pgdn" => self.key(Key::PageDown),
                    "pgup" => self.key(Key::PageUp),
                    "tab" => self.key(Key::Tab),
                    other => panic!("不认识的键 <{other}>"),
                };
                continue;
            }
            rest = &rest[c.len_utf8()..];
            self.ch(c);
        }
    }

    fn preedit(&self) -> String {
        self.session.preedit().to_string()
    }

    fn page(&self) -> Vec<String> {
        (0..self.session.candidate_count())
            .map(|i| self.session.candidate_text(i).unwrap().to_string())
            .collect()
    }

    fn labels(&self) -> Vec<String> {
        (0..self.session.candidate_count())
            .map(|i| self.session.candidate_label(i).unwrap().to_string())
            .collect()
    }

    /// 每个用例前清空上屏记录和编辑区（harness 的 fresh）。
    fn fresh(&mut self) {
        self.ty("<esc>");
        self.committed.clear();
        self.passed.clear();
    }

    fn tap_shift(&mut self, key: Key, hold_ms: i64) -> bool {
        self.event(key, None, 0, false);
        self.now_ms += hold_ms;
        self.event(key, None, mods::SHIFT, true)
    }

    fn history(&self, word: &str) -> u32 {
        self.engine.as_ref().unwrap().history_count(word)
    }
}

/// 只看上屏结果的用例：(harness 行号, 设置, 按键, 上屏, 交给应用)。
const COMMIT_VECTORS: &[(u32, &str, &str, &str, &str)] = &[
    (160, "", "nihao<sp>", "你好", ""),
    (163, "", "nihao,", "你好，", ""),
    (167, "", "zhognguo<sp>", "中国", ""),
    (171, "", "nihao<cr>", "nihao", ""),
    (175, "", "github<sp>", "GitHub", ""),
    (179, "", "Hello<sp>", "Hello", ""),
    (185, "", "zhongdi<sp>", "种地", ""),
    (187, "", "ka<sp>", "卡", ""),
    (334, "", "ni<bs><bs>,", "，", ""),
    (339, "", "1", "", "1"),
    (368, "", "hengxiang<cr>", "hengxiang", ""),
    (382, "", "/jixu<sp>", "继续", "/"),
    (386, "", "nihao/", "你好/", ""),
    (390, "", "nihao+", "你好+", ""),
    (
        620,
        "Shuangpin=True\nShuangpinScheme=Microsoft\n",
        "vsgo<sp>",
        "中国",
        "",
    ),
    (
        627,
        "Shuangpin=True\nShuangpinScheme=Microsoft\n",
        "ni;",
        "你；",
        "",
    ),
    (661, "", "v1+2*3<sp>", "7", ""),
    (667, "", "v(1.5-0.5)/4=", "0.25", ""),
    (710, "", "ni;", "你；", ""),
];

#[test]
fn commit_vectors() {
    for &(line, ini, keys, committed, passed) in COMMIT_VECTORS {
        let mut rig = Rig::new();
        rig.configure(ini);
        rig.ty(keys);
        assert_eq!(
            (rig.committed.as_str(), rig.passed.as_str()),
            (committed, passed),
            "key_harness.cpp:{line} {keys}"
        );
    }
}

/// 只看编辑区的用例：(行号, 设置, 按键, 编辑区)。
const PREEDIT_VECTORS: &[(u32, &str, &str, &str)] = &[
    (366, "", "hengxiang", "heng'xiang"),
    (371, "", "zhongg", "zhong'g"),
    (
        619,
        "Shuangpin=True\nShuangpinScheme=Microsoft\n",
        "vsgo",
        "zhong'guo",
    ),
    (
        624,
        "Shuangpin=True\nShuangpinScheme=Microsoft\n",
        "x;",
        "xing",
    ),
    (659, "", "v1+2*3", "v1+2*3"),
    (665, "", "v(1.5-0.5)/4", "v(1.5-0.5)/4"),
    (
        724,
        "SelectKeys=Semicolon\nEmojiSuggest=False\n",
        "xi'an",
        "xi'an",
    ),
];

#[test]
fn preedit_vectors() {
    for &(line, ini, keys, preedit) in PREEDIT_VECTORS {
        let mut rig = Rig::new();
        rig.configure(ini);
        rig.ty(keys);
        assert_eq!(rig.preedit(), preedit, "key_harness.cpp:{line} {keys}");
        assert_eq!(rig.committed, "", "key_harness.cpp:{line} {keys} 不该上屏");
        assert!(rig.session.is_composing());
        // 编辑区全是还在打的拼音，光标在末尾
        assert_eq!(rig.session.preedit_plain_len(), 0);
        assert_eq!(rig.session.preedit_cursor(), preedit.len());
    }
}

/// 只看第一个候选的用例：(行号, 设置, 自定义词, 按键, 第一个候选)。
const FIRST_VECTORS: &[(u32, &str, &str, &str, &str)] = &[
    (223, "", "", "kyi", "可以"),
    (223, "", "", "yongzwen", "用中文"),
    (530, "", "ni hao\t妮好\n", "nihao", "妮好"),
    (553, "", "", "nihao", "你好"),
    (659, "", "", "v1+2*3", "7"),
    (666, "", "", "v(1.5-0.5)/4", "0.25"),
    (
        697,
        "EmojiSuggest=False\n",
        "mail\tme@example.com\n",
        "mail",
        "me@example.com",
    ),
    (
        702,
        "EmojiSuggest=False\n",
        "di zhi\t四川省\\n宜宾市\n",
        "dizhi",
        "四川省⏎宜宾市",
    ),
];

#[test]
fn first_candidate_vectors() {
    for &(line, ini, custom, keys, first) in FIRST_VECTORS {
        let mut rig = Rig::new();
        rig.configure(ini);
        rig.custom(custom);
        rig.ty(keys);
        assert_eq!(
            rig.page().first().map(String::as_str),
            Some(first),
            "key_harness.cpp:{line} {keys}：{:?}",
            rig.page()
        );
    }
}

#[test]
fn exact_word_and_head_words() {
    // 190-198：单音节不出拆开音节的句首词（dan -> 大）
    let mut rig = Rig::new();
    rig.ty("dan");
    let dan = rig.page();
    assert!(
        !dan.is_empty() && !dan.contains(&"大".to_string()),
        "{dan:?}"
    );
    // 200-215：长句第一页：整句 + 词 + 单字，只差最后一个字的整句只有一个
    rig.fresh();
    rig.ty("hengxianghouxuanci");
    let heng = rig.page();
    let tail_variants = heng
        .iter()
        .filter(|t| t.starts_with("横向候选") && t.chars().count() == 5)
        .count();
    assert!(
        heng.first().map(String::as_str) == Some("横向候选词")
            && tail_variants == 1
            && heng.contains(&"横".to_string()),
        "{heng:?}"
    );
}

#[test]
fn head_word_select_and_undo() {
    // 227-257
    let mut rig = Rig::new();
    rig.ty("nihaozhongguo");
    let at = rig
        .page()
        .iter()
        .position(|t| t == "你好")
        .expect("第一页有句首词 你好");
    rig.ty(&(at + 1).to_string());
    assert_eq!(rig.committed, "", "选句首词不上屏");
    assert_eq!(rig.preedit(), "你好zhong'guo");
    assert_eq!(
        rig.session.preedit_plain_len(),
        "你好".len(),
        "已选的词不加下划线"
    );
    assert_eq!(rig.session.preedit_cursor(), rig.preedit().len());
    // 选句首词时马上记一次选词
    assert_eq!(rig.history("你好"), 1);
    rig.ty("<bs><bs><bs><bs><bs><bs><bs><bs>");
    assert_eq!(rig.preedit(), "你好");
    assert!(rig.session.is_composing());
    rig.ty("<bs>");
    assert_eq!(rig.preedit(), "ni'hao", "再退格撤销选词");
    assert_eq!(rig.session.preedit_plain_len(), 0);
    // 撤销选词不撤销已经记下的那次学习（插件就是这样）
    assert_eq!(rig.history("你好"), 1);
    rig.ty("zhongguo");
    let at = rig.page().iter().position(|t| t == "你好").unwrap();
    rig.ty(&(at + 1).to_string());
    rig.ty("<sp>");
    assert_eq!(rig.committed, "你好中国");
    // 整句也记一次（带每段的拼音），下次能一次打出来
    assert_eq!(rig.history("你好中国"), 1);
    assert_eq!(rig.history("中国"), 1);
}

#[test]
fn paging_and_digits() {
    // 259-269：翻页后数字按当前页选
    let mut rig = Rig::new();
    rig.ty("shi");
    let first = rig.page();
    assert_eq!(first.len(), 9, "默认每页 9 个：{first:?}");
    assert_eq!(rig.labels(), ["1", "2", "3", "4", "5", "6", "7", "8", "9"]);
    assert!(!rig.session.has_prev_page() && rig.session.has_next_page());
    rig.ty("<pgdn>");
    let second = rig.page();
    assert!(!second.is_empty() && second != first, "{second:?}");
    assert!(rig.session.has_prev_page());
    assert_eq!(rig.session.candidate_cursor(), 0, "翻页后光标在这页第一个");
    rig.ty("1");
    assert_eq!(rig.committed, second[0]);

    // = - 也翻页（换个没学过的引擎，免得刚才的选词改了排序）
    let mut rig = Rig::new();
    rig.ty("shi=");
    assert_eq!(rig.page(), second);
    rig.ty("-");
    assert_eq!(rig.page(), first);
    assert_eq!(rig.committed, "");
}

#[test]
fn page_api_like_mouse_wheel() {
    let mut rig = Rig::new();
    rig.ty("shi");
    let first = rig.page();
    assert!(!rig.session.page(-1), "第一页往前翻不动");
    assert!(rig.session.page(1));
    assert_ne!(rig.page(), first);
    while rig.session.page(1) {}
    assert!(!rig.session.has_next_page());
    assert!(rig.session.page(-1));
    assert!(rig.session.has_next_page());
    // 没在编辑时什么也不做
    rig.fresh();
    assert!(!rig.session.page(1));
}

#[test]
fn edge_page_minus_equal_commit_then_symbol() {
    // 插件的行为：第一页按 -、最后一页按 =，翻不动，就当成符号：先上屏当前候选
    let mut rig = Rig::new();
    rig.ty("nihao");
    assert!(!rig.session.has_prev_page() && !rig.session.has_next_page());
    rig.ty("-");
    assert_eq!(rig.committed, "你好-");
    rig.fresh();
    rig.ty("nihao=");
    assert_eq!(rig.committed, "你好=");
}

#[test]
fn grid_expand_move_select() {
    // 277-292：下键展开成每行 6 个的网格
    let mut rig = Rig::new();
    rig.ty("shi");
    assert_eq!(rig.session.columns(), 0);
    assert!(!rig.session.vertical(), "默认横排");
    let first_page = rig.page();
    rig.ty("<down>");
    assert_eq!(rig.session.columns(), 6);
    let grid = rig.page();
    assert!(first_page.len() >= 8 && grid.len() > 6, "{grid:?}");
    assert_eq!(grid[..6], first_page[..6], "第一行 6 个就是第一页的前 6 个");
    assert_eq!(rig.session.candidate_cursor(), 0, "光标框住第一个");
    let labels = rig.labels();
    assert_eq!(labels[..6], ["1", "2", "3", "4", "5", "6"]);
    assert!(
        labels[6..].iter().all(String::is_empty),
        "只有光标所在行有选择键"
    );
    // 293-295：右移两格空格选第三个
    rig.ty("<right><right><sp>");
    assert_eq!(rig.committed, first_page[2]);
    assert_eq!(rig.session.columns(), 0, "上屏后收起");

    // 298-303：下移一行空格选第七个
    rig.fresh();
    rig.ty("zhi");
    let now = rig.page();
    let grid_now = {
        rig.ty("<down>");
        rig.page()
    };
    rig.ty("<down>");
    assert_eq!(rig.session.candidate_cursor(), 6);
    assert_eq!(rig.labels()[6..12], ["1", "2", "3", "4", "5", "6"]);
    assert!(rig.labels()[..6].iter().all(String::is_empty));
    rig.ty("<sp>");
    assert!(now.len() > 6);
    assert_eq!(rig.committed, grid_now[6]);

    // 304-309：数字选光标所在行
    rig.fresh();
    rig.ty("zhi<down><down>2");
    assert_eq!(rig.committed, grid_now[7]);
}

#[test]
fn grid_collapse() {
    // 310-314：在第一行按上键收起
    let mut rig = Rig::new();
    rig.ty("shi");
    let first_page = rig.page();
    rig.ty("<down><down><up><up>");
    assert_eq!(rig.session.columns(), 0);
    assert_eq!(rig.page().len(), first_page.len());
    // 315-318：Esc 只收起，不清空输入
    rig.fresh();
    rig.ty("shi<down><esc>");
    assert_eq!(rig.session.columns(), 0);
    assert_eq!(rig.preedit(), "shi");
    // 319-321：展开后接着打字：收起并照常输入
    rig.ty("<down>j");
    assert_eq!(rig.session.columns(), 0);
    assert_eq!(rig.preedit(), "shi'j");
}

#[test]
fn grid_paging_and_bounds() {
    let mut rig = Rig::new();
    rig.ty("shi<down>");
    let n = rig.page().len();
    assert!(n <= 36);
    // 左键在第一个不动，PageDown 跳到最后一个（一屏装得下）
    rig.ty("<left>");
    assert_eq!(rig.session.candidate_cursor(), 0);
    rig.ty("<pgdn>");
    assert_eq!(rig.session.candidate_cursor(), n - 1);
    rig.ty("<right>");
    assert_eq!(rig.session.candidate_cursor(), n - 1, "最后一个右移不动");
    // PageUp 回到第一屏同一列
    rig.ty("<pgup>");
    assert_eq!(rig.session.candidate_cursor(), (n - 1) % 6);
    // 不在这一行的数字吃掉、不上屏
    rig.ty("9");
    assert_eq!(rig.committed, "");
    assert_eq!(rig.session.columns(), 6);
    // 鼠标点选这一屏的任意一格
    let grid = rig.page();
    assert!(rig.session.select(rig.engine.as_mut(), 8));
    rig.collect();
    assert_eq!(rig.committed, grid[8]);
}

#[test]
fn grid_collapse_keeps_cursor_off_page() {
    // 插件的行为：网格里移到第一页以外再收起，列表回到第一页、光标留在原处（不在这页），
    // 这时空格选这页第一个
    let mut rig = Rig::new();
    rig.configure("EmojiSuggest=False\n");
    rig.ty("shi");
    let first_page = rig.page();
    rig.ty("<down><down><down><esc>");
    assert_eq!(rig.session.columns(), 0);
    assert_eq!(rig.page(), first_page);
    assert_eq!(rig.session.candidate_cursor(), 0);
    rig.ty("<sp>");
    assert_eq!(rig.committed, first_page[0]);
}

#[test]
fn cursor_then_space() {
    // 323-328
    let mut rig = Rig::new();
    rig.ty("shi<right>");
    let list = rig.page();
    assert_eq!(rig.session.candidate_cursor(), 1);
    rig.ty("<sp>");
    assert_eq!(rig.committed, list[1]);
}

#[test]
fn cursor_wraps_across_pages() {
    // fcitx5 的候选列表：光标在第一个时左移，绕到最后一个并翻到那一页
    let mut rig = Rig::new();
    rig.configure("EmojiSuggest=False\n");
    rig.ty("shi<left>");
    assert!(!rig.session.has_next_page() && rig.session.has_prev_page());
    let last = rig.page();
    assert_eq!(rig.session.candidate_cursor(), last.len() - 1);
    rig.ty("<right>");
    assert!(!rig.session.has_prev_page(), "右移绕回第一页第一个");
    assert_eq!(rig.session.candidate_cursor(), 0);
    // 一页的最后一个右移进下一页
    rig.ty("<right><right><right><right><right><right><right><right><right>");
    assert!(rig.session.has_prev_page());
    assert_eq!(rig.session.candidate_cursor(), 0);
}

#[test]
fn tab_moves_candidate_like_fcitx_defaults() {
    // fcitx5 默认热键：Tab 下一个候选、Shift+Tab 上一个
    let mut rig = Rig::new();
    rig.ty("shi");
    let list = rig.page();
    assert!(rig.key(Key::Tab));
    assert_eq!(rig.session.candidate_cursor(), 1);
    assert!(rig.send(Key::Tab, None, mods::SHIFT));
    assert_eq!(rig.session.candidate_cursor(), 0);
    rig.ty("<tab><sp>");
    assert_eq!(rig.committed, list[1]);
    // 没候选（词库还在加载）时吃掉，不交给应用
    let mut bare = Rig::with(None);
    bare.ty("ni");
    assert!(bare.key(Key::Tab));
    assert_eq!(bare.preedit(), "ni");
    // 没在编辑时交给应用
    rig.fresh();
    assert!(!rig.key(Key::Tab));
}

#[test]
fn backspace_and_idle_keys() {
    // 330-335：退格删光后编辑区关闭
    let mut rig = Rig::new();
    rig.ty("ni<bs><bs>");
    assert_eq!(rig.preedit(), "");
    assert!(rig.page().is_empty());
    assert!(!rig.session.is_composing());
    // 没在编辑时退格、回车、Esc、方向键交给应用
    for key in [
        Key::Backspace,
        Key::Return,
        Key::Escape,
        Key::Left,
        Key::Down,
    ] {
        assert!(!rig.key(key), "{key:?}");
    }
    // 没在编辑时：没收录的符号交给应用，开头的隔音符也是
    for c in ['/', '+', '"', '\'', '-', '=', '_'] {
        assert!(!rig.ch(c), "{c}");
    }
    assert_eq!(rig.passed, "/+\"'-=_");
    assert_eq!(rig.committed, "");
    // 收录的标点出中文标点
    for c in ",.?!;:()[]<>\\~@#$%&*".chars() {
        assert!(rig.ch(c), "{c}");
    }
    assert_eq!(rig.committed, "，。？！；：（）【】《》、～＠＃＄％＆＊");
}

#[test]
fn composing_swallows_cursor_keys() {
    let mut rig = Rig::new();
    rig.ty("ni");
    for key in [Key::Delete, Key::Home, Key::End, Key::Other] {
        assert!(rig.key(key), "{key:?} 编辑中吃掉");
        assert_eq!(rig.preedit(), "ni");
    }
    assert_eq!(rig.committed, "");
    rig.fresh();
    for key in [Key::Delete, Key::Home, Key::End, Key::Other] {
        assert!(!rig.key(key), "{key:?} 没在编辑时交给应用");
    }
}

#[test]
fn modifiers_pass_through() {
    // 374-378、537-538：右 Alt 交给应用，编辑区不动
    let mut rig = Rig::new();
    rig.ty("ni");
    assert!(!rig.key(Key::AltR));
    assert_eq!(rig.preedit(), "ni");
    // Ctrl/Alt/Super 组合键交给应用，编辑区保留
    for m in [mods::CONTROL, mods::ALT, mods::SUPER] {
        assert!(!rig.send(Key::None, Some('a'), m));
        assert!(!rig.send(Key::None, Some(' '), m));
        assert_eq!(rig.preedit(), "ni");
    }
    // 单独的修饰键交给应用
    for key in [Key::ControlL, Key::SuperR, Key::CapsLock, Key::ShiftL] {
        assert!(!rig.event(key, None, 0, false));
    }
    assert_eq!(rig.preedit(), "ni");
    assert_eq!(rig.committed, "");
}

#[test]
fn keypad() {
    // 插件的行为：小键盘数字在编辑中不处理（交给应用），计算器里算数字
    let mut rig = Rig::new();
    rig.ty("shi");
    assert!(!rig.send(Key::None, Some('1'), mods::KEYPAD));
    assert!(!rig.send(Key::None, Some('+'), mods::KEYPAD));
    assert_eq!(
        (rig.committed.as_str(), rig.preedit().as_str()),
        ("", "shi")
    );
    rig.fresh();
    rig.ty("v");
    for c in ['1', '+', '2', '.', '5'] {
        assert!(rig.send(Key::None, Some(c), mods::KEYPAD), "{c}");
    }
    assert_eq!(rig.preedit(), "v1+2.5");
    assert_eq!(rig.page()[0], "3.5");
    // 小键盘回车 = 回车
    assert!(rig.send(Key::Return, None, mods::KEYPAD));
    assert_eq!(rig.committed, "v1+2.5");
    // 没在编辑时都交给应用
    rig.fresh();
    assert!(!rig.send(Key::None, Some('1'), mods::KEYPAD));
    assert!(!rig.send(Key::None, Some('.'), mods::KEYPAD));
    assert!(!rig.send(Key::Return, None, mods::KEYPAD));
}

#[test]
fn im_switch_and_focus_loss() {
    // 342-347：切换输入法时打了一半的字母上屏
    let mut rig = Rig::new();
    rig.ty("nihao");
    rig.session.commit_composition(rig.engine.as_mut());
    rig.collect();
    assert_eq!(rig.committed, "nihao");
    assert!(!rig.session.is_composing());
    // 已选的词 + 剩下的字母
    rig.fresh();
    rig.ty("nihaozhongguo");
    let at = rig.page().iter().position(|t| t == "你好").unwrap();
    rig.ty(&(at + 1).to_string());
    rig.session.commit_composition(rig.engine.as_mut());
    rig.collect();
    assert_eq!(rig.committed, "你好zhongguo");
    // 没在编辑时什么也不做
    rig.fresh();
    rig.session.commit_composition(rig.engine.as_mut());
    assert_eq!(rig.session.take_commit(), None);

    // 349-354：失去焦点不上屏、编辑区清空
    rig.fresh();
    rig.ty("nihao");
    rig.session.reset();
    rig.collect();
    assert_eq!(rig.committed, "");
    assert_eq!(rig.preedit(), "");
    assert!(rig.page().is_empty());
    assert!(!rig.session.is_composing());
}

#[test]
fn many_commits_and_context() {
    // 356-362：连续上屏很多字，上下文按 UTF-8 截断不出错
    let mut rig = Rig::new();
    for _ in 0..30 {
        rig.ty("zhongguo<sp>");
    }
    rig.ty("renmin<sp>");
    assert_eq!(rig.committed, "中国".repeat(30) + "人民");
    // 上下文是最近上屏的 24 字节以内，对齐到字
    let context = rig.engine.as_ref().unwrap().context();
    assert!(
        context.len() <= 24 && context.ends_with("人民"),
        "{context}"
    );
    assert_eq!(context, "中国中国中国人民");
}

#[test]
fn head_word_context() {
    // 已经选了开头的词时，把它接在上文后面给引擎
    let mut rig = Rig::new();
    rig.ty("wo<sp>");
    rig.ty("nihaozhongguo");
    let at = rig.page().iter().position(|t| t == "你好").unwrap();
    rig.ty(&(at + 1).to_string());
    assert_eq!(rig.engine.as_ref().unwrap().context(), "我你好");
    rig.ty("<esc>");
    rig.ty("ni");
    assert_eq!(rig.engine.as_ref().unwrap().context(), "我");
}

#[test]
fn enter_records_raw_letters() {
    // 插件的行为：回车上屏原始字母，并把原始字母记一次选词
    let mut rig = Rig::new();
    rig.ty("nihao<cr>");
    assert_eq!(rig.committed, "nihao");
    assert_eq!(rig.history("nihao"), 1);
    assert_eq!(rig.history("你好"), 0);
    assert!(rig.engine.as_ref().unwrap().user_words().is_empty());
}

#[test]
fn save_request_every_20_learning_commits() {
    let mut rig = Rig::new();
    for _ in 0..19 {
        rig.ty("nihao<sp>");
    }
    assert!(!rig.session.take_save_request());
    // 标点、Esc 不算学习
    rig.ty(",nihao<esc>");
    assert!(!rig.session.take_save_request());
    rig.ty("nihao<sp>");
    assert!(rig.session.take_save_request());
    assert!(!rig.session.take_save_request(), "取一次就清零");
    for _ in 0..19 {
        rig.ty("ni<cr>");
    }
    assert!(!rig.session.take_save_request());
    rig.ty("ni<cr>");
    assert!(rig.session.take_save_request());
    // 没有引擎时不学习，也不要求存盘
    let mut bare = Rig::with(None);
    for _ in 0..25 {
        bare.ty("ni<sp>");
    }
    assert_eq!(bare.committed, "ni".repeat(25));
    assert!(!bare.session.take_save_request());
}

#[test]
fn emoji_suggestions() {
    // 393-402：kaixin 第三个是 😊，选了只上屏 😊
    let mut rig = Rig::new();
    rig.ty("kaixin");
    let kaixin = rig.page();
    assert!(
        kaixin.len() > 2 && kaixin[0] == "开心" && kaixin[2] == "😊",
        "{kaixin:?}"
    );
    rig.ty("3");
    assert_eq!(rig.committed, "😊");
    // 选 emoji 不学习
    assert_eq!(rig.history("😊"), 0);
    assert_eq!(rig.history("开心"), 0);
    // 403-416：句尾是开心时推荐「开心😊」，上屏整句 + emoji
    rig.fresh();
    rig.ty("jintianhaokaixin");
    let sentence = rig.page();
    let at = sentence
        .iter()
        .position(|t| t == "开心😊")
        .unwrap_or_else(|| panic!("{sentence:?}"));
    rig.ty(&(at + 1).to_string());
    assert_eq!(rig.committed, sentence[0].clone() + "😊");
    // 417-423：单字不推荐 emoji
    rig.fresh();
    rig.ty("shi");
    assert!(!rig.page().contains(&"✅".to_string()), "{:?}", rig.page());
}

#[test]
fn keycap_suggestions() {
    // 475-490：ba -> 吧 八 8️⃣ 把，选了只上屏 8️⃣
    let mut rig = Rig::new();
    rig.ty("ba");
    let ba = rig.page();
    let at = ba
        .iter()
        .position(|t| t == "8️⃣")
        .unwrap_or_else(|| panic!("{ba:?}"));
    assert!(at > 0 && ba[at - 1] == "八", "{ba:?}");
    rig.ty(&(at + 1).to_string());
    assert_eq!(rig.committed, "8️⃣");
    // 491-503：十 后面是 🔟
    rig.fresh();
    rig.ty("shi");
    let shi = rig.page();
    let ten = shi
        .iter()
        .position(|t| t == "十")
        .unwrap_or_else(|| panic!("{shi:?}"));
    assert_eq!(shi[ten + 1], "🔟");
    assert_eq!(shi.iter().filter(|t| *t == "🔟").count(), 1);
    // 504-510：拼音没打完的「八」不跟 8️⃣
    rig.fresh();
    rig.ty("bage");
    assert!(!rig.page().contains(&"8️⃣".to_string()), "{:?}", rig.page());
}

#[test]
fn config_reload() {
    // 512-543：竖排、每页 5 个、关掉 emoji 推荐、自定义词
    let mut rig = Rig::new();
    rig.configure("CandidateLayout=Vertical\nPageSize=5\nVoice=False\nEmojiSuggest=False\n");
    rig.custom("# 测试\nni hao\t妮好\nwo de you xiang\t我的邮箱\n");
    rig.ty("nihao");
    assert!(rig.session.vertical(), "设置竖排后候选竖排");
    let vertical = rig.page();
    assert_eq!(vertical.len(), 5, "{vertical:?}");
    assert_eq!(vertical[0], "妮好");
    assert_eq!(rig.labels(), ["1", "2", "3", "4", "5"]);
    // 竖排时下键移动光标（不翻页、不展开）
    rig.ty("<down>");
    assert_eq!(rig.session.columns(), 0);
    assert_eq!(rig.session.candidate_cursor(), 1);
    rig.ty("<sp>");
    assert_eq!(rig.committed, vertical[1]);
    rig.fresh();
    assert!(!rig.key(Key::AltR), "右 Alt 交给应用");
    rig.ty("wdyx<sp>");
    assert_eq!(rig.committed, "我的邮箱");
    rig.fresh();
    rig.ty("kaixin");
    assert!(
        !rig.page().iter().any(|t| t.contains("😊")),
        "{:?}",
        rig.page()
    );

    // 竖排时左右键翻页
    rig.fresh();
    rig.ty("shi");
    let first = rig.page();
    rig.ty("<right>");
    assert_ne!(rig.page(), first);
    rig.ty("<left>");
    assert_eq!(rig.page(), first);

    // 545-555：设置文本清空后回到默认
    rig.configure("");
    rig.custom("");
    rig.fresh();
    rig.ty("nihao");
    assert!(!rig.session.vertical());
    assert_eq!(rig.page()[0], "你好");
    rig.fresh();
    rig.ty("shi");
    assert_eq!(rig.page().len(), 9);
}

#[test]
fn configure_keeps_composition() {
    let mut rig = Rig::new();
    rig.ty("shi");
    assert_eq!(rig.session.configure("PageSize=5\n"), 0);
    assert_eq!(rig.preedit(), "shi", "改设置不清空编辑区");
    rig.session.refresh(rig.engine.as_mut());
    assert_eq!(rig.page().len(), 5);
    // 认不出的行计数，其他键照用
    assert_eq!(
        rig.session
            .configure("PageSize=7\nNoSuchKey=1\nPageSize=99\n"),
        2
    );
    rig.session.refresh(rig.engine.as_mut());
    assert_eq!(rig.page().len(), 7);
}

#[test]
fn shift_toggle() {
    // 557-576：单按右 Shift 切英文
    let mut rig = Rig::new();
    assert!(rig.tap_shift(Key::ShiftR, 50), "单按右 Shift 被接住");
    assert_eq!(rig.session.mode(), Mode::Ascii);
    rig.ty("nihao,");
    assert_eq!(rig.committed, "");
    assert_eq!(rig.passed, "nihao,");
    assert_eq!(rig.preedit(), "");
    rig.tap_shift(Key::ShiftR, 50);
    assert_eq!(rig.session.mode(), Mode::Chinese);
    rig.fresh();
    rig.ty("nihao<sp>");
    assert_eq!(rig.committed, "你好");
    rig.fresh();
    rig.ty("nih");
    rig.tap_shift(Key::ShiftR, 50);
    assert_eq!(rig.committed, "nih", "打到一半切英文，字母原样上屏");
    assert_eq!(rig.preedit(), "");
    rig.tap_shift(Key::ShiftR, 50);
    assert_eq!(rig.session.mode(), Mode::Chinese);

    // 578-587：Shift 按着打了别的键就不算单按
    rig.fresh();
    rig.event(Key::ShiftR, None, 0, false);
    rig.send(Key::None, Some('A'), mods::SHIFT);
    assert!(!rig.event(Key::ShiftR, None, mods::SHIFT, true));
    rig.ty("<esc>nihao<sp>");
    assert_eq!(rig.committed, "你好");
    assert_eq!(rig.session.mode(), Mode::Chinese);
}

#[test]
fn shift_toggle_rules() {
    let mut rig = Rig::new();
    // 按太久（超过 0.5 秒）不切
    assert!(!rig.tap_shift(Key::ShiftL, 600));
    assert_eq!(rig.session.mode(), Mode::Chinese);
    // 按着 Ctrl 时按 Shift 不算
    rig.event(Key::ShiftL, None, mods::CONTROL, false);
    assert!(!rig.event(Key::ShiftL, None, mods::CONTROL | mods::SHIFT, true));
    // 左按右放不算；之后再松开左 Shift 还算（插件也这样）
    rig.event(Key::ShiftL, None, 0, false);
    assert!(!rig.event(Key::ShiftR, None, mods::SHIFT, true));
    assert_eq!(rig.session.mode(), Mode::Chinese);
    assert!(rig.event(Key::ShiftL, None, mods::SHIFT, true));
    assert_eq!(rig.session.mode(), Mode::Ascii);
    assert!(rig.tap_shift(Key::ShiftL, 50));
    assert_eq!(rig.session.mode(), Mode::Chinese);
    // 只认右 Shift
    rig.configure("ShiftToggle=Right\n");
    assert!(!rig.tap_shift(Key::ShiftL, 50));
    assert!(rig.tap_shift(Key::ShiftR, 50));
    assert_eq!(rig.session.mode(), Mode::Ascii);
    // 关掉时回到中文、不再切换
    rig.configure("ShiftToggle=Off\n");
    assert_eq!(rig.session.mode(), Mode::Chinese);
    assert!(!rig.tap_shift(Key::ShiftR, 50));
    rig.configure("ShiftToggle=Left\n");
    assert!(rig.tap_shift(Key::ShiftL, 50));
    assert_eq!(rig.session.mode(), Mode::Ascii);
    // 英文模式下命名键也交给应用
    assert!(!rig.key(Key::Backspace));
    assert!(!rig.ch('1'));
}

#[test]
fn shuangpin_semicolon() {
    // 613-631：微软双拼分号当 ing；成对之后的分号还是标点
    let mut rig = Rig::new();
    rig.configure("Shuangpin=True\nShuangpinScheme=Microsoft\n");
    rig.ty("x;");
    assert!(
        rig.preedit().contains("xing") && rig.committed.is_empty(),
        "{}",
        rig.preedit()
    );
    rig.ty("<sp>");
    assert_eq!(rig.committed, "行");
    // 双拼时计算器不开：v 后面的数字按选词处理
    rig.fresh();
    rig.ty("vsgo1");
    assert_eq!(rig.committed, "中国");
    // 自然码不用分号：分号是标点
    rig.configure("Shuangpin=True\nShuangpinScheme=Ziranma\n");
    rig.fresh();
    rig.ty("x;");
    assert!(rig.committed.ends_with('；'), "{}", rig.committed);
}

#[test]
fn datetime_candidates() {
    // 633-655：日期时间候选只认全拼，格式从第二个开始（时间来自按键事件）
    let mut rig = Rig::new();
    rig.ty("riqi");
    let items = rig.page();
    assert_eq!(
        items[..3],
        ["日期", "2026年11月10日", "2026-11-10"],
        "{items:?}"
    );
    rig.ty("2");
    assert_eq!(rig.committed, "2026年11月10日");
    rig.fresh();
    rig.ty("shijian");
    assert_eq!(rig.page()[..3], ["时间", "09:05", "09:05:01"]);
    rig.fresh();
    rig.ty("xingqi");
    assert_eq!(rig.page()[..3], ["星期", "星期二", "周二"]);
    rig.fresh();
    rig.ty("sj");
    assert!(
        !rig.page().iter().any(|t| t.contains(':')),
        "{:?}",
        rig.page()
    );
    // 时区来自事件：西五区还是 11 月 9 日 20 点
    rig.fresh();
    rig.utc_offset = -5 * 3600;
    rig.ty("riqi");
    assert_eq!(rig.page()[1], "2026年11月9日");
    // 关掉后不出
    rig.fresh();
    rig.configure("DateTime=False\n");
    rig.ty("riqi");
    assert!(
        !rig.page().iter().any(|t| t.contains('年')),
        "{:?}",
        rig.page()
    );
}

#[test]
fn calculator() {
    // 657-672
    let mut rig = Rig::new();
    rig.ty("v(1.5-0.5)/4");
    assert_eq!(rig.committed, "", "算式里的 . - 不翻页也不上屏");
    rig.fresh();
    rig.ty("very");
    assert!(rig.page().contains(&"very".to_string()), "{:?}", rig.page());
    // 关掉计算器：v1 的 1 按选词处理，之后的 + 没在编辑，交给应用
    rig.fresh();
    rig.configure("Calculator=False\n");
    rig.ty("v");
    let v = rig.page();
    assert!(!v.is_empty(), "{v:?}");
    rig.ty("1+");
    assert!(!rig.session.is_composing());
    assert_eq!(
        (rig.committed.as_str(), rig.passed.as_str()),
        (v[0].as_str(), "+")
    );
}

#[test]
fn fuzzy_groups() {
    // 674-688：n=l 默认关，打开后 liunai 能出牛奶
    let mut rig = Rig::new();
    rig.configure("Fuzzy=True\nTypoCorrection=False\n");
    rig.ty("liunai");
    assert!(
        !rig.page().contains(&"牛奶".to_string()),
        "{:?}",
        rig.page()
    );
    rig.fresh();
    rig.configure("Fuzzy=True\nFuzzyNL=True\nTypoCorrection=False\n");
    rig.ty("liunai");
    assert!(rig.page().contains(&"牛奶".to_string()), "{:?}", rig.page());
}

#[test]
fn custom_codes_and_newline() {
    // 690-706：自定义编码、\n 换行
    let mut rig = Rig::new();
    rig.configure("EmojiSuggest=False\n");
    rig.custom("mail\tme@example.com\ndi zhi\t四川省\\n宜宾市\n");
    rig.ty("mail<sp>");
    assert_eq!(rig.committed, "me@example.com");
    rig.fresh();
    rig.ty("dizhi");
    assert_eq!(rig.page()[0], "四川省⏎宜宾市");
    rig.ty("<sp>");
    assert_eq!(rig.committed, "四川省\n宜宾市", "上屏真的换行");
    // 带换行的词不学习
    assert_eq!(rig.history("四川省\n宜宾市"), 0);
}

#[test]
fn select_keys() {
    // 708-738
    let mut rig = Rig::new();
    rig.configure("SelectKeys=Semicolon\nEmojiSuggest=False\n");
    rig.ty("shi");
    let items = rig.page();
    rig.ty(";");
    assert_eq!(rig.committed, items[1], "分号选第二个");
    rig.fresh();
    rig.configure("SelectKeys=SemicolonQuote\nEmojiSuggest=False\n");
    rig.ty("shi");
    let items = rig.page();
    rig.ty("'");
    assert_eq!(rig.committed, items[2], "引号选第三个");
    // 翻页后按当前页选
    rig.fresh();
    rig.ty("shi<pgdn>");
    let second = rig.page();
    rig.ty(";");
    assert_eq!(rig.committed, second[1]);
    // 微软双拼的分号是 ing，不拿来选词
    rig.fresh();
    rig.configure("SelectKeys=Semicolon\nShuangpin=True\nShuangpinScheme=Microsoft\n");
    rig.ty("x;");
    assert!(rig.preedit().contains("xing"), "{}", rig.preedit());
}

#[test]
fn digits_out_of_range_are_swallowed() {
    let mut rig = Rig::new();
    rig.ty("nihao");
    let n = rig.page().len();
    assert!(n < 9, "{:?}", rig.page());
    assert!(rig.ch(char::from_digit(n as u32 + 1, 10).unwrap()));
    assert!(rig.ch('0'));
    assert_eq!(rig.committed, "");
    assert_eq!(rig.preedit(), "ni'hao");
}

#[test]
fn select_api_like_mouse_click() {
    let mut rig = Rig::new();
    rig.ty("shi<pgdn>");
    let second = rig.page();
    assert!(!rig.session.select(rig.engine.as_mut(), second.len()));
    assert!(rig.session.select(rig.engine.as_mut(), 2));
    rig.collect();
    assert_eq!(rig.committed, second[2]);
    assert!(
        !rig.session.select(rig.engine.as_mut(), 0),
        "没在编辑时没有候选"
    );
}

#[test]
fn without_engine() {
    // 词库还在加载：字母照样进编辑区，只是没有候选；空格、回车上屏原始字母
    let mut rig = Rig::with(None);
    rig.ty("nihao");
    assert_eq!(rig.preedit(), "nihao");
    assert_eq!(rig.session.candidate_count(), 0);
    assert_eq!(rig.session.candidate_text(0), None);
    assert_eq!(rig.session.candidate_label(0), None);
    assert_eq!(rig.session.candidate_cursor(), 0);
    assert!(!rig.session.has_next_page());
    rig.ty("<sp>");
    assert_eq!(rig.committed, "nihao");
    rig.fresh();
    rig.ty("nihao,");
    assert_eq!(rig.committed, "nihao，");
    // 词库好了：refresh 后有候选
    rig.fresh();
    rig.ty("nihao");
    rig.engine = Some(engine());
    rig.session.refresh(rig.engine.as_mut());
    assert_eq!(rig.page()[0], "你好");
    assert_eq!(rig.preedit(), "ni'hao");
    // 没在编辑时 refresh 什么也不做
    rig.fresh();
    rig.session.refresh(rig.engine.as_mut());
    assert!(rig.page().is_empty());
    assert_eq!(rig.session.take_commit(), None);
}

#[test]
fn outputs_when_idle() {
    let rig = Rig::new();
    let s = &rig.session;
    assert!(!s.is_composing());
    assert_eq!(s.preedit(), "");
    assert_eq!((s.preedit_plain_len(), s.preedit_cursor()), (0, 0));
    assert_eq!(s.aux(), "");
    assert_eq!(s.candidate_count(), 0);
    assert_eq!(s.columns(), 0);
    assert_eq!(s.mode(), Mode::Chinese);
}

#[test]
fn releases_are_ignored() {
    let mut rig = Rig::new();
    assert!(!rig.event(Key::None, Some('n'), 0, true));
    assert!(!rig.session.is_composing());
}

#[test]
fn key_codes_match_the_header() {
    // C ABI 的 WOWTYPE_KEY_* / WOWTYPE_MOD_* 数值
    let header = include_str!("../include/wowtype_core.h");
    let value = |name: &str| -> u32 {
        let at = header
            .find(&format!("{name} = "))
            .unwrap_or_else(|| panic!("{name}"));
        let rest = &header[at + name.len() + 3..];
        rest[..rest.find(|c: char| !c.is_ascii_digit()).unwrap()]
            .parse()
            .unwrap()
    };
    for (name, key) in [
        ("WOWTYPE_KEY_NONE", Key::None),
        ("WOWTYPE_KEY_BACKSPACE", Key::Backspace),
        ("WOWTYPE_KEY_RETURN", Key::Return),
        ("WOWTYPE_KEY_ESCAPE", Key::Escape),
        ("WOWTYPE_KEY_TAB", Key::Tab),
        ("WOWTYPE_KEY_DELETE", Key::Delete),
        ("WOWTYPE_KEY_LEFT", Key::Left),
        ("WOWTYPE_KEY_RIGHT", Key::Right),
        ("WOWTYPE_KEY_UP", Key::Up),
        ("WOWTYPE_KEY_DOWN", Key::Down),
        ("WOWTYPE_KEY_HOME", Key::Home),
        ("WOWTYPE_KEY_END", Key::End),
        ("WOWTYPE_KEY_PAGE_UP", Key::PageUp),
        ("WOWTYPE_KEY_PAGE_DOWN", Key::PageDown),
        ("WOWTYPE_KEY_SHIFT_L", Key::ShiftL),
        ("WOWTYPE_KEY_SHIFT_R", Key::ShiftR),
        ("WOWTYPE_KEY_CONTROL_L", Key::ControlL),
        ("WOWTYPE_KEY_CONTROL_R", Key::ControlR),
        ("WOWTYPE_KEY_ALT_L", Key::AltL),
        ("WOWTYPE_KEY_ALT_R", Key::AltR),
        ("WOWTYPE_KEY_SUPER_L", Key::SuperL),
        ("WOWTYPE_KEY_SUPER_R", Key::SuperR),
        ("WOWTYPE_KEY_CAPS_LOCK", Key::CapsLock),
        ("WOWTYPE_KEY_OTHER", Key::Other),
    ] {
        assert_eq!(value(name), key.code(), "{name}");
        assert_eq!(Key::from_code(key.code()), Some(key));
    }
    assert_eq!(Key::from_code(14), None);
    for (name, bit) in [
        ("WOWTYPE_MOD_SHIFT", mods::SHIFT),
        ("WOWTYPE_MOD_CONTROL", mods::CONTROL),
        ("WOWTYPE_MOD_ALT", mods::ALT),
        ("WOWTYPE_MOD_SUPER", mods::SUPER),
        ("WOWTYPE_MOD_KEYPAD", mods::KEYPAD),
        ("WOWTYPE_MOD_CAPS_LOCK", mods::CAPS_LOCK),
    ] {
        assert_eq!(value(name), bit, "{name}");
    }
    assert_eq!(value("WOWTYPE_MODE_CHINESE"), Mode::Chinese as u32);
    assert_eq!(value("WOWTYPE_MODE_ASCII"), Mode::Ascii as u32);
}
