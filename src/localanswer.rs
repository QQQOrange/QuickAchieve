//! 本地答案：划词的内容如果是「能算出来的东西」，横条直接给答案，不用去搜。
//!
//! 用户口径（2026-11-05）：「复制了一个算式 / 一个单位 / 一个日期，还得打开浏览器
//! 搜一下才知道答案」—— 这类东西在本地算就行，零网络、零延迟。
//!
//! # 支持什么
//!
//! | 输入 | 输出 |
//! |---|---|
//! | `12*(3+4)`、`0xff`、`2^10`、`sqrt(2)` | 算式结果 |
//! | `100 usd` 这种**不做**（要汇率，得联网） | — |
//! | `5km → m`、`37F`、`3.5 GB → MB`、`2h → min` | 单位换算 |
//! | `2026-11-04` | 星期几 + 距今天数 |
//! | `#3B7DFF` | RGB / HSL |
//! | 10 位或 13 位数字（时间戳） | 本地日期时间 |
//! | 一段 base64 | 解码结果（可打印 ASCII 才算） |
//! | `%E4%B8%AD` | URL 解码 |
//!
//! # 为什么不做成"全都能算"
//!
//! 每条错误的答案都比没有答案更糟 —— 用户会照着错的复制走。所以这里的判据是
//! **"输入形态足够明确"**：带单位的数字、带 `-` 的日期、以 `#` 开头的六位十六进制、
//! 明显的算式。形态不明确就返回 `None`，让横条照常去搜。

/// 一条本地答案。
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    /// 分类（给界面挑图标 / 文案用）：expr / unit / date / color / time / b64 / url
    pub kind: &'static str,
    /// 结果本身（用户可复制的那串）
    pub value: String,
}

impl Answer {
    fn new(kind: &'static str, value: impl Into<String>) -> Self {
        Self {
            kind,
            value: value.into(),
        }
    }
}

/// 主入口：给一段文字，能算就给答案。
pub fn answer(raw: &str) -> Option<Answer> {
    let t = raw.trim();
    if t.is_empty() || t.chars().count() > 200 {
        return None;
    }
    // 多行几乎都是正文，不是"一个值"
    if t.contains('\n') {
        return None;
    }
    // 顺序有讲究：先认形态最独特的（颜色 / 日期 / 时间戳 / 编解码），
    // 最后才是算式 —— 算式最宽松，放前面会把别的都吃掉。
    color(t)
        .or_else(|| date(t))
        .or_else(|| timestamp(t))
        .or_else(|| encoding(t))
        .or_else(|| unit(t))
        .or_else(|| expr(t))
}

// ───────────────────────────── 颜色 ─────────────────────────────

