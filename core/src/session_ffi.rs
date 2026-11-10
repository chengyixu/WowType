//! 输入会话的 C ABI（wowtype_core.h 的「输入会话」一节）。只是薄薄一层，行为都在 session.rs。
//!
//! 和 ffi.rs 一样：NULL 指针返回 0 / NULL / -1（并设 wowtype_last_error），panic 不越过边界。
//! 返回的字符串由会话保管，在同一会话下一次调用 wowtype_session_* 前有效
//! （实际上只读的调用不管调多少次都不会让之前取的失效，改状态的调用才会）。
//!
//! 同一线程上建的会话是一组（共用中英文模式、上文和学习计数，见 session.rs 的 SessionGroup）：
//! 会话本来就只在一个线程上用，前端每个输入框一个会话，这样就和插件一样全局生效。

#![allow(clippy::missing_safety_doc)]

use std::cell::RefCell;
use std::ffi::{c_char, c_int, CString};
use std::panic::{self, AssertUnwindSafe};
use std::ptr;

use crate::ffi::{cstr_to_str, set_last_error, WowtypeEngineHandle};
use crate::session::{Key, KeyEvent, Session, SessionGroup};
use crate::Engine;

/// 和头文件里的 WowtypeKeyEvent 一致。
#[repr(C)]
pub struct WowtypeKeyEvent {
    pub struct_size: u32,
    pub key: u32,
    pub unicode: u32,
    pub modifiers: u32,
    pub is_release: c_int,
    pub utc_offset_seconds: c_int,
    pub time_ms: i64,
}

/// 会话句柄：会话本身 + 交给 C 的字符串。
pub struct WowtypeSession {
    session: Session,
    strings: RefCell<Vec<CString>>,
}

impl WowtypeSession {
    /// 存一份 C 字符串，返回的指针到下一次改状态的调用前都有效。
    /// 已经存过同样的字符串就给原来那个指针：状态不变时能读到的字符串只有那么几十个
    /// （编辑区、提示、这一屏的候选和选择键），所以只读调用再多也不会越攒越多，不用清掉。
    fn keep(&self, text: &str) -> *const c_char {
        let text = text.replace('\0', " ");
        let mut strings = self.strings.borrow_mut();
        if let Some(found) = strings.iter().find(|s| s.as_bytes() == text.as_bytes()) {
            return found.as_ptr();
        }
        strings.push(CString::new(text).unwrap_or_default());
        strings.last().map_or(ptr::null(), |s| s.as_ptr())
    }

    /// 改状态的调用开始时丢掉之前交出去的字符串。
    fn forget(&mut self) {
        self.strings.get_mut().clear();
    }
}

thread_local! {
    /// 这个线程上建的会话所在的组。
    static GROUP: SessionGroup = SessionGroup::default();
}

/// panic 不越过 C 边界：出 panic 时记下错误，返回 fallback。
fn guard<T>(what: &str, fallback: T, f: impl FnOnce() -> T) -> T {
    match panic::catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(_) => {
            set_last_error(format!("Rust panic in {what}"));
            fallback
        }
    }
}

unsafe fn session_mut<'a>(session: *mut WowtypeSession) -> Option<&'a mut WowtypeSession> {
    let found = unsafe { session.as_mut() };
    if found.is_none() {
        set_last_error("session is null");
    }
    found
}

unsafe fn session_ref<'a>(session: *const WowtypeSession) -> Option<&'a WowtypeSession> {
    unsafe { session.as_ref() }
}

unsafe fn engine_mut<'a>(engine: *mut WowtypeEngineHandle) -> Option<&'a mut Engine> {
    unsafe { engine.as_mut() }.map(|handle| &mut handle.engine)
}

fn flag(on: bool) -> c_int {
    c_int::from(on)
}

