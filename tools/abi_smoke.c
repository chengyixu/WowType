/* WowType C ABI 冒烟测试：模拟 fcitx5 插件的调用序列。
 *
 * 覆盖：引擎创建 -> 输入 -> 取候选 -> 记录选词 -> 设置上下文 -> 预测下一词
 *      -> 用户词增删 -> 用户词库持久化；
 *      设置文本（默认值、engine_configure）、快照只写进缓存目录、输入会话（macOS 前端的调用序列）。
 */
#include <dirent.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
#include "wowtype_core.h"

static int failures = 0;

static void check(int cond, const char *what) {
    if (!cond) {
        failures++;
        printf("FAIL %s", what);
        putchar(10);
    } else {
        printf("ok   %s", what);
        putchar(10);
    }
}

static int has_candidate(WowtypeCandidates *c, const char *text) {
    size_t n = wowtype_candidates_len(c);
    for (size_t i = 0; i < n; i++) {
        const char *t = wowtype_candidates_text(c, i);
        if (t && strcmp(t, text) == 0) return 1;
    }
    return 0;
}

/* ---- 输入会话 ---- */

static long long session_now = 1794272701000LL; /* 2026-11-10 09:05:01 东八区 */

/* 按下再松开，返回按下时会话接没接 */
static int session_key(WowtypeSession *s, WowtypeEngineHandle *e, unsigned key, unsigned ch,
                       unsigned mods) {
    WowtypeKeyEvent ev;
    memset(&ev, 0, sizeof ev);
    ev.struct_size = sizeof ev;
    ev.key = key;
    ev.unicode = ch;
    ev.modifiers = mods;
    ev.utc_offset_seconds = 8 * 3600;
    ev.time_ms = (session_now += 10);
    int handled = wowtype_session_process_key(s, e, &ev);
    ev.is_release = 1;
    ev.time_ms = (session_now += 10);
    wowtype_session_process_key(s, e, &ev);
    return handled;
}

static void session_type(WowtypeSession *s, WowtypeEngineHandle *e, const char *keys) {
    for (const char *p = keys; *p; p++) {
        unsigned mods = (*p >= 'A' && *p <= 'Z') ? WOWTYPE_MOD_SHIFT : 0;
        session_key(s, e, WOWTYPE_KEY_NONE, (unsigned char)*p, mods);
    }
}

/* 取上屏文字并比较；want 为 NULL 表示应当没有 */
static int commit_is(WowtypeSession *s, const char *want) {
    const char *got = wowtype_session_take_commit(s);
    if (!want) return got == NULL;
    return got && strcmp(got, want) == 0;
}

static int text_is(const char *got, const char *want) { return got && strcmp(got, want) == 0; }

/* 目录里的条目数（不算 . ..），suffix 不为 NULL 时只数这个后缀的 */
static int count_entries(const char *dir, const char *suffix) {
    DIR *d = opendir(dir);
    if (!d) return -1;
    int n = 0;
    struct dirent *ent;
    while ((ent = readdir(d)) != NULL) {
        if (strcmp(ent->d_name, ".") == 0 || strcmp(ent->d_name, "..") == 0) continue;
        size_t len = strlen(ent->d_name);
        if (suffix && (len < strlen(suffix) || strcmp(ent->d_name + len - strlen(suffix), suffix) != 0))
            continue;
        n++;
    }
    closedir(d);
    return n;
}

static void remove_dir(const char *dir) {
    DIR *d = opendir(dir);
    if (!d) return;
    struct dirent *ent;
    char path[1024];
    while ((ent = readdir(d)) != NULL) {
        if (strcmp(ent->d_name, ".") == 0 || strcmp(ent->d_name, "..") == 0) continue;
        snprintf(path, sizeof path, "%s/%s", dir, ent->d_name);
        remove(path);
    }
    closedir(d);
    rmdir(dir);
}