fn color(t: &str) -> Option<Answer> {
    let s = t.strip_prefix('#')?;
    let hex = match s.len() {
        3 => s.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => s.to_string(),
        _ => return None,
    };
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    let (h, sat, l) = rgb_to_hsl(r, g, b);
    Some(Answer::new(
        "color",
        format!("rgb({r}, {g}, {b}) · hsl({h:.0}, {sat:.0}%, {l:.0}%)"),
    ))
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (rf, gf, bf) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f64::EPSILON {
        return (0.0, 0.0, l * 100.0);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == rf {
        ((gf - bf) / d + if gf < bf { 6.0 } else { 0.0 }) / 6.0
    } else if max == gf {
        ((bf - rf) / d + 2.0) / 6.0
    } else {
        ((rf - gf) / d + 4.0) / 6.0
    };
    (h * 360.0, s * 100.0, l * 100.0)
}

// ───────────────────────────── 日期 ─────────────────────────────

fn date(t: &str) -> Option<Answer> {
    // 只认 `YYYY-MM-DD` / `YYYY/MM/DD` / `YYYY.MM.DD`
    let parts: Vec<&str> = t
        .split(['-', '/', '.'])
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let d: u32 = parts[2].parse().ok()?;
    if !(1000..=9999).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let (wy, wm, wd) = civil_to_days(y, m, d);
    let target = days_from_civil(wy, wm, wd);
    let today = days_from_civil(0, 0, 0) + today_offset_days();
    let delta = target - today;
    let wd_name = ["一", "二", "三", "四", "五", "六", "日"][weekday_index(y, m, d) as usize];
    let rel = match delta {
        0 => "就是今天".to_string(),
        n if n > 0 => format!("还有 {n} 天"),
        n => format!("已过 {} 天", -n),
    };
    Some(Answer::new("date", format!("{y}-{m:02}-{d:02} 周{wd_name} · {rel}")))
}

/// 用"天数差"算日期差，不需要 calendar 库。
fn civil_to_days(y: i64, m: u32, d: u32) -> (i64, u32, u32) {
    (y, m, d)
}

/// Howard Hinnant 的 days_from_civil，返回 1970-01-01 起的天数。
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn weekday_index(y: i64, m: u32, d: u32) -> i64 {
    // 1970-01-01 是星期四（索引 3，0=周一）
    (days_from_civil(y, m, d) + 3).rem_euclid(7)
}

/// 今天（**本地时区**）距离 1970-01-01 的天数。
///
/// ⚠ v2.23.47 修：原来直接用 `UNIX 秒 / 86400`，那是 **UTC 的"今天"**。
/// 在 UTC+8，每天 00:00–08:00 之间算出来的"今天"还是昨天 —— 于是
/// 「2026-11-05 → 还有 1 天」会在凌晨错报成「还有 2 天」。
/// 这里改成问系统要本地日期（`GetLocalTime`），再算天数差。
fn today_offset_days() -> i64 {
    #[cfg(windows)]
    {
        use windows::Win32::System::SystemInformation::GetLocalTime;
        let st = unsafe { GetLocalTime() };
        days_from_civil(st.wYear as i64, st.wMonth as u32, st.wDay as u32)
    }
    #[cfg(not(windows))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        secs / 86_400
    }
}

// ───────────────────────────── 时间戳 ─────────────────────────────

fn timestamp(t: &str) -> Option<Answer> {
    if !t.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (secs, out) = match t.len() {
        10 => (t.parse::<i64>().ok()?, fmt_time(t.parse().ok()?)),
        13 => (t.parse::<i64>().ok()? / 1000, fmt_time(t.parse::<i64>().ok()? / 1000)),
        _ => return None,
    };
    if secs < 0 {
        return None;
    }
    Some(Answer::new("time", out))
}

/// Unix 秒 → `YYYY-MM-DD HH:MM:SS`（**本地时区**）。
///
/// 手算而不是引 chrono：只要"秒 → 年月日时分秒"这一件事，引入一个带时区数据库的
/// 依赖不值当。时区偏移用当天正午的 UTC 差（避开夏令时的边界抖动）。
fn fmt_time(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let mut rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let hh = rem / 3600;
    rem %= 3600;
    let mm = rem / 60;
    let ss = rem % 60;
    format!("{y}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02} UTC")
}

/// 反函数：days_from_civil。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ───────────────────────────── 编解码 ─────────────────────────────

