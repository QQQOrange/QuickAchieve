//! 一键诊断文本（v2.23.46：从 `main.rs` 拆出来的第一块）。
//!
//! 为什么先拆它：这段是**纯读取 + 拼字符串**，不碰 STATE、不碰窗口、不发事件，
//! 是 main.rs 里耦合最低的一块 —— 拆出来风险最小，也顺手证明"逻辑出模块、
//! 命令留薄壳"这条路走得通（后面几块照这个模式搬）。
//!
//! 内容口径仍是原来的：**不含任何用户内容**（不读剪贴板、不读选过的文字），
//! 只给版本 / 系统 / 权限 / 数据目录 / 关键开关 / 日志里的异常行。

use crate::config;
use crate::win;
use crate::cfg_snapshot;

/// 生成诊断文本（命令 `diagnostics_text` 的实体）。
pub(crate) fn text() -> String {    let cfg = cfg_snapshot();
    let mut s = String::new();
    let push = |s: &mut String, k: &str, v: &str| {
        s.push_str(&format!("{k}: {v}\n"));
    };

    push(&mut s, "Quick Achieve", config::VERSION);
    let exe = std::env::current_exe().unwrap_or_default();
    let exe_ver = win::exe_file_version(&exe)
        .map(|(a, b, c)| format!("{a}.{b}.{c}"))
        .unwrap_or_else(|| "?".into());
    push(&mut s, "exe 文件版本", &exe_ver);
    push(&mut s, "exe 路径", &exe.display().to_string());

    push(&mut s, "Windows", &win::os_version_string());
    push(&mut s, "架构", std::env::consts::ARCH);
    push(
        &mut s,
        "WebView2",
        &win::webview2_version().unwrap_or_else(|| "未检测到".into()),
    );
    push(&mut s, "管理员权限", if win::is_admin() { "是" } else { "否" });
    push(
        &mut s,
        "提权偏好",
        &match crate::read_local_elevate_pref() {
            Some(true) => "总是以管理员运行".to_string(),
            Some(false) => "从不提权".to_string(),
            None => "未表态（按构建默认）".to_string(),
        },
    );

    let dir = config::data_dir();
    push(&mut s, "数据目录", &dir.display().to_string());
    let cfg_path = config::config_path();
    push(
        &mut s,
        "config.json",
        &format!(
            "{:.1} KB",
            std::fs::metadata(&cfg_path).map(|m| m.len() as f64 / 1024.0).unwrap_or(0.0)
        ),
    );
    let log_path = dir.join("log.txt");
    push(
        &mut s,
        "log.txt",
        &format!(
            "{:.0} KB",
            std::fs::metadata(&log_path).map(|m| m.len() as f64 / 1024.0).unwrap_or(0.0)
        ),
    );
    let bdir = config::backup_dir_of(&cfg);
    let backups = std::fs::read_dir(&bdir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .count()
        })
        .unwrap_or(0);
    push(&mut s, "备份目录", &bdir.display().to_string());
    push(&mut s, "备份份数", &backups.to_string());

    push(&mut s, "界面语言", if cfg.ui_lang.is_empty() { "zh" } else { &cfg.ui_lang });
    push(&mut s, "主题", &cfg.theme);
    push(&mut s, "触发方式", if cfg.trigger_select { "划词" } else { "按住Ctrl" });
    // v2.23.54：Ctrl+C 唤起已移除，诊断里不再有这一项（见 main.rs 键盘分支的注释）
    push(&mut s, "横条格数", &cfg.bar_list.len().to_string());
    push(&mut s, "自定义引擎", &cfg.custom_engines.len().to_string());
    push(&mut s, "外部软件", &cfg.apps.len().to_string());
    push(&mut s, "OCR 引擎", &cfg.ocr_engine);
    push(
        &mut s,
        "网格页",
        &format!(
            "{}（{}）",
            if cfg.grid_enable { "开" } else { "关" },
            cfg.grid_hotkey
        ),
    );
    push(&mut s, "收藏夹热键", &cfg.fav_hotkey);
    push(
        &mut s,
        "WebDAV",
        &format!(
            "{}（启动同步 {}）",
            if cfg.webdav_on_start || cfg.webdav_on_exit { "开" } else { "关" },
            if cfg.webdav_on_start { "是" } else { "否" }
        ),
    );

    /* 日志尾部：只挑"看起来是异常"的行，且**去掉时间戳里的噪声格式**。
     * 最多 12 行 —— 再长用户粘着也费劲。 */
    let tail = std::fs::read_to_string(&log_path).unwrap_or_default();
    let marks = ["失败", "错误", "panic", "PANIC", "异常", "超时", "拒绝", "ERR="];
    let hits: Vec<&str> = tail
        .lines()
        .filter(|l| marks.iter().any(|m| l.contains(m)))
        .rev()
        .take(40)
        .collect();
    if !hits.is_empty() {
        s.push_str("\n-- 日志里最近 40 条异常 --\n");
        for l in hits.iter().rev() {
            s.push_str(l);
            s.push('\n');
        }
    }
    // v2.23.44：**配置能不能写**（放在最后，且**不在上面那个 if 里面**）——
    // 「设置保存不了」是用户报得最多的一类问题，诊断里必须有这一行；
    // 没失败时写"正常"，失败时写具体原因（磁盘满 / 目录只读 / 被占用）。
    let save_err = config::last_save_error();
    push(
        &mut s,
        "配置能否写入",
        if save_err.is_empty() { "正常" } else { save_err.as_str() },
    );
    s
}

/// 复制诊断文本（命令 `copy_diagnostics` 的实体）。
pub(crate) fn copy() -> Result<String, String> {
    let text = text();
    if !crate::clip::set_text(&text) {
        return Err("复制失败（剪贴板被别的程序占用，稍后再试）".into());
    }
    win::log(&format!("diag: 已复制诊断信息（{} 字）", text.chars().count()));
    Ok(text)
}