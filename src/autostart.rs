//! 开机自启（v2.23.117 重写）。
//!
//! # 为什么重写
//!
//! 用户报「设置里明明打开了开机自启，重启后却没有起来」。旧的实现有三个
//! 结构性问题，任何一个都足以让它"看起来开着、实际不开"：
//!
//!   ① **只写一次**。旧代码只在设置被改动时写 `HKCU\...\Run`；启动时那句
//!      补写被 `QS_LITE_PORTABLE` 整段跳过（本仓库正是 `true`）。于是
//!      "注册表被优化软件删了 / 被另一份副本顶掉 / 路径变了"这些状态
//!      **永远修不回来**，而设置页还一直显示"开"。
//!   ② **写完不校验**。`RegSetValueExW` 返回成功就当成功。组策略、安全
//!      软件、注册表重定向都可能让写入落空，用户看到的是"开着但不自启"。
//!   ③ **提权与自启打架**。本程序默认以管理员运行，而 `Run` 项只能拉起
//!      普通权限的实例，起来之后还要 `ShellExecuteW("runas")` 二次提权。
//!      开机那一刻这一跳可能被 UAC 拦住、被安全软件打断、或者用户不在
//!      电脑前 —— 表现就是"没自启"。而"任务管理器 → 启动"里还能把
//!      `Run` 项单独禁用，程序自己却看不出来。
//!
//! # 现在的设计
//!
//! * **两种落地方式**，按"要不要提权"自动选：
//!     - 需要提权 → **计划任务**（`ONLOGON` + `HIGHEST`）：登录即以管理员
//!       身份启动，一次 UAC 都不弹；
//!     - 不需要提权 → `HKCU\...\Run`（不需要管理员，最轻）。
//! * 两条路都**写完立刻读回校验**，结果如实回给界面（设置页不再骗人）。
//! * **每次启动自我修复一次**（不再被 `QS_LITE_PORTABLE` 挡住），并把实际
//!   生效状态写进日志与「一键诊断」。
//! * 启动项被任务管理器禁用（`StartupApproved\Run` 首字节 3）时能识别出来，
//!   并在打开开关时顺手清掉那个标记。
//!
//! 逃生口：`QS_SKIP_AUTOSTART=1`（测试副本 / 便携副本用）——只跳过**写**，
//! 读与诊断照常。

use crate::win;
use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};

/// 自启项名（注册表值名 / 计划任务名）。改名前的名字见 [`RUN_VALUE_LEGACY`]。
pub const RUN_VALUE: &str = "QuickAchieve";
/// 更名（v2.15.0）前的自启项名。老项目指向一个**已经不存在的 exe**，每次
/// 写/删自启时顺手清掉，别留一条永远删不掉的僵尸项。
pub const RUN_VALUE_LEGACY: &str = "QuickSearch";
/// 计划任务名。和注册表值同名，方便用户在"任务计划程序"里一眼认出来。
pub const TASK_NAME: &str = "QuickAchieve";
/// 自启拉起时带的参数。
///
/// 用途有三：① 日志里能分清"开机拉起"和"用户双击"；② 提权重启时把它
/// 原样带过去（`QS_BOOT` → `relaunch_as_admin` 会补这个参数）；③ 开机
/// 那一刻先等 shell 就绪再建窗，避免托盘图标挂不上。
pub const BOOT_ARG: &str = "--autostart";

/* ------------------------------------------------------------------ */
/* 状态                                                                */
/* ------------------------------------------------------------------ */

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mechanism {
    /// 没启用（或写了但没生效）。
    None,
    /// `HKCU\...\Run`。
    RunKey,
    /// 计划任务（登录时运行，最高权限）。
    Task,
}

impl Mechanism {
    pub fn id(self) -> &'static str {
        match self {
            Mechanism::None => "none",
            Mechanism::RunKey => "run",
            Mechanism::Task => "task",
        }
    }
}

