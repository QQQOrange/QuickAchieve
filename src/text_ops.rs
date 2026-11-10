//! 纯文本处理：包裹 / 拆包 / 换对，以及「智能去空格 / 标点全半角 /
//! 标题大小写 / 句首大写」这些 `@txt:<op>` 操作（v2.23.95 从 main 拆出）。
//!
//! 这里只放**不碰窗口、不碰剪贴板**的纯函数；`wrap_text` / `text_op` 两个
//! 命令、回写目标软件、剪贴板备份还原仍在 main。


/// 支持哪些符号对：`(key, 左, 右, 中文名)`。
///
/// **这份表是唯一事实来源** —— 前端通过 `wrap_pairs()` 拿，菜单里的顺序和
/// 显示名都来自这里，不在 JS 里再抄一份（抄的那份迟早会跑偏）。
pub(crate) const WRAP_PAIRS: &[(&str, &str, &str, &str)] = &[
    ("double", "“", "”", "双引号"),
    ("single", "‘", "’", "单引号"),
    ("corner", "「", "」", "直角引号"),
    ("title", "《", "》", "书名号"),
    ("angle", "〈", "〉", "单书名号"),
    ("paren", "（", "）", "圆括号"),
    ("square", "[", "]", "方括号"),
    ("curly", "{", "}", "花括号"),
    ("lenticular", "【", "】", "方头括号"),
    ("ascii_double", "\"", "\"", "英文双引号"),
    ("ascii_single", "'", "'", "英文单引号"),
    ("backtick", "`", "`", "反引号"),
];


/// 去掉最外层一对符号。`""`（只剩符号本身）不算被包过 —— 否则会一路拆到空。
pub(crate) fn wrap_strip<'a>(s: &'a str, l: &str, r: &str) -> Option<&'a str> {
    if s.len() <= l.len() + r.len() {
        return None;
    }
    let inner = s.strip_prefix(l)?;
    inner.strip_suffix(r)
}


/// 核心逻辑（纯函数，好测）：返回 `(结果文本, 动作)`。
///
/// 动作只有三种，前端据此显示不同提示：
///   * `wrap`   包上了一层
///   * `unwrap` 把**同一对**拆掉（套了几层就拆几层 = 去重）
///   * `swap`   外层原来是别的符号，换成了这一对
///
/// 首尾空白原样保留在符号**外面**：用户选中的常常是带换行/缩进的一整段，
/// 去掉空白再包会把这些版面信息弄丢。
pub(crate) fn wrap_apply(text: &str, key: &str) -> Result<(String, &'static str), String> {
    let Some(&(_, l, r, _)) = WRAP_PAIRS.iter().find(|(k, ..)| *k == key) else {
        return Err(format!("不认识的符号类型：{key}"));
    };

    /* 先切出"去掉首尾空白"的那一段。⚠ 别用 trim_start/trim_end 两次算区间：
     * 全空白时前者给出 len、后者给出 0，区间会反着来（5..0 直接 panic）。 */
    let core = text.trim();
    if core.is_empty() {
        return Err("还没有选中文字".to_string());
    }
    let start = text.len() - text.trim_start().len();
    let end = start + core.len();
    let lead = &text[..start];
    let trail = &text[end..];

    // ① 已经是同一对 → 全部拆掉（这同时解决了"套了两层"的去重问题）
    let mut cur = core;
    let mut layers = 0usize;
    while let Some(inner) = wrap_strip(cur, l, r) {
        cur = inner;
        layers += 1;
    }
    if layers > 0 {
        return Ok((format!("{lead}{cur}{trail}"), "unwrap"));
    }

    // ② 外面包着**另一对** → 直接换掉那一层，不再叠
    for (_, ol, or, _) in WRAP_PAIRS {
        if *ol == l && *or == r {
            continue;
        }
        if let Some(inner) = wrap_strip(core, ol, or) {
            return Ok((format!("{lead}{l}{inner}{r}{trail}"), "swap"));
        }
    }

    // ③ 什么都没包 → 包一层
    Ok((format!("{lead}{l}{core}{r}{trail}"), "wrap"))
}


/// 中日韩文字：用来判断"两个汉字之间的空格该不该删"。
pub(crate) fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x3040..=0x30FF
        | 0xAC00..=0xD7AF | 0x3000..=0x303F | 0xFF00..=0xFFEF)
}


/// 中文标点（去空格时，它两侧的空格也要清掉）。
pub(crate) fn is_cjk_punct(c: char) -> bool {
    matches!(c, '，' | '。' | '、' | '；' | '：' | '？' | '！' | '“' | '”' | '‘' | '’'
        | '（' | '）' | '《' | '》' | '【' | '】' | '…' | '—' | '·')
}


