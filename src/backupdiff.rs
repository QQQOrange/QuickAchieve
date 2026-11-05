//! 备份差异预览的**逻辑**（v2.23.48：从 `main.rs` 搬出的第三块）。
//!
//! 命令本身（`#[crate::t::command] fn backup_diff`）**留在 main.rs 当薄壳**。
//! 上一版我试着把命令一起搬走，结果 `#[command]` 宏在**定义所在模块**生成的
//! `__qs_reg_*` 符号没被 `generate_handler!` 看到，编译直接红。
//! 结论：搬逻辑、留薄壳 —— 注册表永远只认 main.rs 里的那几个名字。

use crate::config;
use crate::{cfg_snapshot, cfg_with};
use serde_json::{json, Value};

/// 恢复前算"会变哪些项"。
pub(crate) fn diff(name: String) -> Result<Value, String> {    if name.trim().is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.contains(':')
    {
        return Err("文件名不合法".into());
    }
    let dir = cfg_with(config::backup_dir_of);
    let p = dir.join(&name);
    if !p.is_file() {
        return Err("这份备份已经不在了（可能被清理过）".into());
    }
    let pw = cfg_snapshot().backup_password;
    let bak = config::read_backup_config(&p, &pw)
        .ok_or_else(|| "这份备份打不开（密码不对或文件已损坏）".to_string())?;
    let cur = cfg_snapshot();
    let mut diffs: Vec<Value> = Vec::new();
    let mut push = |label: &str, from: String, to: String| {
        if from != to {
            diffs.push(json!({ "label": label, "from": from, "to": to }));
        }
    };
    push(
        "横条内容",
        format!("{} 项", cur.bar_list.len()),
        format!("{} 项", bak.bar_list.len()),
    );
    push(
        "自定义引擎",
        format!("{} 个", cur.custom_engines.len()),
        format!("{} 个", bak.custom_engines.len()),
    );
    push(
        "收藏分组",
        format!("{} 个", cur.favorites.len()),
        format!("{} 个", bak.favorites.len()),
    );
    push(
        "外部软件",
        format!("{} 个", cur.apps.len()),
        format!("{} 个", bak.apps.len()),
    );
    push("界面主题", cur.theme.clone(), bak.theme.clone());
    push("界面语言", cur.ui_lang.clone(), bak.ui_lang.clone());
    push("OCR 引擎", cur.ocr_engine.clone(), bak.ocr_engine.clone());
    push(
        "WebDAV 地址",
        if cur.webdav_url.is_empty() { "未填".into() } else { "已填".into() },
        if bak.webdav_url.is_empty() { "未填".into() } else { "已填".into() },
    );
    push(
        "备份密码",
        if cur.backup_password.is_empty() { "未设".into() } else { "已设".into() },
        if bak.backup_password.is_empty() { "未设".into() } else { "已设".into() },
    );
    Ok(json!({ "count": diffs.len(), "items": diffs }))
}