#[no_mangle]
pub extern "C" fn wowtype_session_new() -> *mut WowtypeSession {
    guard("wowtype_session_new", ptr::null_mut(), || {
        Box::into_raw(Box::new(WowtypeSession {
            session: GROUP.with(Session::in_group),
            strings: RefCell::new(Vec::new()),
        }))
    })
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_free(session: *mut WowtypeSession) {
    if !session.is_null() {
        guard("wowtype_session_free", (), || {
            drop(unsafe { Box::from_raw(session) });
        });
    }
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_configure(
    session: *mut WowtypeSession,
    ini_text: *const c_char,
) -> c_int {
    let Some(handle) = (unsafe { session_mut(session) }) else {
        return -1;
    };
    let Some(text) = (unsafe { cstr_to_str(ini_text) }) else {
        set_last_error("ini_text is null or not utf8");
        return -1;
    };
    guard("wowtype_session_configure", -1, || {
        handle.forget();
        c_int::try_from(handle.session.configure(text)).unwrap_or(c_int::MAX)
    })
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_process_key(
    session: *mut WowtypeSession,
    engine: *mut WowtypeEngineHandle,
    event: *const WowtypeKeyEvent,
) -> c_int {
    let Some(handle) = (unsafe { session_mut(session) }) else {
        return 0;
    };
    let Some(event) = (unsafe { event.as_ref() }) else {
        set_last_error("event is null");
        return 0;
    };
    if (event.struct_size as usize) < std::mem::size_of::<WowtypeKeyEvent>() {
        set_last_error("event.struct_size is smaller than WowtypeKeyEvent");
        return 0;
    }
    let ev = KeyEvent {
        // 不认识的具名键（以后的头文件加的）当成别的功能键
        key: Key::from_code(event.key).unwrap_or(Key::Other),
        unicode: char::from_u32(event.unicode).filter(|c| *c != '\0'),
        modifiers: event.modifiers,
        is_release: event.is_release != 0,
        utc_offset_seconds: event.utc_offset_seconds,
        time_ms: event.time_ms,
    };
    let engine = unsafe { engine_mut(engine) };
    guard("wowtype_session_process_key", 0, || {
        handle.forget();
        flag(handle.session.process_key(engine, &ev))
    })
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_refresh(
    session: *mut WowtypeSession,
    engine: *mut WowtypeEngineHandle,
) {
    let Some(handle) = (unsafe { session_mut(session) }) else {
        return;
    };
    let engine = unsafe { engine_mut(engine) };
    guard("wowtype_session_refresh", (), || {
        handle.forget();
        handle.session.refresh(engine);
    });
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_select(
    session: *mut WowtypeSession,
    engine: *mut WowtypeEngineHandle,
    index: usize,
) -> c_int {
    let Some(handle) = (unsafe { session_mut(session) }) else {
        return 0;
    };
    let engine = unsafe { engine_mut(engine) };
    guard("wowtype_session_select", 0, || {
        handle.forget();
        flag(handle.session.select(engine, index))
    })
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_page(session: *mut WowtypeSession, delta: c_int) -> c_int {
    let Some(handle) = (unsafe { session_mut(session) }) else {
        return 0;
    };
    guard("wowtype_session_page", 0, || {
        handle.forget();
        flag(handle.session.page(delta))
    })
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_reset(session: *mut WowtypeSession) {
    let Some(handle) = (unsafe { session_mut(session) }) else {
        return;
    };
    guard("wowtype_session_reset", (), || {
        handle.forget();
        handle.session.reset();
    });
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_commit_composition(
    session: *mut WowtypeSession,
    engine: *mut WowtypeEngineHandle,
) {
    let Some(handle) = (unsafe { session_mut(session) }) else {
        return;
    };
    let engine = unsafe { engine_mut(engine) };
    guard("wowtype_session_commit_composition", (), || {
        handle.forget();
        handle.session.commit_composition(engine);
    });
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_take_commit(
    session: *mut WowtypeSession,
) -> *const c_char {
    let Some(handle) = (unsafe { session_mut(session) }) else {
        return ptr::null();
    };
    guard("wowtype_session_take_commit", ptr::null(), || {
        handle.forget();
        match handle.session.take_commit() {
            Some(text) => handle.keep(&text),
            None => ptr::null(),
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_is_composing(session: *const WowtypeSession) -> c_int {
    unsafe { session_ref(session) }.map_or(0, |h| flag(h.session.is_composing()))
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_preedit(session: *const WowtypeSession) -> *const c_char {
    unsafe { session_ref(session) }.map_or(ptr::null(), |h| h.keep(h.session.preedit()))
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_preedit_plain_len(
    session: *const WowtypeSession,
) -> usize {
    unsafe { session_ref(session) }.map_or(0, |h| h.session.preedit_plain_len())
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_preedit_cursor(session: *const WowtypeSession) -> usize {
    unsafe { session_ref(session) }.map_or(0, |h| h.session.preedit_cursor())
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_aux(session: *const WowtypeSession) -> *const c_char {
    unsafe { session_ref(session) }.map_or(ptr::null(), |h| h.keep(h.session.aux()))
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_candidate_count(session: *const WowtypeSession) -> usize {
    unsafe { session_ref(session) }.map_or(0, |h| h.session.candidate_count())
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_candidate_text(
    session: *const WowtypeSession,
    index: usize,
) -> *const c_char {
    unsafe { session_ref(session) }
        .and_then(|h| h.session.candidate_text(index).map(|t| h.keep(t)))
        .unwrap_or(ptr::null())
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_candidate_label(
    session: *const WowtypeSession,
    index: usize,
) -> *const c_char {
    unsafe { session_ref(session) }
        .and_then(|h| h.session.candidate_label(index).map(|t| h.keep(t)))
        .unwrap_or(ptr::null())
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_candidate_cursor(session: *const WowtypeSession) -> usize {
    unsafe { session_ref(session) }.map_or(0, |h| h.session.candidate_cursor())
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_columns(session: *const WowtypeSession) -> usize {
    unsafe { session_ref(session) }.map_or(0, |h| h.session.columns())
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_vertical(session: *const WowtypeSession) -> c_int {
    unsafe { session_ref(session) }.map_or(0, |h| flag(h.session.vertical()))
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_has_prev_page(session: *const WowtypeSession) -> c_int {
    unsafe { session_ref(session) }.map_or(0, |h| flag(h.session.has_prev_page()))
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_has_next_page(session: *const WowtypeSession) -> c_int {
    unsafe { session_ref(session) }.map_or(0, |h| flag(h.session.has_next_page()))
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_mode(session: *const WowtypeSession) -> c_int {
    unsafe { session_ref(session) }.map_or(-1, |h| h.session.mode() as c_int)
}

#[no_mangle]
pub unsafe extern "C" fn wowtype_session_take_save_request(session: *mut WowtypeSession) -> c_int {
    let Some(handle) = (unsafe { session_mut(session) }) else {
        return 0;
    };
    guard("wowtype_session_take_save_request", 0, || {
        handle.forget();
        flag(handle.session.take_save_request())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    fn press(c: char) -> WowtypeKeyEvent {
        WowtypeKeyEvent {
            struct_size: std::mem::size_of::<WowtypeKeyEvent>() as u32,
            key: 0,
            unicode: c as u32,
            modifiers: 0,
            is_release: 0,
            utc_offset_seconds: 0,
            time_ms: 1,
        }
    }

    fn text(p: *const c_char) -> &'static str {
        assert!(!p.is_null());
        unsafe { CStr::from_ptr(p) }.to_str().unwrap()
    }

    #[test]
    fn layout_matches_the_header() {
        // 头文件：6 个 32 位字段后面跟一个 long long
        assert_eq!(std::mem::size_of::<WowtypeKeyEvent>(), 32);
        let ev = press('a');
        let base = &ev as *const WowtypeKeyEvent as usize;
        assert_eq!(&ev.time_ms as *const i64 as usize - base, 24);
    }

    #[test]
    fn strings_stay_valid_across_reads() {
        let s = wowtype_session_new();
        for c in "ni".chars() {
            assert_eq!(
                unsafe { wowtype_session_process_key(s, ptr::null_mut(), &press(c)) },
                1
            );
        }
        let preedit = unsafe { wowtype_session_preedit(s) };
        let aux = unsafe { wowtype_session_aux(s) };
        // 只读调用不让之前取的字符串失效
        for _ in 0..10 {
            unsafe { wowtype_session_preedit(s) };
        }
        assert_eq!((text(preedit), text(aux)), ("ni", ""));
        // 没有引擎时空格上屏原始字母
        assert_eq!(
            unsafe { wowtype_session_process_key(s, ptr::null_mut(), &press(' ')) },
            1
        );
        assert_eq!(text(unsafe { wowtype_session_take_commit(s) }), "ni");
        assert!(unsafe { wowtype_session_take_commit(s) }.is_null());
        unsafe { wowtype_session_free(s) };
    }

    #[test]
    fn many_reads_keep_earlier_strings() {
        // 前端不按键、一直重画（一屏候选约 72 个字符串）：之前取的指针不能被释放
        let s = wowtype_session_new();
        unsafe { wowtype_session_process_key(s, ptr::null_mut(), &press('n')) };
        let preedit = unsafe { wowtype_session_preedit(s) };
        // 状态没变时同样的读取给同一个指针（前一个还活着，所以不会碰巧是同一块内存）
        assert_eq!(unsafe { wowtype_session_preedit(s) }, preedit);
        for _ in 0..3000 {
            unsafe {
                wowtype_session_aux(s);
                wowtype_session_preedit(s);
            }
        }
        // 存的字符串不随读取次数变多（只有 "n" 和 ""），也就不用清掉
        assert_eq!(unsafe { (*s).strings.borrow().len() }, 2);
        assert_eq!(unsafe { wowtype_session_preedit(s) }, preedit);
        assert_eq!(text(preedit), "n");
        unsafe { wowtype_session_free(s) };
    }

    fn shift_r(release: bool) -> WowtypeKeyEvent {
        WowtypeKeyEvent {
            key: 21,
            unicode: 0,
            modifiers: if release { 1 } else { 0 },
            is_release: c_int::from(release),
            ..press('\0')
        }
    }

    #[test]
    fn sessions_on_one_thread_share_the_mode() {
        // 照插件：中英文模式全局生效。前端每个输入框一个会话，同一线程上的会话共用它
        let a = wowtype_session_new();
        let b = wowtype_session_new();
        unsafe {
            wowtype_session_process_key(a, ptr::null_mut(), &shift_r(false));
            assert_eq!(
                wowtype_session_process_key(a, ptr::null_mut(), &shift_r(true)),
                1
            );
            assert_eq!(wowtype_session_mode(a), 1);
            assert_eq!(wowtype_session_mode(b), 1, "另一个会话也是英文");
            assert_eq!(
                wowtype_session_process_key(b, ptr::null_mut(), &press('n')),
                0
            );
        }
        // 别的线程上的会话是另一组
        let elsewhere = std::thread::spawn(|| {
            let c = wowtype_session_new();
            let mode = unsafe { wowtype_session_mode(c) };
            unsafe { wowtype_session_free(c) };
            mode
        });
        assert_eq!(elsewhere.join().unwrap(), 0);
        unsafe {
            wowtype_session_free(a);
            // 先建的会话释放了，后建的照样用这一组的状态
            assert_eq!(wowtype_session_mode(b), 1);
            let c = wowtype_session_new();
            assert_eq!(wowtype_session_mode(c), 1);
            wowtype_session_free(b);
            wowtype_session_free(c);
        }
    }

    #[test]
    fn null_and_bad_events() {
        let s = wowtype_session_new();
        let mut ev = press('n');
        ev.struct_size = 8;
        assert_eq!(
            unsafe { wowtype_session_process_key(s, ptr::null_mut(), &ev) },
            0
        );
        assert_eq!(unsafe { wowtype_session_is_composing(s) }, 0);
        // 不认识的具名键当成别的功能键：没在编辑时交给应用
        let mut ev = press('\0');
        ev.key = 77;
        assert_eq!(
            unsafe { wowtype_session_process_key(s, ptr::null_mut(), &ev) },
            0
        );
        assert_eq!(
            unsafe { wowtype_session_process_key(s, ptr::null_mut(), ptr::null()) },
            0
        );
        let empty = CString::new("").unwrap();
        assert_eq!(
            unsafe { wowtype_session_configure(ptr::null_mut(), empty.as_ptr()) },
            -1
        );
        assert_eq!(unsafe { wowtype_session_configure(s, ptr::null()) }, -1);
        assert_eq!(unsafe { wowtype_session_mode(ptr::null()) }, -1);
        assert!(unsafe { wowtype_session_preedit(ptr::null()) }.is_null());
        assert!(unsafe { wowtype_session_candidate_text(s, 0) }.is_null());
        assert_eq!(unsafe { wowtype_session_page(ptr::null_mut(), 1) }, 0);
        unsafe {
            wowtype_session_reset(ptr::null_mut());
            wowtype_session_refresh(ptr::null_mut(), ptr::null_mut());
            wowtype_session_commit_composition(ptr::null_mut(), ptr::null_mut());
            wowtype_session_free(ptr::null_mut());
            wowtype_session_free(s);
        }
    }
}