static void session_smoke(void) {
    char base[] = "/tmp/wowtype_abi_session_XXXXXX";
    if (!mkdtemp(base)) {
        check(0, "mkdtemp");
        return;
    }
    char app[512], caches[512], lexicon[600];
    snprintf(app, sizeof app, "%s/app", base);
    snprintf(caches, sizeof caches, "%s/caches", base);
    snprintf(lexicon, sizeof lexicon, "%s/lexicon.tsv", app);
    check(mkdir(app, 0755) == 0, "mkdir app");
    FILE *f = fopen(lexicon, "w");
    fputs("ni hao\t你好\t100000\nni\t你\t60000\nhao\t好\t50000\nzhong guo\t中国\t90000\n"
          "shi\t是\t90000\nshi\t事\t60000\nshi\t时\t55000\nshi\t市\t40000\nshi\t使\t35000\n"
          "shi\t世\t30000\nshi\t式\t25000\nshi\t试\t20000\n",
          f);
    fclose(f);

    /* 词库快照只写进缓存目录，不写在词库旁边（.app 里签过名） */
    WowtypeEngineHandle *e = wowtype_engine_new_with_cache(lexicon, "", caches);
    check(e != NULL, "engine_new_with_cache");
    if (!e) return;
    check(count_entries(app, NULL) == 1, "new_with_cache: 词库旁边没有快照");
    check(count_entries(caches, ".wcache") == 1, "new_with_cache: 缓存目录里有一个 .wcache");
    wowtype_engine_free(e);
    e = wowtype_engine_new_with_cache(lexicon, "", caches);
    check(e != NULL && count_entries(app, NULL) == 1 && count_entries(caches, NULL) == 1,
          "new_with_cache: 第二次从快照载入，不多写文件");
    if (!e) return;

    /* 设置文本：默认值读回来没有认不出的行 */
    const char *defaults = wowtype_config_default_ini();
    check(defaults && strstr(defaults, "PageSize=9\n") && strstr(defaults, "ShiftToggle=Both\n"),
          "config_default_ini");
    check(wowtype_engine_configure(e, defaults) == 0, "engine_configure: 默认值 0 行认不出");
    check(wowtype_engine_configure(e, "Bogus=1\nPageSize=99\n") == 2,
          "engine_configure: 数认不出的行");
    check(wowtype_engine_configure(NULL, defaults) == -1, "engine_configure: NULL 引擎 -1");
    check(wowtype_engine_configure(e, NULL) == -1, "engine_configure: NULL 文本 -1");
    wowtype_engine_configure(e, "Shuangpin=True\nShuangpinScheme=Microsoft\n");
    WowtypeCandidates *c = wowtype_engine_input(e, "vsgo");
    check(c && wowtype_candidates_len(c) > 0 && text_is(wowtype_candidates_text(c, 0), "中国"),
          "engine_configure: 微软双拼 vsgo -> 中国");
    wowtype_candidates_free(c);
    wowtype_engine_configure(e, defaults);

    WowtypeSession *s = wowtype_session_new();
    check(s != NULL, "session_new");
    if (!s) return;
    check(wowtype_session_configure(s, defaults) == 0, "session_configure: 默认值");
    check(wowtype_session_configure(s, "PageSize\nNoSuchKey=1\n") == 2,
          "session_configure: 数认不出的行");
    check(wowtype_session_configure(NULL, defaults) == -1, "session_configure: NULL -1");
    check(wowtype_session_configure(s, NULL) == -1, "session_configure: NULL 文本 -1");
    check(wowtype_session_mode(s) == WOWTYPE_MODE_CHINESE, "session: 默认中文");
    check(text_is(wowtype_session_preedit(s), "") && text_is(wowtype_session_aux(s), ""),
          "session: 空编辑区、空提示");

    /* nihao 空格 -> 你好 */
    session_type(s, e, "nihao");
    check(wowtype_session_is_composing(s) == 1, "session: nihao 在编辑");
    check(text_is(wowtype_session_preedit(s), "ni'hao"), "session: 编辑区分音节 ni'hao");
    check(text_is(wowtype_session_candidate_text(s, 0), "你好"), "session: 第一个候选 你好");
    check(session_key(s, e, WOWTYPE_KEY_NONE, ' ', 0) == 1, "session: 空格被接住");
    check(commit_is(s, "你好"), "session: nihao 空格上屏 你好");
    check(commit_is(s, NULL), "session: 上屏文字取一次就清空");
    check(wowtype_session_is_composing(s) == 0, "session: 上屏后不在编辑");

    /* 选句首词：编辑区 = 已选的词（不加下划线）+ 剩下的拼音 */
    session_type(s, e, "nihaozhongguo");
    {
        size_t n = wowtype_session_candidate_count(s), at = n;
        for (size_t i = 0; i < n; i++) {
            if (text_is(wowtype_session_candidate_text(s, i), "你好")) at = i;
        }
        check(at < n && at < 9, "session: 第一页有句首词 你好");
        session_key(s, e, WOWTYPE_KEY_NONE, (unsigned)('1' + at), 0);
    }
    check(commit_is(s, NULL), "session: 选句首词不上屏");
    check(text_is(wowtype_session_preedit(s), "你好zhong'guo"), "session: 编辑区 你好zhong'guo");
    check(wowtype_session_preedit_plain_len(s) == strlen("你好"), "session: preedit_plain_len");
    check(wowtype_session_preedit_cursor(s) == strlen("你好zhong'guo"), "session: 光标在末尾");
    /* commit_composition：已选的词 + 原始字母上屏 */
    wowtype_session_commit_composition(s, e);
    check(commit_is(s, "你好zhongguo"), "session: commit_composition 上屏已选 + 原始字母");
    wowtype_session_commit_composition(s, e);
    check(commit_is(s, NULL), "session: 没在编辑时 commit_composition 什么也不做");

    /* 翻页和选择键 */
    check(wowtype_session_configure(s, "PageSize=3\nEmojiSuggest=False\n") == 0,
          "session_configure: PageSize=3");
    session_type(s, e, "shi");
    check(wowtype_session_candidate_count(s) == 3, "session: 每页 3 个");
    check(text_is(wowtype_session_candidate_label(s, 0), "1") &&
              text_is(wowtype_session_candidate_label(s, 2), "3"),
          "session: 选择键 1..3");
    check(wowtype_session_candidate_label(s, 3) == NULL &&
              wowtype_session_candidate_text(s, 3) == NULL,
          "session: 越界返回 NULL");
    check(!wowtype_session_has_prev_page(s) && wowtype_session_has_next_page(s),
          "session: 第一页有下一页");
    check(wowtype_session_page(s, -1) == 0, "session: 第一页往前翻不动");
    {
        char first[64], second[64];
        snprintf(first, sizeof first, "%s", wowtype_session_candidate_text(s, 0));
        check(wowtype_session_page(s, 1) == 1, "session: 翻到第二页");
        snprintf(second, sizeof second, "%s", wowtype_session_candidate_text(s, 0));
        check(strcmp(first, second) != 0 && wowtype_session_has_prev_page(s),
              "session: 第二页的候选不同");
        check(wowtype_session_candidate_cursor(s) == 0, "session: 翻页后光标在这页第一个");
        session_key(s, e, WOWTYPE_KEY_NONE, '1', 0);
        check(commit_is(s, second), "session: 第二页按 1 选第二页第一个");
    }
    /* 鼠标点选 */
    session_type(s, e, "shi");
    {
        char want[64];
        snprintf(want, sizeof want, "%s", wowtype_session_candidate_text(s, 1));
        check(wowtype_session_select(s, e, 3) == 0, "session_select: 这一屏没有第 4 个");
        check(wowtype_session_select(s, e, 1) == 1 && commit_is(s, want),
              "session_select: 点第二个上屏");
        check(wowtype_session_select(s, e, 0) == 0, "session_select: 没在编辑时没有候选");
    }
    /* 网格 */
    session_type(s, e, "shi");
    session_key(s, e, WOWTYPE_KEY_DOWN, 0, 0);
    check(wowtype_session_columns(s) == 6 && wowtype_session_vertical(s) == 0,
          "session: 横排时下键展开成每行 6 个");
    session_key(s, e, WOWTYPE_KEY_ESCAPE, 0, 0);
    check(wowtype_session_columns(s) == 0 && text_is(wowtype_session_preedit(s), "shi"),
          "session: Esc 只收起");
    /* reset：丢掉编辑中的内容，不上屏 */
    wowtype_session_reset(s);
    check(!wowtype_session_is_composing(s) && commit_is(s, NULL) &&
              text_is(wowtype_session_preedit(s), "") && wowtype_session_candidate_count(s) == 0,
          "session_reset: 清空、不上屏");

    /* 词库还在加载：引擎为 NULL 时字母照样进编辑区，refresh 后出候选 */
    session_type(s, NULL, "nihao");
    check(wowtype_session_is_composing(s) && wowtype_session_candidate_count(s) == 0,
          "session: 没有引擎时没有候选");
    wowtype_session_refresh(s, e);
    check(text_is(wowtype_session_candidate_text(s, 0), "你好"), "session_refresh: 有引擎后出候选");
    wowtype_session_reset(s);

    /* 单按 Shift 切英文 */
    {
        WowtypeKeyEvent ev;
        memset(&ev, 0, sizeof ev);
        ev.struct_size = sizeof ev;
        ev.key = WOWTYPE_KEY_SHIFT_R;
        ev.time_ms = (session_now += 10);
        wowtype_session_process_key(s, e, &ev);
        ev.is_release = 1;
        ev.modifiers = WOWTYPE_MOD_SHIFT;
        ev.time_ms = (session_now += 50);
        check(wowtype_session_process_key(s, e, &ev) == 1 &&
                  wowtype_session_mode(s) == WOWTYPE_MODE_ASCII,
              "session: 单按右 Shift 切英文");
        check(session_key(s, e, WOWTYPE_KEY_NONE, 'a', 0) == 0, "session: 英文时字母交给应用");
        ev.is_release = 0;
        ev.modifiers = 0;
        ev.time_ms = (session_now += 10);
        wowtype_session_process_key(s, e, &ev);
        ev.is_release = 1;
        ev.modifiers = WOWTYPE_MOD_SHIFT;
        ev.time_ms = (session_now += 50);
        wowtype_session_process_key(s, e, &ev);
        check(wowtype_session_mode(s) == WOWTYPE_MODE_CHINESE, "session: 再按一次回到中文");
        /* struct_size 不对的事件不处理 */
        ev.struct_size = 8;
        ev.is_release = 0;
        ev.key = WOWTYPE_KEY_NONE;
        ev.unicode = 'n';
        check(wowtype_session_process_key(s, e, &ev) == 0 && !wowtype_session_is_composing(s),
              "session: struct_size 太小的事件不处理");
    }

    /* 学够 20 次请求存盘 */
    check(wowtype_session_take_save_request(s) == 0, "session: 还没要求存盘");
    for (int i = 0; i < 20; i++) {
        session_type(s, e, "nihao ");
    }
    {
        char want[256] = "";
        for (int i = 0; i < 20; i++) strcat(want, "你好");
        check(commit_is(s, want), "session: 没取走的上屏文字连在一起");
    }
    check(wowtype_session_take_save_request(s) == 1, "session: 20 次学习后要求存盘");
    check(wowtype_session_take_save_request(s) == 0, "session: 存盘请求取一次就清零");

    /* NULL 会话不崩 */
    check(wowtype_session_process_key(NULL, e, NULL) == 0 && wowtype_session_preedit(NULL) == NULL &&
              wowtype_session_candidate_count(NULL) == 0 && wowtype_session_take_commit(NULL) == NULL &&
              wowtype_session_mode(NULL) == -1,
          "session: NULL 会话不崩");
    check(wowtype_session_process_key(s, e, NULL) == 0, "session: NULL 事件返回 0");
    wowtype_session_free(NULL);

    wowtype_session_free(s);
    wowtype_engine_free(e);
    remove_dir(caches);
    remove_dir(app);
    rmdir(base);
}