/// 一次"自启状态"快照。**只读**，不修改任何东西。
#[derive(Clone, Debug)]
pub struct Status {
    /// 配置里想不想开（`cfg.auto_start`）。
    pub desired: bool,
    /// 真的能自启吗（注册表项指向本 exe，或者计划任务存在）。
    pub active: bool,
    pub mechanism: Mechanism,
    /// 注册表里的原始字符串（诊断用）。
    pub run_value: Option<String>,
    /// 注册表项指向的是不是**这一份** exe。
    pub run_points_here: bool,
    /// 任务管理器「启动」页里被关掉了。
    pub run_disabled: bool,
    pub task_exists: bool,
    /// 计划任务指向的是不是**这一份** exe（任务留着但 exe 搬走了 = 失效）。
    pub task_points_here: bool,
    /// 计划任务里实际的命令行（诊断用）。
    pub task_command: Option<String>,
    /// 这次进程有没有管理员权限（决定能不能建计划任务）。
    pub elevated: bool,
    /// 最近一次"动手"失败的原因（读状态时为 None）。
    pub error: Option<String>,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            desired: false,
            active: false,
            mechanism: Mechanism::None,
            run_value: None,
            run_points_here: false,
            run_disabled: false,
            task_exists: false,
            task_points_here: false,
            task_command: None,
            elevated: false,
            error: None,
        }
    }
}

impl Status {
    /// 一句话人话（日志 / 诊断 / 设置页提示都用它）。
    pub fn summary(&self) -> String {
        if self.active {
            let how = match self.mechanism {
                Mechanism::Task => "计划任务（登录时以最高权限运行，无 UAC）",
                Mechanism::RunKey => "注册表启动项 HKCU\\...\\Run",
                Mechanism::None => "?",
            };
            return format!("已生效：{how}");
        }
        if self.desired {
            let mut why = Vec::new();
            if let Some(v) = &self.run_value {
                if !self.run_points_here {
                    why.push(format!("启动项指向别处（{v}）"));
                }
            } else {
                why.push("启动项不存在".to_string());
            }
            if self.task_exists && !self.task_points_here {
                why.push(format!(
                    "计划任务指向别处（{}）",
                    self.task_command.as_deref().unwrap_or("读不出命令行")
                ));
            }
            if self.run_disabled {
                why.push("被任务管理器「启动」页禁用".to_string());
            }
            if let Some(e) = &self.error {
                why.push(e.clone());
            }
            if why.is_empty() {
                why.push("未知原因".to_string());
            }
            return format!("未生效：{}", why.join("；"));
        }
        "未启用".to_string()
    }

    pub fn to_json(&self) -> Value {
        json!({
            "desired": self.desired,
            "active": self.active,
            "mechanism": self.mechanism.id(),
            "pointsHere": self.run_points_here || self.task_points_here,
            "disabledInTaskManager": self.run_disabled,
            "task": self.task_exists,
            "elevated": self.elevated,
            "error": self.error,
            "summary": self.summary(),
        })
    }
}

static LAST: OnceLock<Mutex<Option<Status>>> = OnceLock::new();

fn last_slot() -> &'static Mutex<Option<Status>> {
    LAST.get_or_init(|| Mutex::new(None))
}

fn remember(st: &Status) {
    if let Ok(mut g) = last_slot().lock() {
        *g = Some(st.clone());
    }
}

/// 最近一次算出来的状态（给 `config_view` / 诊断用，**不再查注册表**）。
pub fn cached() -> Option<Status> {
    last_slot().lock().ok().and_then(|g| g.clone())
}

/// 设置页要的那一小块 JSON。没算过就返回 `null`（前端按"未知"处理）。
pub fn ui_state() -> Value {
    match cached() {
        Some(s) => s.to_json(),
        None => Value::Null,
    }
}

/* ------------------------------------------------------------------ */
/* 小工具                                                              */
/* ------------------------------------------------------------------ */

/// 当前 exe 的完整路径（拿不到就返回空串）。
fn exe_path() -> String {
    let raw = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    abs_path(&raw)
}

/// 把路径变成绝对、无 `.` / `..` 分量的写法。
///
/// 从 `.\QuickAchieve.exe` 这种相对路径启动时，`current_exe()` 会原样带回
/// `D:\dir\.\QuickAchieve.exe`。这个字符串一旦被写进注册表或计划任务，
/// 登录时的工作目录并不是 exe 目录 —— 相对命令必然起不来；而且跟任务里
/// 存的干净路径比较也会误判成"不是本副本"，每次启动都白重写一遍。
fn abs_path(p: &str) -> String {
    if p.is_empty() {
        return String::new();
    }
    match std::fs::canonicalize(p) {
        Ok(real) => {
            let s = real.to_string_lossy().to_string();
            s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
        }
        Err(_) => p.to_string(),
    }
}