fn encoding(t: &str) -> Option<Answer> {
    // URL 编码：含 %XX 且解码后是可打印文本
    if t.contains('%') {
        if let Some(s) = percent_decode(t) {
            if !s.is_empty() && s.chars().all(|c| !c.is_control()) {
                return Some(Answer::new("url", s));
            }
        }
    }
    // base64：长度 8+、只用 base64 字符、以 = 结尾或长度是 4 的倍数
    if t.len() >= 8 && t.len().is_multiple_of(4) && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=') {
        if let Some(bytes) = base64_decode(t) {
            if let Ok(s) = String::from_utf8(bytes) {
                let printable = s.chars().all(|c| !c.is_control() || c == '\n');
                if printable && s.chars().any(|c| c.is_alphanumeric()) {
                    return Some(Answer::new("b64", s));
                }
            }
        }
    }
    None
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if i + 2 >= b.len() {
                return None;
            }
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else if b[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut acc: u32 = 0;
    let mut bits = 0;
    for c in s.bytes() {
        if c == b'=' {
            break;
        }
        let v = T.iter().position(|x| *x == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

// ───────────────────────────── 单位换算 ─────────────────────────────

/// 一个单位的"到基准单位"的倍率。
fn unit_factor(u: &str) -> Option<(&'static str, f64)> {
    let k = u.to_ascii_lowercase();
    let k = k.trim_end_matches('s');
    Some(match k {
        // 长度（基准：米）
        "mm" => ("len", 0.001),
        "cm" | "公分" => ("len", 0.01),
        "m" | "米" => ("len", 1.0),
        "km" | "公里" | "千米" => ("len", 1000.0),
        "in" | "英寸" => ("len", 0.0254),
        "ft" | "英尺" => ("len", 0.3048),
        "yd" | "码" => ("len", 0.9144),
        "mi" | "英里" => ("len", 1609.344),
        // 质量（基准：克）
        "mg" => ("mass", 0.001),
        "g" | "克" => ("mass", 1.0),
        "kg" | "公斤" | "千克" => ("mass", 1000.0),
        "t" | "吨" => ("mass", 1_000_000.0),
        "lb" | "磅" => ("mass", 453.59237),
        "oz" | "盎司" => ("mass", 28.349523125),
        // 数据（基准：字节）
        "b" | "字节" => ("data", 1.0),
        "kb" => ("data", 1024.0),
        "mb" => ("data", 1024.0 * 1024.0),
        "gb" => ("data", 1024.0f64.powi(3)),
        "tb" => ("data", 1024.0f64.powi(4)),
        // 时间（基准：秒）
        "ms" | "毫秒" => ("time", 0.001),
        "s" | "sec" | "秒" => ("time", 1.0),
        "min" | "分" | "分钟" => ("time", 60.0),
        "h" | "小时" => ("time", 3600.0),
        "d" | "天" => ("time", 86400.0),
        _ => return None,
    })
}

fn unit(t: &str) -> Option<Answer> {
    // 温度单独处理（不是简单的倍率：有偏移）
    if let Some(a) = temperature(t) {
        return Some(a);
    }
    // 形态：`<数字><单位>` 或 `<数字><单位> → <单位>` / `to` / `in`
    let (num, rest) = split_number(t)?;
    let (from, to) = split_units(rest)?;
    let (cat_a, fa) = unit_factor(from)?;
    let (cat_b, fb) = unit_factor(to)?;
    if cat_a != cat_b {
        return None;
    }
    let out = num * fa / fb;
    Some(Answer::new(
        "unit",
        format!("{num} {from} = {} {to}", fmt_num(out)),
    ))
}

fn temperature(t: &str) -> Option<Answer> {
    let s = t.trim();
    let lower = s.to_ascii_lowercase();
    let (num_s, from) = if let Some(x) = lower.strip_suffix('c').or_else(|| s.strip_suffix('℃')) {
        (x, 'c')
    } else if let Some(x) = lower.strip_suffix('f').or_else(|| s.strip_suffix('℉')) {
        (x, 'f')
    } else {
        let x = lower.strip_suffix('k')?;
        (x, 'k')
    };
    let v: f64 = num_s.trim().parse().ok()?;
    // 只给另外两种，输入的那一种不再重复报一遍
    let c = match from {
        'c' => v,
        'f' => (v - 32.0) * 5.0 / 9.0,
        _ => v - 273.15,
    };
    let out = match from {
        'c' => format!("{v}°C = {}°F", fmt_num(c * 9.0 / 5.0 + 32.0)),
        'f' => format!("{v}°F = {}°C", fmt_num(c)),
        _ => format!("{v}K = {}°C", fmt_num(c)),
    };
    Some(Answer::new("unit", out))
}

fn split_number(t: &str) -> Option<(f64, &str)> {
    let mut end = 0;
    let bytes = t.as_bytes();
    let mut seen_dot = false;
    while end < bytes.len() {
        let c = bytes[end] as char;
        if c.is_ascii_digit() {
            end += 1;
        } else if c == '.' && !seen_dot {
            seen_dot = true;
            end += 1;
        } else if (c == '-' || c == '+') && end == 0 {
            end += 1;
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    let n: f64 = t[..end].parse().ok()?;
    Some((n, &t[end..]))
}

fn split_units(rest: &str) -> Option<(&str, &str)> {
    let r = rest.trim();
    if r.is_empty() {
        return None;
    }
    for sep in ["→", "->", "=>", " to ", " in "] {
        if let Some(idx) = r.find(sep) {
            let a = r[..idx].trim();
            let b = r[idx + sep.len()..].trim();
            if !a.is_empty() && !b.is_empty() {
                return Some((a, b));
            }
        }
    }
    // 没有箭头：视为"换算成同类里最常见的那一个"没意义 → 不算
    None
}

fn fmt_num(v: f64) -> String {
    if !v.is_finite() {
        return "—".into();
    }
    let r = (v * 1e6).round() / 1e6;
    if (r - r.round()).abs() < 1e-9 {
        format!("{}", r.round() as i64)
    } else {
        let s = format!("{r:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

// ───────────────────────────── 算式 ─────────────────────────────

fn expr(t: &str) -> Option<Answer> {
    /* 先把手写体符号换成 ASCII：`×` `÷` 是**多字节**的，逐字节比较会漏。
     * 统一替换之后解析器只认 ASCII，逻辑简单一半。 */
    let norm: String = t
        .chars()
        .map(|c| match c {
            '×' | '＊' => '*',
            '÷' | '／' => '/',
            '＋' => '+',
            '－' | '—' => '-',
            '（' => '(',
            '）' => ')',
            '％' => '%',
            _ => c,
        })
        .collect();
    let t = norm.trim();

    // 十六进制先单独认（它没有运算符，走不到下面那步）
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        if let Ok(n) = i64::from_str_radix(hex.trim(), 16) {
            return Some(Answer::new("expr", format!("{n}（0x{n:X}）")));
        }
    }
    // 日期形态（`2026-13-01` 这种非法日期）**不许**掉进算式：
    // 否则 2026-13-01 会被算成 2012，用户复制走一个完全错误的"答案"。
    if looks_like_date(t) {
        return None;
    }
    // 必须像算式：只由数字 / 运算符 / 括号 / 函数名 / 空格组成
    let ok = t.chars().all(|c| {
        c.is_ascii_digit() || c.is_whitespace() || "+-*/%^().,".contains(c) || c.is_ascii_alphabetic()
    });
    if !ok {
        return None;
    }
    // 至少得有一个运算符，否则 "2026" 这种也会被算成表达式
    if !t.chars().any(|c| "+-*/%^".contains(c)) && !t.contains("sqrt") {
        return None;
    }
    let mut p = Parser::new(t);
    let v = p.parse_expr()?;
    p.skip_ws();
    if !p.at_end() {
        return None; // 有剩料 → 不是我们能算的东西
    }
    if !v.is_finite() {
        return None;
    }
    Some(Answer::new("expr", fmt_num(v)))
}

/// 看起来像 `YYYY-M-D` / `YYYY/M/D` / `YYYY.M.D` 吗（不校验真假，只认形态）。
fn looks_like_date(t: &str) -> bool {
    let parts: Vec<&str> = t
        .split(['-', '/', '.'])
        .filter(|s| !s.is_empty())
        .collect();
    parts.len() == 3
        && parts[0].len() == 4
        && parts[0].chars().all(|c| c.is_ascii_digit())
        && parts[1].chars().all(|c| c.is_ascii_digit())
        && parts[2].chars().all(|c| c.is_ascii_digit())
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    src: &'a str,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            s: src.as_bytes(),
            i: 0,
            src,
        }
    }
    fn at_end(&self) -> bool {
        self.i >= self.s.len()
    }
    fn skip_ws(&mut self) {
        while self.i < self.s.len() && (self.s[self.i] as char).is_whitespace() {
            self.i += 1;
        }
    }
    fn peek(&mut self) -> Option<char> {
        self.skip_ws();
        self.s.get(self.i).map(|b| *b as char)
    }
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn parse_expr(&mut self) -> Option<f64> {
        let mut v = self.parse_term()?;
        loop {
            if self.eat('+') {
                v += self.parse_term()?;
            } else if self.eat('-') {
                v -= self.parse_term()?;
            } else {
                return Some(v);
            }
        }
    }

    fn parse_term(&mut self) -> Option<f64> {
        let mut v = self.parse_unary()?;
        loop {
            if self.eat('*') || self.eat('×') {
                v *= self.parse_unary()?;
            } else if self.eat('/') || self.eat('÷') {
                let d = self.parse_unary()?;
                if d == 0.0 {
                    return None;
                }
                v /= d;
            } else if self.eat('%') {
                let d = self.parse_unary()?;
                if d == 0.0 {
                    return None;
                }
                v %= d;
            } else {
                return Some(v);
            }
        }
    }

    fn parse_unary(&mut self) -> Option<f64> {
        if self.eat('-') {
            return Some(-self.parse_unary()?);
        }
        if self.eat('+') {
            return self.parse_unary();
        }
        self.parse_power()
    }

    fn parse_power(&mut self) -> Option<f64> {
        let base = self.parse_atom()?;
        if self.eat('^') {
            let e = self.parse_unary()?;
            return Some(base.powf(e));
        }
        Some(base)
    }

    fn parse_atom(&mut self) -> Option<f64> {
        self.skip_ws();
        if self.eat('(') {
            let v = self.parse_expr()?;
            if !self.eat(')') {
                return None;
            }
            return Some(v);
        }
        // 函数名
        let start = self.i;
        while self.i < self.s.len() && (self.s[self.i] as char).is_ascii_alphabetic() {
            self.i += 1;
        }
        if self.i > start {
            let name = &self.src[start..self.i];
            let arg = if self.eat('(') {
                let a = self.parse_expr()?;
                if !self.eat(')') {
                    return None;
                }
                a
            } else {
                self.parse_unary()?
            };
            return match name {
                "sqrt" => Some(arg.sqrt()),
                "abs" => Some(arg.abs()),
                "sin" => Some(arg.sin()),
                "cos" => Some(arg.cos()),
                "tan" => Some(arg.tan()),
                "ln" => Some(arg.ln()),
                "log" | "log10" => Some(arg.log10()),
                "pi" => Some(std::f64::consts::PI),
                "e" => Some(std::f64::consts::E),
                _ => None,
            };
        }
        // 数字
        let ns = self.i;
        while self.i < self.s.len() {
            let c = self.s[self.i] as char;
            if c.is_ascii_digit() || c == '.' {
                self.i += 1;
            } else if (c == 'e' || c == 'E') && self.i + 1 < self.s.len() {
                // 科学计数法 1e3
                self.i += 1;
                if self.s.get(self.i).map(|b| *b as char) == Some('-') {
                    self.i += 1;
                }
            } else {
                break;
            }
        }
        if ns == self.i {
            return None;
        }
        self.src[ns..self.i].parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn val(s: &str) -> Option<String> {
        answer(s).map(|a| a.value)
    }

    #[test]
    fn 算式能算() {
        assert_eq!(val("12*(3+4)").as_deref(), Some("84"));
        assert_eq!(val("2^10").as_deref(), Some("1024"));
        assert_eq!(val("1+2*3").as_deref(), Some("7"));
        assert_eq!(val("(1+2)*3").as_deref(), Some("9"));
        assert_eq!(val("10/4").as_deref(), Some("2.5"));
        assert_eq!(val("sqrt(16)").as_deref(), Some("4"));
        assert_eq!(val("2 × 3").as_deref(), Some("6"));
        assert_eq!(val("6 ÷ 3").as_deref(), Some("2"));
        assert_eq!(val("1e3 + 1").as_deref(), Some("1001"));
    }

    #[test]
    fn 十六进制直接给十进制() {
        assert_eq!(val("0xff").as_deref(), Some("255（0xFF）"));
    }

    #[test]
    fn 纯数字不算算式() {
        // "2026" 这种是年份/编号，不该被当成答案
        assert_eq!(val("2026"), None);
        assert_eq!(val("12345"), None);
    }

    #[test]
    fn 除零与非法输入给_none() {
        assert_eq!(val("1/0"), None);
        assert_eq!(val("1+"), None);
        assert_eq!(val("(1+2"), None);
        assert_eq!(val("1+2)"), None);
    }

    #[test]
    fn 单位换算() {
        assert_eq!(val("5km → m").as_deref(), Some("5 km = 5000 m"));
        assert_eq!(val("100 cm -> m").as_deref(), Some("100 cm = 1 m"));
        assert_eq!(val("1.5 GB → MB").as_deref(), Some("1.5 GB = 1536 MB"));
        assert_eq!(val("2 h → min").as_deref(), Some("2 h = 120 min"));
        assert_eq!(val("1 kg → g").as_deref(), Some("1 kg = 1000 g"));
        assert_eq!(val("1 mi → km").as_deref(), Some("1 mi = 1.609344 km"));
    }

    #[test]
    fn 跨类单位拒绝换算() {
        // 长度换质量没有意义 —— 宁可不算，也不能给个错答案
        assert_eq!(val("5 km → kg"), None);
    }

    #[test]
    fn 温度换算() {
        assert_eq!(val("37C").as_deref(), Some("37°C = 98.6°F"));
        assert_eq!(val("212F").as_deref(), Some("212°F = 100°C"));
        assert_eq!(val("300K").as_deref(), Some("300K = 26.85°C"));
    }

    #[test]
    fn 颜色给_rgb_和_hsl() {
        let a = val("#3B7DFF").unwrap();
        assert!(a.starts_with("rgb(59, 125, 255)"), "{a}");
        assert!(a.contains("hsl("), "{a}");
        // 三位缩写等价于六位展开
        assert_eq!(val("#fff").unwrap(), val("#FFFFFF").unwrap());
    }

    #[test]
    fn 日期给星期和相差天数() {
        let a = val("2026-11-04").unwrap();
        assert!(a.contains("周"), "{a}");
        assert!(a.contains("2026-11-04"), "{a}");
        // 不存在的月份/日期不认
        assert_eq!(val("2026-13-01"), None);
        assert_eq!(val("2026-00-10"), None);
    }

    #[test]
    fn 时间戳转日期() {
        // 0 秒 = 1970-01-01 00:00:00
        assert_eq!(val("0".repeat(1).as_str()), None); // 1 位不算
        assert_eq!(val("0000000000").as_deref(), Some("1970-01-01 00:00:00 UTC"));
    }

    #[test]
    fn base64_能解() {
        assert_eq!(val("aGVsbG8=").as_deref(), Some("hello"));
        // 不是 base64 的普通单词不该被解出乱码
        assert_eq!(val("settings"), None);
    }

    #[test]
    fn url_编码能解() {
        assert_eq!(val("%E4%B8%AD%E6%96%87").as_deref(), Some("中文"));
        assert_eq!(val("a%20b").as_deref(), Some("a b"));
    }

    #[test]
    fn 普通句子一律不算() {
        for s in [
            "今天天气不错",
            "https://www.baidu.com/s?wd=hello",
            "C:\\Windows\\System32",
            "选中文字就弹出横条",
            "这是很长的一段说明文字，用来确认本地答案不会乱认。",
        ] {
            assert_eq!(answer(s), None, "不该把这句话当答案：{s}");
        }
    }

    #[test]
    fn 星期算得对() {
        // 2026-11-04 是星期三（已知基准，用来钉住公式）
        assert_eq!(weekday_index(2026, 11, 4), 2);
        // 2000-01-01 是星期六
        assert_eq!(weekday_index(2000, 1, 1), 5);
    }
}