/// 智能去空格：折叠连续空格、去掉行首行尾空白、去掉**汉字/中文标点之间**的空格；
/// 英文单词之间、单词与中文之间的那个空格**保留**（删了英文就粘成一坨了）。
pub(crate) fn text_op_trim(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for (li, line) in src.lines().enumerate() {
        if li > 0 {
            out.push('\n');
        }
        let chars: Vec<char> = line.trim().chars().collect();
        let mut prev_kept: Option<char> = None;
        for (i, &c) in chars.iter().enumerate() {
            if c == ' ' || c == '\t' || c == '\u{3000}' {
                let next = chars.get(i + 1).copied();
                let left_cjk = prev_kept.map(|p| is_cjk(p) || is_cjk_punct(p)).unwrap_or(false);
                let right_cjk = next.map(|n| is_cjk(n) || is_cjk_punct(n)).unwrap_or(false);
                // 汉字之间 / 汉字与中文标点之间 / 行尾 → 直接吃掉
                if next.is_none() || (left_cjk && right_cjk) {
                    continue;
                }
                if prev_kept == Some(' ') {
                    continue; // 连续空格折叠成一个
                }
                out.push(' ');
                prev_kept = Some(' ');
                continue;
            }
            out.push(c);
            prev_kept = Some(c);
        }
    }
    // 三个以上的连续换行压成两个（保留"空一行 = 分段"的语义）
    let mut res = String::with_capacity(out.len());
    let mut nl_run = 0usize;
    for c in out.chars() {
        if c == '\n' {
            nl_run += 1;
            if nl_run <= 2 {
                res.push(c);
            }
        } else {
            nl_run = 0;
            res.push(c);
        }
    }
    res
}


/// 标点全角 / 半角互转。
///
/// 只动**成对的中英标点**，刻意不碰：
///   · `.` —— 全角化成 `。` 会把 3.14、www.a.com 一起毁掉；
///   · `。` 半角化只在**后面不是数字**时才换成 `.`（同样的理由）。
pub(crate) fn text_op_punct(src: &str, to_full: bool) -> String {
    const PAIRS: &[(char, char)] = &[
        (',', '，'), (';', '；'), (':', '：'), ('!', '！'), ('?', '？'),
        ('(', '（'), (')', '）'), ('[', '【'), (']', '】'),
    ];
    /* 引号**不能**走上面那张一对一表：
     *
     *   · 全角化时两个半角 `"` 都会落到同一个 U+201C → `他说"你好"` 变成
     *     `他说“你好“`（收尾引号成了开引号）；
     *   · 反向更糟：表里只有 U+201C/U+2019，没有 U+201D/U+2018，
     *     于是 `他说“你好”` 转半角只改左边那一个（`"你好”`）。
     *
     * 所以双引号按**出现次序**交替成对；单引号保持"一律 ’"（英文撇号
     * don't / it's 走的是同一条路，交替反而会把它变成开引号）。
     * 反向把弯引号一律收回半角 —— 这才是能往返的映射。 */
    let mut dq_open = true;
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    for (i, &c) in chars.iter().enumerate() {
        if to_full {
            if c == '"' {
                out.push(if dq_open { '“' } else { '”' });
                dq_open = !dq_open;
                continue;
            }
            if c == '\'' {
                out.push('’');
                continue;
            }
        } else if matches!(c, '“' | '”' | '‘' | '’') {
            out.push(if matches!(c, '“' | '”') { '"' } else { '\'' });
            continue;
        }
        let rep = if to_full {
            PAIRS.iter().find(|(h, _)| *h == c).map(|(_, f)| *f)
        } else {
            PAIRS.iter().find(|(_, f)| *f == c).map(|(h, _)| *h)
        };
        if let Some(r) = rep {
            out.push(r);
            continue;
        }
        // 句号：半角化时后面跟着数字就保持全角（"第 3。5 节"这种极少，但 3.14 很常见）
        if !to_full && c == '。' {
            let next_digit = chars.get(i + 1).map(|n| n.is_ascii_digit()).unwrap_or(false);
            out.push(if next_digit { '。' } else { '.' });
            continue;
        }
        out.push(c);
    }
    out
}


/// 每个单词的首字母大写（英文单词；中文/数字原样）。
pub(crate) fn text_op_title(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut at_start = true;
    for c in src.chars() {
        if c.is_alphabetic() {
            if at_start {
                out.extend(c.to_uppercase());
                at_start = false;
            } else {
                out.push(c);
            }
        } else {
            at_start = !c.is_alphanumeric();
            out.push(c);
        }
    }
    out
}


/// 句首字母大写：按 `. ! ?`（含中文全角）和换行断句，每句第一个字母大写。
pub(crate) fn text_op_sentence(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut start = true;
    for c in src.chars() {
        if start && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            start = false;
            continue;
        }
        out.push(c);
        if matches!(c, '.' | '!' | '?' | '。' | '！' | '？' | '\n') {
            start = true;
        } else if !c.is_whitespace() && c.is_alphanumeric() {
            start = false;
        }
    }
    out
}