/// 自启要写的命令行。
///
/// 一定要给路径加引号：`D:\Program Files\...` 这种带空格的路径不加引号，
/// 登录时 Windows 会按空格切分，第一段找不到就静默失败 —— 这是"开了自启
/// 却没起来"最常见的一种。
pub fn command_line(exe: &str) -> String {
    format!("\"{exe}\" {BOOT_ARG}")
}

/// 从注册表值里把 exe 路径拆出来（支持带引号 / 不带引号 / 带参数）。
///
/// 只做字符串处理，方便单测；真正比较前会 [`normalize_path`]。
pub fn exe_of(value: &str) -> String {
    let v = value.trim();
    if let Some(rest) = v.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            return rest[..end].to_string();
        }
        return rest.to_string();
    }
    /* 不带引号时，路径本身可能带空格。这里按"最后一个 .exe 结尾"截取，
     * 没有 .exe 就退回第一个空格前的片段。 */
    let lower = v.to_ascii_lowercase();
    if let Some(i) = lower.find(".exe") {
        return v[..i + 4].to_string();
    }
    v.split_whitespace().next().unwrap_or("").to_string()
}

/// 路径比较用的归一化：斜杠统一、去掉首尾空白、大小写不敏感（Windows）。
///
/// 还会折叠 `.` / `..`：`D:\dir\.\a.exe` 与 `D:\dir\a.exe` 必须判等，
/// 否则从相对路径启动时自启状态会误报"未生效"。
pub fn normalize_path(p: &str) -> String {
    let lowered = p
        .trim()
        .trim_matches('"')
        .replace('/', "\\")
        .to_lowercase();
    let stripped = lowered.strip_prefix(r"\\?\").unwrap_or(&lowered).to_string();
    match std::fs::canonicalize(&stripped) {
        Ok(real) => {
            let real = real.to_string_lossy().replace('/', "\\").to_lowercase();
            real.strip_prefix(r"\\?\").unwrap_or(&real).to_string()
        }
        Err(_) => stripped,
    }
}

/// 允许测试用别的值名，避免单测去动用户真正的自启项。
///
/// ⚠ 单测里**不要**用 `std::env::set_var` 来传这个（旧写法）：测试是多线程并行跑的，
///   而 `set_var` 会改写进程环境块 —— 别的线程正在 `env::var` 时就是一次真实的
///   数据竞争（实测能把测试进程干成 STATUS_HEAP_CORRUPTION）。
///   所以这里留一个只在测试构建里存在的槽位。
fn value_name() -> String {
    #[cfg(test)]
    if let Ok(g) = test_value_slot().lock() {
        if let Some(v) = g.as_ref() {
            return v.clone();
        }
    }
    std::env::var("QS_AUTOSTART_VALUE")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| RUN_VALUE.to_string())
}

#[cfg(test)]
fn test_value_slot() -> &'static Mutex<Option<String>> {
    static S: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

fn task_name() -> String {
    #[cfg(test)]
    if let Ok(g) = test_task_slot().lock() {
        if let Some(v) = g.as_ref() {
            return v.clone();
        }
    }
    std::env::var("QS_AUTOSTART_TASK")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| TASK_NAME.to_string())
}

#[cfg(test)]
fn test_task_slot() -> &'static Mutex<Option<String>> {
    static S: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

fn skipped() -> bool {
    std::env::var("QS_SKIP_AUTOSTART").map(|v| v == "1").unwrap_or(false)
}

