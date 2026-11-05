//! 剪贴板历史（v2.23.44）。
//!
//! # 为什么单独一个模块、单独一个文件
//!
//! 历史和"配置"是两种东西：
//!
//! * 配置要跟着备份 / WebDAV 同步走，**历史绝对不该**（里面是用户复制过的
//!   密码、验证码、地址 —— 同步到云端等于把它们交出去）；
//! * 所以历史只存在本机数据目录的 `cliphist.json`，设置项也单独存，不进 `config.json`。
//!
//! # 隐私红线（和「复制诊断信息」同一条口径）
//!
//! * **默认关闭**，打开才记；
//! * 只记**文本**，不记图片 / 文件列表；
//! * 单条上限 20,000 字，总数默认 30 条（可调 5~200）；
//! * 提供「不落盘」选项：只在内存里留，退出即清；
//! * 一键清空。
//!
//! # 实现
//!
//! 一个 800ms 的轮询线程：读剪贴板文本 → 和上一条比 → 变了就入队。
//! 不用低级钩子，是因为"读剪贴板"本身就要开剪贴板（钩子里绝对不能干这个）。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// 一条历史。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    pub text: String,
    /// Unix 毫秒
    pub at: i64,
    /// 复制时前台进程名（给界面显示"从哪来"）
    pub app: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct HistCfg {
    pub enabled: bool,
    /// 最多留几条（5~200）
    pub max: usize,
    /// 是否写进磁盘（false = 只在内存里，退出即清）
    pub persist: bool,
}

impl Default for HistCfg {
    fn default() -> Self {
        Self {
            enabled: false, // 隐私相关：出厂必须关
            max: 30,
            persist: false, // 默认**不落盘**：先让用户明确选择
        }
    }
}

fn cfg_slot() -> &'static Mutex<HistCfg> {
    static C: OnceLock<Mutex<HistCfg>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HistCfg::default()))
}

fn buf() -> &'static Mutex<VecDeque<Entry>> {
    static B: OnceLock<Mutex<VecDeque<Entry>>> = OnceLock::new();
    B.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn last_seen() -> &'static Mutex<String> {
    static S: OnceLock<Mutex<String>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(String::new()))
}

fn cfg_path() -> std::path::PathBuf {
    crate::config::data_dir().join("cliphist.json")
}

fn hist_path() -> std::path::PathBuf {
    crate::config::data_dir().join("cliphist-data.json")
}

pub fn cfg() -> HistCfg {
    cfg_slot().lock().map(|g| g.clone()).unwrap_or_default()
}

/// 改设置：立刻生效（含"关掉就清空内存"）。
pub fn set_cfg(c: HistCfg) -> Result<(), String> {
    let mut c = c;
    c.max = c.max.clamp(5, 200);
    {
        let Ok(mut g) = cfg_slot().lock() else {
            return Err("历史设置读不了".into());
        };
        *g = c.clone();
    }
    // 设置单独存（不进 config.json，理由见模块头）
    if let Ok(s) = serde_json::to_vec_pretty(&c) {
        let _ = crate::config::write_atomic(&cfg_path(), &s);
    }
    if !c.enabled {
        // 关掉就把内存里的也清干净 —— "关"必须等于"现在没有"
        if let Ok(mut b) = buf().lock() {
            b.clear();
        }
    }
    /* v2.23.47：**只要不该留盘就删掉**。
     *
     * 原来这里写的是 `if c.persist { remove_file }` —— 关掉历史时如果用户
     * 同时也把"写入磁盘"关了（persist=false），那条件不成立，**旧文件就留在
     * 磁盘上永远不删**。用户以为"我关了、什么都没有"，实际上历史还躺在那儿。
     * 这是隐私问题，不是小事。 */
    if removes_disk_copy(&c) {
        let _ = std::fs::remove_file(hist_path());
    }
    trim(c.max);
    Ok(())
}

/// 这份设置下，磁盘上**不该**留历史文件吗？
///
/// 两种情况都不该留：① 功能关着；② 明确选了"不落盘"。
fn removes_disk_copy(c: &HistCfg) -> bool {
    !c.enabled || !c.persist
}
pub fn list() -> Vec<Entry> {
    buf().lock().map(|b| b.iter().cloned().collect()).unwrap_or_default()
}