int main(int argc, char **argv) {
    const char *lexicon = argc > 1 ? argv[1] : "/tmp/wowtype_abi.tsv";
    const char *bigram = argc > 2 ? argv[2] : "";
    WowtypeEngineHandle *e = wowtype_engine_new(lexicon, bigram);
    if (!e) {
        printf("engine_new failed: %s", wowtype_last_error());
        putchar(10);
        return 1;
    }

    WowtypeCandidates *c = wowtype_engine_input(e, "nihao");
    check(c && has_candidate(c, "你好"), "exact: nihao -> 你好");
    wowtype_candidates_free(c);

    c = wowtype_engine_input(e, "nih");
    check(c && has_candidate(c, "你好"), "partial: nih -> 你好");
    wowtype_candidates_free(c);

    c = wowtype_engine_input(e, "hello");
    check(c && has_candidate(c, "hello"), "english: hello -> hello");
    wowtype_candidates_free(c);

    /* 句首词候选：consumed 小于输入长度，剩下的部分留给下一轮 */
    c = wowtype_engine_input(e, "nihaozhongguo");
    {
        int found = 0;
        size_t n = wowtype_candidates_len(c);
        for (size_t i = 0; c && i < n; i++) {
            const char *t = wowtype_candidates_text(c, i);
            if (t && strcmp(t, "你好") == 0 && wowtype_candidates_consumed(c, i) == 5) found = 1;
        }
        check(found, "prefix: nihaozhongguo 有 你好(用掉 5 字节)");
        check(c && wowtype_candidates_consumed(c, 0) == 13, "整句候选用掉全部输入");
        const char *py = c ? wowtype_candidates_pinyin(c, 0) : NULL;
        check(py && strcmp(py, "ni hao zhong guo") == 0, "candidates_pinyin");
    }
    wowtype_candidates_free(c);

    /* 按错键：zhonggup（o 旁边的 p） */
    c = wowtype_engine_input(e, "zhonggup");
    check(c && has_candidate(c, "中国"), "typo: zhonggup -> 中国");
    wowtype_candidates_free(c);
    check(wowtype_engine_set_typo_correction(e, 0) == 0, "set_typo_correction");
    c = wowtype_engine_input(e, "zhonggup");
    check(c && !has_candidate(c, "中国"), "纠错关闭后 zhonggup 不出 中国");
    wowtype_candidates_free(c);
    wowtype_engine_set_typo_correction(e, 1);
    check(wowtype_engine_set_max_candidates(e, 5) == 0, "set_max_candidates");

    check(wowtype_engine_record_selection(e, "你好") == 0, "record_selection");
    check(wowtype_engine_set_context(e, "今天") == 0, "set_context");

    c = wowtype_engine_input(e, "tian");
    check(c && has_candidate(c, "天"), "context: tian -> 天");
    wowtype_candidates_free(c);

    c = wowtype_engine_predict_next(e, "今天", 3);
    check(c && wowtype_candidates_len(c) > 0, "predict_next 有结果");
    wowtype_candidates_free(c);

    check(wowtype_engine_add_user_word(e, "wo wo te", "我特") == 0, "add_user_word");
    c = wowtype_engine_input(e, "wowote");
    check(c && has_candidate(c, "我特"), "user word 可输入");
    wowtype_candidates_free(c);

    check(wowtype_engine_set_fuzzy(e, 1) == 0, "set_fuzzy");
    c = wowtype_engine_input(e, "zidao");
    check(c && has_candidate(c, "知道"), "fuzzy: zidao -> 知道");
    wowtype_candidates_free(c);

    check(wowtype_engine_set_shuangpin(e, 1) == 0, "set_shuangpin");
    c = wowtype_engine_input(e, "vsgo");
    check(c && has_candidate(c, "中国"), "shuangpin: vsgo -> 中国");
    wowtype_candidates_free(c);
    wowtype_engine_set_shuangpin(e, 0);

    const char *s2t = argc > 3 ? argv[3] : "";
    if (*s2t) {
        check(wowtype_engine_load_conversion(e, s2t) > 0, "load_conversion");
        check(wowtype_engine_set_traditional(e, 1) == 0, "set_traditional");
        c = wowtype_engine_input(e, "zhongguo");
        check(c && has_candidate(c, "中國"), "traditional: zhongguo -> 中國");
        wowtype_candidates_free(c);
        wowtype_engine_set_traditional(e, 0);
    }

    check(wowtype_engine_save_user_dict(e, "/tmp/wowtype_abi_user.tsv") == 0, "save_user_dict");
    check(wowtype_engine_remove_user_word(e, "我特") == 1, "remove_user_word");
    check(wowtype_engine_load_user_dict(e, "/tmp/wowtype_abi_user.tsv") >= 1, "load_user_dict");
    {
        FILE *custom = fopen("/tmp/wowtype_abi_custom.tsv", "w");
        check(custom != NULL, "open custom.tsv");
        fputs("ni hao\t妮好\nnot pinyin zz\t坏\n", custom);
        fclose(custom);
        check(wowtype_engine_load_custom_words(e, "/tmp/wowtype_abi_custom.tsv") == 1,
              "load_custom_words");
        check(wowtype_engine_load_custom_words(e, "/tmp/wowtype_abi_no_such.tsv") == 0,
              "load_custom_words missing file clears");
        remove("/tmp/wowtype_abi_custom.tsv");
    }

    /* 自动学习：同一个词选 3 次后应当能直接打出来 */
    for (int i = 0; i < 3; i++) {
        wowtype_engine_record_selection_ex(e, "阿特", "a te");
    }
    c = wowtype_engine_input(e, "ate");
    check(c && has_candidate(c, "阿特"), "auto learn: ate -> 阿特");
    wowtype_candidates_free(c);

    c = wowtype_symbol_search("juhao", 3);
    check(c && wowtype_candidates_len(c) > 0 &&
              strcmp(wowtype_candidates_text(c, 0), "。") == 0,
          "symbol: juhao -> 。");
    wowtype_candidates_free(c);
    {
        const char *p = wowtype_fullwidth_punct(',');
        check(p && strcmp(p, "，") == 0, "punct: comma -> ，");
    }

    session_smoke();

    printf("version=%s", wowtype_version());
    putchar(10);
    wowtype_engine_free(e);

    printf(failures ? "ABI SMOKE FAILED" : "ABI SMOKE OK");
    putchar(10);
    return failures ? 1 : 0;
}