/// 这是不是一个**测试 / 探针实例**（自动化脚本用 `QS_INSTANCE_TAG` 起的那一份）。
///
/// 为什么要区分：测试实例常常用 `QS_DATA_DIR` 指向一份临时配置（`autoStart`
/// 默认 false），一启动就会走到"配置里关着 → 清理残留启动项"那条路 ——
/// 而注册表是**全用户共享**的，于是它会把**正式版**的启动项删掉。
///
/// 这个坑真的踩了（v2.23.117 自测时，一份临时实例把用户的开机自启删了）。
/// 现在的规则：
///   · 测试实例**永远不碰**指向别的 exe 的启动项 / 计划任务；
///   · 普通实例（用户自己那一份）照常按配置管理。
fn test_instance() -> bool {
    std::env::var("QS_INSTANCE_TAG")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

/// 一段自启命令行指向的 exe 是不是**另一个存在的文件**（= 另一份副本）。
///
/// 指向一个已经不存在的文件（僵尸项）不算"别的副本"—— 那种正是该清的。
fn points_to_other_copy(value: &str, here: &str) -> bool {
    let target = exe_of(value);
    if target.is_empty() || normalize_path(&target) == normalize_path(here) {
        return false;
    }
    !is_zombie_command(value)
}

/// 命令行的目标文件是不是已经不在了（僵尸启动项：删了 exe / 换了目录）。
fn is_zombie_command(value: &str) -> bool {
    let target = exe_of(value);
    if target.is_empty() {
        return false;
    }
    !std::path::Path::new(target.trim_matches('"')).exists()
}

fn schtasks() -> String {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    format!("{root}\\System32\\schtasks.exe")
}

/* ------------------------------------------------------------------ */
/* 计划任务                                                            */
/* ------------------------------------------------------------------ */

/// 计划任务存在吗（以及它到底跑的是哪个命令行）。
///
/// 只查一次 `schtasks /Query /XML`：返回值告诉我们任务在不在，XML 里的
/// `<Command>` / `<Arguments>` 告诉我们它指向谁 —— 少了后一半就查不出
/// "任务还在、但 exe 已经搬走/换位置"这种情况（那正是"自启失效"的常见形态）。
fn task_probe() -> (bool, Option<String>) {
    let name = task_name();
    match win::run_hidden(&schtasks(), &["/Query", "/TN", &name, "/XML"]) {
        Some((true, out)) => {
            let cmd = xml_field(&out, "Command").unwrap_or_default();
            let args = xml_field(&out, "Arguments").unwrap_or_default();
            let full = format!("{cmd} {args}").trim().to_string();
            (true, if full.is_empty() { None } else { Some(full) })
        }
        _ => (false, None),
    }
}

/// 取 XML 里一个简单标签的文本（`<Command>…</Command>`）。
///
/// 任务计划程序导出的 XML 是固定结构、单行标签，用字符串找就够了 ——
/// 为它拉一个 XML 解析依赖不值得。
fn xml_field(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let i = xml.find(&open)? + open.len();
    let j = xml[i..].find(&close)? + i;
    let v = xml[i..j].trim();
    if v.is_empty() {
        return None;
    }
    /* XML 里 `&` / `<` / `>` / 引号会被转义。不还原的话，路径里带 `&`
     * （`D:\Tools\A&B\QuickAchieve.exe`）会被判成"指向别处"，
     * 于是每次启动都白建一遍计划任务。 */
    Some(
        v.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&"),
    )
}

/// 计划任务存在吗。
pub fn task_exists() -> bool {
    task_probe().0
}

fn task_create() -> Result<(), String> {
    let name = task_name();
    let cmd = command_line(&exe_path());
    let exe = schtasks();
    /* `/SC ONLOGON` + `/RL HIGHEST` + `/IT`：
     *   · 登录时触发（比 Run 项更可靠，Explorer 还没起来也会跑）；
     *   · 最高权限 —— 不需要 UAC 弹窗，这正是旧方案在开机那一刻卡住的地方；
     *   · `/IT` 交互式令牌：不需要存密码，也不需要 `/RU` 指定账户。
     * `/F` 覆盖同名任务，保证"路径变了"时能就地更新。 */
    let args = [
        "/Create", "/F", "/TN", &name, "/TR", &cmd, "/SC", "ONLOGON", "/RL", "HIGHEST", "/IT",
    ];
    match win::run_hidden(&exe, &args) {
        Some((true, _)) => {
            if task_exists() {
                Ok(())
            } else {
                Err("schtasks 报成功，但任务查不到".to_string())
            }
        }
        Some((false, out)) => Err(format!(
            "建计划任务失败（{}）",
            first_line(&out).unwrap_or_else(|| "无输出".to_string())
        )),
        None => Err("调不起 schtasks.exe".to_string()),
    }
}

fn task_delete() -> Result<(), String> {
    let name = task_name();
    if !task_exists() {
        return Ok(());
    }
    match win::run_hidden(&schtasks(), &["/Delete", "/F", "/TN", &name]) {
        Some((true, _)) => Ok(()),
        Some((false, out)) => Err(format!(
            "删计划任务失败（{}）",
            first_line(&out).unwrap_or_else(|| "无输出".to_string())
        )),
        None => Err("调不起 schtasks.exe".to_string()),
    }
}

fn first_line(s: &str) -> Option<String> {
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| l.chars().take(160).collect())
}

/* ------------------------------------------------------------------ */
/* 读状态                                                              */
/* ------------------------------------------------------------------ */