/// 执行一条文本处理。返回 (结果, 中文动作名)。
pub(crate) fn apply_text_op(op: &str, src: &str) -> Result<(String, &'static str), String> {
    let (out, name) = match op {
        "trim" => (text_op_trim(src), "智能去空格"),
        "nl" => (unwrap_lines(src), "智能换行"),
        "punct:full" => (text_op_punct(src, true), "标点转全角"),
        "punct:half" => (text_op_punct(src, false), "标点转半角"),
        "case:lower" => (src.to_lowercase(), "英文转小写"),
        "case:upper" => (src.to_uppercase(), "英文转大写"),
        "case:title" => (text_op_title(src), "首字母大写"),
        "case:sentence" => (text_op_sentence(src), "句首大写"),
        other => return Err(format!("不认识的文本处理：{other}")),
    };
    Ok((out, name))
}



/// 合并硬换行：把"因为排版被断开"的行接回同一段。
///
/// 用户原话：「选中的本来是一段话，但因为显示而分为好几段」—— PDF 复制、
/// 窄栏网页、扫描件的 OCR 都会这样。
///
/// 判据**保守**，宁可少接也不接错：
///   · 上一行以句末标点收尾 → 是完整句子，不接；
///   · 上一行或当前行是"结构行"（列表 / 编号 / 引用 / 表格 / 标题）→ 不接
///     （这些行的换行是**语义**，接起来会把列表糊成一坨）；
///   · 空行是段落分隔 → 不接。
/// 接的时候：中↔中直接贴；只要有一边是 ASCII 字母数字就补一个空格
/// （不然 "hello" + "world" 会粘成 "helloworld"）。
pub(crate) fn unwrap_lines(s: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in s.split('\n') {
        let cur = line.trim();
        if cur.is_empty() {
            out.push(String::new());
            continue;
        }
        let prev_ok = match out.last() {
            Some(p) => {
                let p = p.trim_end();
                !p.is_empty() && !ends_sentence(p) && !is_structural_line(p)
            }
            None => false,
        };
        if prev_ok && !is_structural_line(cur) {
            let last = out.last_mut().unwrap();
            let a = last.chars().last().unwrap_or(' ');
            let b = cur.chars().next().unwrap_or(' ');
            if needs_space_between(a, b) {
                last.push(' ');
            }
            last.push_str(cur);
        } else {
            out.push(cur.to_string());
        }
    }
    out.join("\n")
}


/// 这一行是不是以句末标点收尾。
/// 右引号/右括号收尾时看它**前面**那个字符（`他说：“好的。”` 这种）。
fn ends_sentence(s: &str) -> bool {
    let t = s.trim_end();
    match t.chars().last() {
        Some('”') | Some('」') | Some('』') | Some('）') | Some(')') | Some('"') | Some('’') => {
            t.chars().rev().nth(1).map(ends_punct).unwrap_or(false)
        }
        Some(c) => ends_punct(c),
        None => false,
    }
}


/// 这一行是不是"换行有语义"的结构行：列表项、编号、引用、表格、标题。
fn is_structural_line(s: &str) -> bool {
    let t = s.trim();
    let mut it = t.chars();
    let Some(first) = it.next() else {
        return false;
    };
    if matches!(
        first,
        '-' | '*' | '+' | '·' | '•' | '#' | '>' | '|' | '[' | '【' | '〔'
    ) {
        return true;
    }
    // 表格行：整行两个以上竖线
    if t.matches('|').count() >= 2 {
        return true;
    }
    // 编号：`1.` `2、` `3)` `(4)` `①`
    if first.is_ascii_digit() || (0x2460..=0x2473).contains(&(first as u32)) {
        if let Some(second) = it.next() {
            if matches!(second, '.' | '、' | ')' | '）' | ':' | '：') {
                return true;
            }
        }
    }
    // 中文数字编号：一、二、三…十、
    if matches!(
        first,
        '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十'
    ) {
        if let Some(second) = it.next() {
            if matches!(second, '、' | '.' | '）' | ')') {
                return true;
            }
        }
    }
    false
}


/// 拼接两行时中间要不要补空格：只要有一边是 ASCII 字母/数字就要
/// （中文之间直接贴；中文↔英文补一个空格 —— 那是更好的排版）。
fn needs_space_between(a: char, b: char) -> bool {
    a.is_ascii_alphanumeric() || b.is_ascii_alphanumeric()
}


fn ends_punct(c: char) -> bool {
    matches!(c, '。' | '！' | '？' | '；' | '：' | '.' | '!' | '?' | ';' | ':' | '…')
}