pub fn clear() {
    if let Ok(mut b) = buf().lock() {
        b.clear();
    }
    if let Ok(mut s) = last_seen().lock() {
        s.clear();
    }
    let _ = std::fs::remove_file(hist_path());
    crate::win::log("cliphist: 已清空");
}

fn trim(max: usize) {
    {
        let Ok(mut b) = buf().lock() else { return };
        while b.len() > max {
            b.pop_back();
        }
    }
    if cfg().persist {
        save_to_disk();
    }
}

fn save_to_disk() {
    let items: Vec<Entry> = buf()
        .lock()
        .map(|b| b.iter().cloned().collect())
        .unwrap_or_default();
    if let Ok(s) = serde_json::to_vec(&items) {
        let _ = crate::config::write_atomic(&hist_path(), &s);
    }
}

/// 启动：读设置（+ 读回上次的历史，前提是开了"落盘"）。
pub fn init() {
    if let Ok(s) = std::fs::read_to_string(cfg_path()) {
        if let Ok(c) = serde_json::from_str::<HistCfg>(&s) {
            if let Ok(mut g) = cfg_slot().lock() {
                *g = c;
            }
        }
    }
    let c = cfg();
    if c.enabled && c.persist {
        if let Ok(s) = std::fs::read_to_string(hist_path()) {
            if let Ok(items) = serde_json::from_str::<Vec<Entry>>(&s) {
                if let Ok(mut b) = buf().lock() {
                    *b = items.into_iter().take(c.max).collect::<VecDeque<_>>();
                }
            }
        }
    }
}

/// 轮询线程（进程内只起一次）。
pub fn spawn() {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("qs-cliphist".into())
        .spawn(|| loop {
            std::thread::sleep(std::time::Duration::from_millis(800));
            poll_once();
        });
}

fn poll_once() {
    let c = cfg();
    if !c.enabled {
        return;
    }
    let Some(text) = crate::clip::read_text_any_within(1) else {
        return;
    };
    let text = text.trim().to_string();
    if text.is_empty() || text.chars().count() > 20_000 {
        return;
    }
    // 和"上一次看到的"比，而不是和队头比：用户把同一条内容复制两次，
    // 第二次也应该算一次新的操作吗？不算 —— 内容没变就是同一条。
    {
        let Ok(mut seen) = last_seen().lock() else { return };
        if *seen == text {
            return;
        }
        *seen = text.clone();
    }
    let app = crate::win::foreground_exe().unwrap_or_default();
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    {
        let Ok(mut b) = buf().lock() else { return };
        b.push_front(Entry { text, at, app });
        while b.len() > c.max {
            b.pop_back();
        }
    }
    if c.persist {
        save_to_disk();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 出厂默认是关闭且不落盘() {
        let c = HistCfg::default();
        assert!(!c.enabled, "剪贴板历史涉及隐私，出厂必须关");
        assert!(!c.persist, "默认不落盘");
        assert_eq!(c.max, 30);
    }

    /// 关掉功能、或关掉"写入磁盘"，都必须删掉磁盘上的历史文件。
    #[test]
    fn 关掉或改成不落盘都要删掉磁盘副本() {
        let mut c = HistCfg::default();
        c.enabled = true;
        c.persist = true;
        assert!(!removes_disk_copy(&c), "开着且落盘：文件该留");

        c.persist = false;
        assert!(removes_disk_copy(&c), "改成不落盘：文件必须删");

        c.persist = true;
        c.enabled = false;
        assert!(removes_disk_copy(&c), "关掉功能：文件必须删");
    }
    #[test]
    fn 条数上限会被夹在合理区间() {
        let mut c = HistCfg::default();
        c.max = 1;
        set_cfg(c.clone()).ok();
        assert_eq!(cfg().max, 5, "少于 5 条没有意义");
        c.max = 9999;
        set_cfg(c).ok();
        assert_eq!(cfg().max, 200, "上限 200，避免历史无限膨胀");
    }

    #[test]
    fn 入队会按上限裁剪且新的在前() {
        {
            let mut b = buf().lock().unwrap();
            b.clear();
            for i in 0..5 {
                b.push_front(Entry {
                    text: format!("t{i}"),
                    at: i as i64,
                    app: String::new(),
                });
            }
        }
        trim(3);
        let got = list();
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].text, "t4", "最新的在最前面");
    }
}