/// 查一次真实状态（读注册表 + 查计划任务）。
///
/// ⚠ 会起一次 `schtasks /Query`（约 30~80 ms），**别放进每次配置广播里**：
///   广播的热路径用 [`ui_state`]（读缓存）。这里只在设置页打开、诊断、
///   改设置、启动自检时调。
pub fn status() -> Status {
    let value = win::hkcu_get_sz(win::RUN_KEY, &value_name());
    let here = exe_path();
    let points = value
        .as_deref()
        .map(|v| normalize_path(&exe_of(v)) == normalize_path(&here))
        .unwrap_or(false);
    let (task, task_cmd) = task_probe();
    let task_here = task_cmd
        .as_deref()
        .map(|c| normalize_path(&exe_of(c)) == normalize_path(&here))
        .unwrap_or(false);
    let st = Status {
        desired: false, // 由调用方填
        active: points || task_here,
        mechanism: if task_here {
            Mechanism::Task
        } else if points {
            Mechanism::RunKey
        } else {
            Mechanism::None
        },
        run_value: value,
        run_points_here: points,
        run_disabled: win::startup_approved_disabled(&value_name()),
        task_exists: task,
        task_points_here: task_here,
        task_command: task_cmd,
        elevated: win::is_admin(),
        error: None,
    };
    remember(&st);
    st
}

/* ------------------------------------------------------------------ */
/* 写状态                                                              */
/* ------------------------------------------------------------------ */

/// 按"要不要提权"选一种落地方式并写下去，写完**读回校验**。
///
/// * `on = true`：需要提权且当前是管理员 → 建计划任务（失败再退回 Run 项）；
///   否则写 `HKCU\...\Run`。
/// * `on = false`：两条都清掉。
pub fn apply(on: bool, want_elevate: bool) -> Status {
    /* 先摸一遍现状（注册表 + 计划任务）。这一步不是多余的：
     * "关掉提权开关时要顺手删掉上一个版本建的计划任务"全靠它 ——
     * 以前这里用的是 `Status::default()`，`task_exists` 恒 false，
     * 于是"关提权"之后任务还在，开机照样以管理员启动。 */
    let mut st = status();
    st.desired = on;
    st.error = None;
    st.elevated = win::is_admin();
    if skipped() {
        st.error = Some("QS_SKIP_AUTOSTART=1：本次不写启动项".to_string());
        st.run_value = win::hkcu_get_sz(win::RUN_KEY, &value_name());
        st.run_points_here = st
            .run_value
            .as_deref()
            .map(|v| normalize_path(&exe_of(v)) == normalize_path(&exe_path()))
            .unwrap_or(false);
        st.active = st.run_points_here;
        st.mechanism = if st.active { Mechanism::RunKey } else { Mechanism::None };
        remember(&st);
        return st;
    }

    /* 现有启动项是不是**另一份副本**的（别的 exe 路径）。 */
    let here = exe_path();
    let run_other = st
        .run_value
        .as_deref()
        .map(|v| points_to_other_copy(v, &here))
        .unwrap_or(false);
    let task_other = st.task_exists && !st.task_points_here;
    /* 测试实例看到"别人的启动项"就完全不动手 —— 见 `test_instance` 的说明。 */
    if test_instance() {
        /* 更彻底的一条：测试实例**完全不碰**系统启动项（不写也不删）。
         *
         * 理由：测试实例常常是"同一个 exe + 一份临时配置"，光看路径分不清
         * 它和正式版 —— 而注册表是全用户共享的。要测"写入"这条路，用
         * `QS_AUTOSTART_VALUE` 换一个值名即可。 */
        st.error = Some("测试实例（QS_INSTANCE_TAG）：不修改系统启动项".to_string());
        crate::win::log(&format!("autostart: {}", st.summary()));
        remember(&st);
        return st;
    }

    if on {
        if here.is_empty() {
            st.error = Some("拿不到自身 exe 路径".to_string());
            remember(&st);
            return st;
        }
        /* 老名字那条僵尸项：不管是开是关都清掉。 */
        let _ = win::hkcu_delete_value(win::RUN_KEY, RUN_VALUE_LEGACY);

        let mut task_err: Option<String> = None;
        let mut task_ok = false;
        if want_elevate && st.elevated {
            match task_create() {
                Ok(()) => {
                    task_ok = true;
                    /* 计划任务已经能拉起，就把 Run 项删掉 —— 否则登录时
                     * 两条同时触发：一个普通权限、一个管理员，单实例会
                     * 让后到那个静默退出，日志里多一堆噪声，托盘图标还
                     * 可能闪一下。 */
                    let _ = win::hkcu_delete_value(win::RUN_KEY, &value_name());
                    let _ = win::startup_approved_reset(&value_name());
                }
                Err(e) => task_err = Some(e),
            }
        }
        if !task_ok {
            /* 不需要提权（或提权任务建不起来）→ 用 Run 项。
             * ⚠ 先把可能残留的旧计划任务删掉：用户刚把「默认以管理员运行」
             *   关掉时，上一个版本建的任务还在，不删的话开机仍然以管理员
             *   启动，开关就成了摆设。 */
            if st.task_exists {
                let _ = task_delete();
            }
            let cmd = command_line(&here);
            if !win::hkcu_set_sz(win::RUN_KEY, &value_name(), &cmd) {
                st.error = Some("写注册表失败（权限 / 组策略 / 安全软件拦截）".to_string());
            }
            /* 任务管理器里被关掉过的话，这里顺手把"停用"标记清掉 ——
             * 否则注册表写得再对，Windows 还是不会启动它。 */
            if st.error.is_none() {
                let _ = win::startup_approved_reset(&value_name());
            }
            if let Some(e) = task_err.take() {
                /* 只有"需要提权但任务建不起来"才值得提一句；普通用户本来
                 * 就不建任务，不算错。 */
                if st.error.is_none() {
                    st.error = Some(format!("{e}，已退回注册表启动项"));
                }
            }
        }
        st = status();
        st.desired = on;
        st.elevated = win::is_admin();
        if st.error.is_none() {
            st.error = task_err;
        }
        if !st.active && st.error.is_none() {
            st.error = Some("写完了但读回校验没通过".to_string());
        }
    } else {
        /* 关：只清**自己的**那条。如果注册表项 / 计划任务指向另一份副本
         * （用户桌面上还有一个测试版、或者旧版装在别的目录），**不能删** ——
         * 那是别人（另一份安装）的自启，删掉等于砸别人的设置。
         * 这个坑真踩过：一份临时实例用着默认配置（autoStart=false），
         * 一启动就把正式版的启动项当"残留"清掉了。 */
        let run_ok = if run_other {
            true
        } else {
            win::hkcu_delete_value(win::RUN_KEY, &value_name())
        };
        let _ = win::hkcu_delete_value(win::RUN_KEY, RUN_VALUE_LEGACY);
        if !run_other {
            let _ = win::startup_approved_reset(&value_name());
        }
        let task_ok = if task_other {
            true
        } else {
            match task_delete() {
                Ok(()) => true,
                Err(e) => {
                    st.error = Some(e);
                    false
                }
            }
        };
        st = status();
        st.desired = on;
        st.elevated = win::is_admin();
        if run_other || task_other {
            st.error = Some("启动项属于另一份副本，本次没有动它".to_string());
        }
        if (st.active || !run_ok || !task_ok) && st.error.is_none() {
            st.error = Some("还有启动项没清掉".to_string());
        }
    }
    crate::win::log(&format!("autostart: {} — {}", if on { "开启" } else { "关闭" }, st.summary()));
    remember(&st);
    st
}

