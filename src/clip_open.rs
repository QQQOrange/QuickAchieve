//! 「智能直达」：打开剪贴板里最新复制的内容（v2.23.80）。
//!
//! 从 `main.rs` 拆出来的第一块：只依赖 `clip` / `paths` / `engines` / `open`
//! 与日志，不碰窗口、不碰命令注册 —— 命令壳留在 `main.rs`，这里放业务。
//!
//! 判据顺序（先"真实存在的东西"，再"看起来像网址"）：
//! ① `CF_HDROP`：资源管理器里复制的文件 / 文件夹，最明确的意图；
//! ② 文本是一条真实存在的路径（文件 → 默认程序；文件夹 → 打开目录）；
//! ③ 文本是一条网址（`engines::text_to_url`）；
//! ④ 其它一律拒绝，报「不是 文件、文件夹、网址，无法打开」。
//!
//! ⚠ ② 必须在 ③ 之前：`C:\a\b.com` 同时像路径和网址，磁盘上真实存在的
//!    路径要优先，否则用户复制一个叫 `b.com` 的本地文件，点下去却被浏览器
//!    打开 —— 打不开、还找不到原因。

use crate::{clip, engines, paths, win};

pub(crate) fn open_clipboard_target() -> Result<String, String> {
    let bad = || "不是 文件、文件夹、网址，无法打开".to_string();

    /* v2.23.81：**一次开门读全**（文件 + 文本），并允许一次延迟重试。
     *
     * 以前是 `read_files_any` + `read_text_any_within` 两次开门；中间目标
     * 程序刚好在分段写剪贴板时，就会出现"文件没读到、文本也没有"的窗口，
     * 用户看到的是「有时候点了没反应，再点一次又好了」。 */
    let mut c = clip::read_direct_content(25); // 25 × 10ms ≈ 250ms
    let text_empty = c
        .text
        .as_deref()
        .map(|t| t.trim().is_empty())
        .unwrap_or(true);
    if c.opened && c.files.is_empty() && text_empty {
        /* 再读一次：有些宿主先写一个占位、过一会儿才换成真正内容。
         * 不在这里 sleep —— 同步命令跑在主线程上（见
         * `sync_commands_must_not_block_the_main_thread`），重试窗口交给
         * `read_direct_content` 自己的 10ms 开门重试。 */
        let again = clip::read_direct_content(15);
        if again.opened {
            c = again;
        }
    }
    if !c.opened {
        win::log(&format!(
            "clip-direct: 剪贴板一直打不开（{}）",
            clip::describe_formats()
        ));
        return Err("剪贴板被别的程序占着，稍等一下再点这一格".into());
    }

    // ① 资源管理器里复制的是文件 / 文件夹
    if let Some(first) = c.files.first() {
        let p = std::path::Path::new(first);
        if !p.exists() {
            win::log(&format!("clip-direct: 剪贴板里的位置已不存在 -> {first}"));
            return Err(bad());
        }
        let msg = if p.is_dir() {
            paths::open(first)
        } else {
            paths::open_file(first)
        };
        win::log(&format!(
            "clip-direct: 剪贴板文件 {} 个，打开第一个 -> {msg}",
            c.files.len()
        ));
        return if is_open_failure(&msg) { Err(msg) } else { Ok(msg) };
    }

    // ② / ③ 文本：先看真实存在的路径，再看网址
    let Some(text) = c.text else {
        win::log(&format!(
            "clip-direct: 剪贴板里没有文件，也没有文本（{}）",
            clip::describe_formats()
        ));
        return Err(bad());
    };
    let text = text
        .trim()
        .trim_matches(|c: char| c == '"' || c == '\'' || c.is_whitespace())
        .trim()
        .to_string();
    if text.is_empty() {
        return Err(bad());
    }

    if paths::looks_like_path(&text).is_some() {
        if let Some(p) = paths::resolve(&text) {
            let pb = std::path::Path::new(&p);
            if pb.exists() {
                let msg = if pb.is_dir() {
                    paths::open(&p)
                } else {
                    paths::open_file(&p)
                };
                win::log(&format!("clip-direct: 剪贴板文本是本地路径 -> {msg}"));
                return if is_open_failure(&msg) { Err(msg) } else { Ok(msg) };
            }
        }
    }

    if let Some(u) = engines::text_to_url(&text) {
        let brief: String = u.chars().take(120).collect();
        // `that_detached` 是"发起后立刻返回"的版本，不会卡住命令线程；
        // 失败要把原因抛给前端，不能像以前那样静默吞掉。
        match open::that_detached(u.as_str()) {
            Ok(()) => {
                win::log(&format!("clip-direct: 打开网址 {brief}"));
                return Ok(format!("已在浏览器打开：{brief}"));
            }
            Err(e) => {
                let why: String = e.to_string().chars().take(120).collect();
                win::log(&format!("clip-direct: 打开网址失败（{brief}）：{why}"));
                return Err(format!("打开网址失败：{why}"));
            }
        }
    }

    win::log(&format!(
        "clip-direct: 剪贴板文本既不是路径也不是网址（{} 字，格式 {}）",
        text.chars().count(),
        clip::describe_formats()
    ));
    Err(bad())
}

/// `paths::open` / `paths::open_file` 返回的是"给人看的文案"，不是 Result。
/// 这里按它们的失败前缀判一下，把失败变成 `Err` —— 否则前端会当成打开成功、
/// 收起网格，用户看到的就是"点了没反应"。
pub(crate) fn is_open_failure(msg: &str) -> bool {
    ["打开失败", "找不到这个路径", "找不到这个文件", "这个文件没有关联程序", "这不像"]
        .iter()
        .any(|p| msg.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::is_open_failure;

    /// v2.23.81：`open_clipboard_target` 把 `paths::open*` 的返回文案分成
    /// 成功 / 失败两种。分错的话，打不开的文件会被前端当成成功、收起网格，
    /// 用户看到的就是「点了没反应」——
    /// 所以这几个前缀必须与 `paths.rs` 里的 return 语句保持一致。
    #[test]
    fn open_failure_messages_are_detected() {
        assert!(is_open_failure("打开失败：拒绝访问"));
        assert!(is_open_failure("找不到这个路径：C:\\nope"));
        assert!(is_open_failure("找不到这个文件：C:\\nope.txt"));
        assert!(is_open_failure(
            "这个文件没有关联程序：a.xyz —— 右键它选「打开方式」设一个"
        ));
        assert!(!is_open_failure("已在文件管理器中打开"));
        assert!(!is_open_failure("已在文件管理器中定位"));
        assert!(!is_open_failure("已用默认程序打开：a.txt"));
        assert!(!is_open_failure("已在浏览器打开：https://example.com"));
    }
}
