//! 日期、时间候选：打 riqi / shijian / xingqi（或英文 date / time）时，在首选词后面
//! 给出当前日期、时间的几种写法。
//!
//! 只认完整拼音，不认 rq、sj 这类简拼——它们太常见，平时打别的词会被打扰。
//! 当前时间由调用方（输入法插件）按本地时区传进来，这里不读系统时钟，方便测试。

/// 本地时间。weekday：0 = 星期日，1 = 星期一 …… 6 = 星期六。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub weekday: u32,
}

impl LocalTime {
    /// 由 Unix 毫秒和本地时区与 UTC 的差（秒，东八区 28800）算出本地时间。不读系统时区库。
    pub fn from_unix_ms(time_ms: i64, utc_offset_seconds: i32) -> Self {
        let secs = time_ms.div_euclid(1000) + i64::from(utc_offset_seconds);
        let days = secs.div_euclid(86_400);
        let of_day = secs.rem_euclid(86_400);
        // 公历日期：按 3 月 1 日起算的 400 年周期（Howard Hinnant 的 civil_from_days）
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        Self {
            year: year as i32,
            month: month as u32,
            day: day as u32,
            hour: (of_day / 3600) as u32,
            minute: (of_day % 3600 / 60) as u32,
            second: (of_day % 60) as u32,
            // 1970-01-01 是星期四
            weekday: (days + 4).rem_euclid(7) as u32,
        }
    }
}

const WEEKDAYS: [&str; 7] = ["日", "一", "二", "三", "四", "五", "六"];

/// 触发词（拼音或英文，小写、不带隔音符）对应的候选；不是触发词时返回空。
pub fn candidates(key: &str, now: &LocalTime) -> Vec<String> {
    let key: String = key
        .chars()
        .filter(|c| !matches!(c, '\'' | ' '))
        .flat_map(char::to_lowercase)
        .collect();
    let LocalTime {
        year: y,
        month: mo,
        day: d,
        hour: h,
        minute: mi,
        second: s,
        weekday,
    } = *now;
    let week = WEEKDAYS[(weekday % 7) as usize];
    match key.as_str() {
        "riqi" | "date" => vec![
            format!("{y}年{mo}月{d}日"),
            format!("{y}-{mo:02}-{d:02}"),
            format!("{y}/{mo:02}/{d:02}"),
            format!("{y}.{mo:02}.{d:02}"),
            format!("{y}{mo:02}{d:02}"),
            format!("{mo}月{d}日"),
            format!("{y}年{mo}月{d}日 星期{week}"),
        ],
        "shijian" | "time" => vec![
            format!("{h:02}:{mi:02}"),
            format!("{h:02}:{mi:02}:{s:02}"),
            format!("{y}-{mo:02}-{d:02} {h:02}:{mi:02}"),
            format!("{y}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}"),
            format!("{h}时{mi}分"),
            format!("{y}年{mo}月{d}日 {h:02}:{mi:02}"),
        ],
        "xingqi" => vec![format!("星期{week}"), format!("周{week}")],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> LocalTime {
        LocalTime {
            year: 2026,
            month: 11,
            day: 10,
            hour: 9,
            minute: 5,
            second: 1,
            weekday: 2,
        }
    }

    #[test]
    fn date_formats() {
        let c = candidates("riqi", &now());
        assert_eq!(c[0], "2026年11月10日");
        assert!(c.contains(&"2026-11-10".to_string()));
        assert!(c.contains(&"2026/11/10".to_string()));
        assert!(c.contains(&"2026.11.10".to_string()));
        assert!(c.contains(&"20261110".to_string()));
        assert!(c.contains(&"2026年11月10日 星期二".to_string()));
        assert_eq!(candidates("date", &now()), c);
        assert_eq!(candidates("ri'qi", &now()), c);
    }

    #[test]
    fn time_formats() {
        let c = candidates("shijian", &now());
        assert_eq!(c[0], "09:05");
        assert!(c.contains(&"09:05:01".to_string()));
        assert!(c.contains(&"2026-11-10 09:05".to_string()));
        assert!(c.contains(&"9时5分".to_string()));
        assert_eq!(candidates("TIME", &now()), c);
    }

    #[test]
    fn weekday() {
        assert_eq!(candidates("xingqi", &now()), vec!["星期二", "周二"]);
        let sunday = LocalTime {
            weekday: 0,
            ..now()
        };
        assert_eq!(candidates("xingqi", &sunday)[0], "星期日");
    }

    #[test]
    fn local_time_from_unix_ms() {
        // 2026-11-10 01:05:01 UTC，东八区是 09:05:01，星期二
        assert_eq!(LocalTime::from_unix_ms(1_794_272_701_000, 8 * 3600), now());
        // 闰日
        assert_eq!(
            LocalTime::from_unix_ms(1_709_208_000_000, 0),
            LocalTime {
                year: 2024,
                month: 2,
                day: 29,
                hour: 12,
                minute: 0,
                second: 0,
                weekday: 4,
            }
        );
        // 西五区跨回前一天：2026-11-11 01:00 UTC 是 11 月 10 日 20:00
        let t = LocalTime::from_unix_ms(1_794_358_800_000, -5 * 3600);
        assert_eq!((t.month, t.day, t.hour, t.weekday), (11, 10, 20, 2));
        // 世纪闰年后的 3 月 1 日，1970 年 1 月 1 日是星期四，1969 年最后一秒是星期三
        let t = LocalTime::from_unix_ms(951_868_800_000, 0);
        assert_eq!((t.year, t.month, t.day, t.weekday), (2000, 3, 1, 3));
        assert_eq!(LocalTime::from_unix_ms(0, 0).weekday, 4);
        let t = LocalTime::from_unix_ms(-1000, 0);
        assert_eq!(
            (t.year, t.month, t.day, t.hour, t.minute, t.second, t.weekday),
            (1969, 12, 31, 23, 59, 59, 3)
        );
        // 毫秒不进位
        assert_eq!(LocalTime::from_unix_ms(1_794_272_701_999, 8 * 3600), now());
    }

    #[test]
    fn abbreviations_do_not_trigger() {
        for key in ["rq", "sj", "xq", "riq", "shij", "dat", ""] {
            assert!(candidates(key, &now()).is_empty(), "{key} 不该出日期时间");
        }
    }
}