/* ------------------------------------------------------------------ */
/* 启动自检                                                            */
/* ------------------------------------------------------------------ */

/// 启动时按配置**自我修复**一次（v2.23.117 起每条启动路径都会走到）。
///
/// 旧代码这里被 `!QS_LITE_PORTABLE` 挡住，而本仓库的构建正是 lite ——
/// 于是"设置里开着、注册表里没有"这种状态永远不会被修好。现在：
///
/// * 配置要开 → 状态不对（没写 / 指向别处 / 被禁用 / 该用任务却只有 Run 项）
///   就重写一次；
/// * 配置要关 → 还有残留就清掉；
/// * 状态本来就对 → 一次注册表读 + 一次 `schtasks /Query`，不写盘。
///
/// 返回最终状态（调用方拿去写日志 / 诊断）。
pub fn ensure(desired: bool, want_elevate: bool) -> Status {
    let mut st = status();
    st.desired = desired;
    if test_instance() {
        /* 测试实例连"自检"都不做：它读到的现状可能是正式版的，按它的配置
         * 去修就会改到正式版（见 `test_instance` 的说明）。 */
        win::log("autostart: 测试实例，跳过启动项自检");
        remember(&st);
        return st;
    }
    let need_task = want_elevate && st.elevated;
    /* 落地方式也要对得上：要提权却只有 Run 项 → 升级成计划任务；
     * 不再提权却留着计划任务 → 降级回 Run 项（否则关掉提权开关后
     * 开机照样以管理员启动，那个开关等于假的）。 */
    let wrong_mech = (need_task && st.mechanism == Mechanism::RunKey)
        || (!need_task && st.mechanism == Mechanism::Task)
        /* 任务在、但指向别的 exe（换了位置）：`/F` 覆盖一次就修好了。 */
        || (st.task_exists && !st.task_points_here);
    if desired {
        if !st.active || wrong_mech {
            win::log(&format!("autostart: 启动自检发现状态不对（{}），重写", st.summary()));
            st = apply(true, want_elevate);
        }
    } else if st.active || st.run_value.as_deref().map(is_zombie_command).unwrap_or(false) {
        /* 配置里关着：只清**自己的**（或指向已不存在文件的僵尸项）。
         * 指向另一份副本的启动项不能动 —— 那是另一份安装的设置。 */
        win::log("autostart: 配置里已关闭，清掉自己的残留启动项");
        st = apply(false, want_elevate);
    } else if st.run_value.is_some() || st.task_exists {
        win::log("autostart: 配置里已关闭；现有启动项属于另一份副本，不动它");
    }
    st.desired = desired;
    remember(&st);
    win::log(&format!("autostart: 当前状态 — {}", st.summary()));
    st
}

/* ------------------------------------------------------------------ */
/* 单元测试                                                            */
/* ------------------------------------------------------------------ */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_quotes_path_and_carries_boot_arg() {
        let s = command_line("C:\\Program Files\\QA\\QuickAchieve.exe");
        assert_eq!(s, "\"C:\\Program Files\\QA\\QuickAchieve.exe\" --autostart");
        /* 空格路径不加引号会被 Windows 按空格切开 —— 这条断言就是防它。 */
        assert!(s.starts_with('"'));
        assert!(s.ends_with(BOOT_ARG));
    }

    #[test]
    fn exe_of_parses_quoted_unquoted_and_with_args() {
        assert_eq!(exe_of("\"D:\\a b\\q.exe\" --autostart"), "D:\\a b\\q.exe");
        assert_eq!(exe_of("D:\\a b\\q.exe --autostart"), "D:\\a b\\q.exe");
        assert_eq!(exe_of("C:\\x\\q.exe"), "C:\\x\\q.exe");
        assert_eq!(exe_of("  \"C:\\x\\q.exe\""), "C:\\x\\q.exe");
        assert_eq!(exe_of(""), "");
    }

    #[test]
    fn normalize_path_is_case_and_slash_insensitive() {
        assert_eq!(normalize_path("C:/A/B.exe"), normalize_path("c:\\a\\b.exe"));
        assert_eq!(normalize_path("\"C:\\A\\B.exe\""), "c:\\a\\b.exe");
    }

    #[test]
    fn normalize_path_collapses_dot_components() {
        let dir = std::env::temp_dir().join(format!("qa-norm-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let file = dir.join("q.exe");
        std::fs::write(&file, b"x").unwrap();

        let plain = file.to_string_lossy().to_string();
        let dotted = format!("{d}\\sub\\..\\q.exe", d = dir.display());
        let mixed = format!("{d}\\sub\\.\\..\\q.exe", d = dir.display());
        assert_eq!(normalize_path(&dotted), normalize_path(&plain));
        assert_eq!(normalize_path(&mixed), normalize_path(&plain));
        /* abs_path 也一样：写进注册表/任务前必须把 `.` 折掉。 */
        assert_eq!(abs_path(&dotted), plain);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn xml_field_reads_command_and_decodes_entities() {
        let x = "<Task><Actions><Exec><Command>D:\\A&amp;B\\q.exe</Command>\
                 <Arguments>--autostart</Arguments></Exec></Actions></Task>";
        assert_eq!(xml_field(x, "Command").as_deref(), Some("D:\\A&B\\q.exe"));
        assert_eq!(xml_field(x, "Arguments").as_deref(), Some("--autostart"));
        assert_eq!(xml_field(x, "Missing"), None);
    }

    #[test]
    fn command_line_roundtrips_through_exe_of() {
        let p = "D:\\software\\myself\\QuickAchieve.exe";
        assert_eq!(normalize_path(&exe_of(&command_line(p))), normalize_path(p));
    }

    /// 真机冒烟：写一个**临时值名**的 Run 项，读回，再删掉。
    ///
    /// 只碰 `QuickAchieveTestOnly__` 这个名字，不会动用户真正的自启项。
    /// 计划任务名也换成临时名 —— 否则 `apply(false)` 会去删用户真正的
    /// `QuickAchieve` 任务（那正是新版建出来的那个）。
    fn with_test_names<T>(f: impl FnOnce() -> T) -> T {
        if let Ok(mut g) = test_value_slot().lock() {
            *g = Some("QuickAchieveTestOnly__".to_string());
        }
        if let Ok(mut g) = test_task_slot().lock() {
            *g = Some("QuickAchieveTestOnlyTask__".to_string());
        }
        let out = f();
        if let Ok(mut g) = test_value_slot().lock() {
            *g = None;
        }
        if let Ok(mut g) = test_task_slot().lock() {
            *g = None;
        }
        out
    }

    #[test]
    fn run_key_roundtrip_with_temp_value() {
        with_test_names(|| {
            let st = apply(true, false);
            assert!(st.run_value.is_some(), "写入后应能读到 Run 项：{}", st.summary());
            let off = apply(false, false);
            assert!(!off.active);
            assert!(win::hkcu_get_sz(win::RUN_KEY, "QuickAchieveTestOnly__").is_none());
        });
    }

    /// **关自启时不许删别人（另一份副本）的启动项。**
    ///
    /// 这是 v2.23.117 自测时真踩到的：一份临时实例用默认配置（autoStart=false）
    /// 启动，把正式版的 Run 项当"残留"清掉了。这里用临时值名把那条规则钉住。
    #[test]
    fn disable_keeps_another_copys_entry() {
        with_test_names(|| {
            let other = "C:\\Windows\\System32\\notepad.exe";
            assert!(win::hkcu_set_sz(
                win::RUN_KEY,
                "QuickAchieveTestOnly__",
                &format!("\"{other}\" --autostart")
            ));
            let st = apply(false, false);
            assert!(!st.active, "别人的项不算我们生效");
            let left = win::hkcu_get_sz(win::RUN_KEY, "QuickAchieveTestOnly__");
            assert!(
                left.as_deref().map(|v| v.contains("notepad.exe")).unwrap_or(false),
                "另一份副本的启动项被删了：{left:?}"
            );
            assert!(win::hkcu_delete_value(win::RUN_KEY, "QuickAchieveTestOnly__"));
        });
    }

    /// `ensure(true)` 这条启动自检路径：配置要开、注册表里没有 → 应该补写并生效。
    #[test]
    fn ensure_writes_when_desired_and_missing() {
        with_test_names(|| {
            let _ = win::hkcu_delete_value(win::RUN_KEY, "QuickAchieveTestOnly__");
            let st = ensure(true, false);
            assert!(st.active, "补写之后应生效：{}", st.summary());
            assert!(st.run_points_here);
            let off = apply(false, false);
            assert!(!off.active);
        });
    }

    /// `ensure(false)` —— **开机自检最关键的那条安全规则**：
    /// 配置里关着，也不能把另一份副本的启动项当"残留"清掉。
    #[test]
    fn ensure_disable_keeps_another_copys_entry() {
        with_test_names(|| {
            let other = "C:\\Windows\\System32\\notepad.exe";
            assert!(win::hkcu_set_sz(
                win::RUN_KEY,
                "QuickAchieveTestOnly__",
                &format!("\"{other}\" --autostart")
            ));
            let _st = ensure(false, false);
            let left = win::hkcu_get_sz(win::RUN_KEY, "QuickAchieveTestOnly__");
            assert!(
                left.as_deref().map(|v| v.contains("notepad.exe")).unwrap_or(false),
                "ensure(false) 把另一份副本的启动项删了：{left:?}"
            );
            let _ = win::hkcu_delete_value(win::RUN_KEY, "QuickAchieveTestOnly__");
        });
    }

    #[test]
    fn ownership_helpers_classify_foreign_and_zombie_entries() {
        let me = exe_path();
        let my_cmd = command_line(&me);
        assert!(!points_to_other_copy(&my_cmd, &me), "自己那条不能算别人的");
        assert!(is_zombie_command("\"C:\\definitely\\missing\\qa.exe\" --autostart"));
        assert!(!is_zombie_command(&my_cmd));
        /* 指向一份真实存在的别的 exe（notepad 一定在）→ 别人的 */
        assert!(points_to_other_copy(
            "\"C:\\Windows\\System32\\notepad.exe\"",
            &me
        ));
    }
}
