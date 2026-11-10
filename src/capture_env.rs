//! 取词环境判断：剪贴板 owner 是否可信、微信进程、同一软件的不同进程
//! （v2.23.97 从 main 拆出）。只依赖 `win` 的进程/窗口查询。

use crate::win;


/// 剪贴板里的内容，是不是「我们信得过的那些进程」之一放进去的？
///
/// 只盯着剪贴板序号是不够的。序号变化只说明「有人动过剪贴板」，
/// 而几百毫秒的等待窗口里，用户自己按了复制、剪贴板管理器、
/// 带「自动复制」的词典插件……都可能插一脚。那种情况下读到的
/// 是**别人的内容**，横条就会莫名其妙弹出来 —— 用户要的是
/// 「没选中文字绝对不出现」，所以这里再加一道身份校验。
///
/// `trusted` = 允许的 owner pid 列表；**0 一律忽略**（0 表示"没拿到"）。
///
/// ⚠ **取不到判据就放行（fail-open）** —— 这里只是第二道保险，真正的判据是
///   「剪贴板序号变了没有」。卡在 pid=0 上会让正常复制也被挡掉：宁可多弹一次，
///   不能哑火。所以 `trusted` 里**全是 0**（一个可信 pid 都没拿到）时也放行。
///
/// 这个函数是三个调用点（Ctrl+C 兜底 / 拖选取词 / 迟到的复制补丁）合并出来的：
/// 它们判据本来就略有不同，但骨架一模一样，各自抄了一遍 —— 结果其中一份
/// （拖选取词）把「一个 pid 都没拿到」写成了拒绝，和它自己文档里写的 fail-open
/// 正好相反，会把那次取词白白丢掉。
pub(crate) fn clipboard_owner_trusted(trusted: &[u32]) -> bool {
    let owner = win::clipboard_owner();
    // 没人放 / 已经销毁 → 放行
    if owner.0.is_null() {
        return true;
    }
    // 拿不到 owner 的进程号（窗口已销毁 / 权限不足）→ 放行
    let opid = win::window_pid(owner);
    if opid == 0 {
        return true;
    }
    if trusted.iter().any(|&p| p != 0 && p == opid) {
        return true;
    }
    /* 浏览器 / Electron 的剪贴板 owner 常是渲染子进程，而 `target_pid` 是
     * 鼠标底下的另一个子进程；pid 对不上但 exe 是同一个。只比 pid 会把
     * 真正的选区判成"别的程序写的"，日志里表现为 owned=false、白丢结果。 */
    if trusted
        .iter()
        .any(|&p| p != 0 && same_app_pid(opid, p))
    {
        return true;
    }
    // 一个可信 pid 都没有 —— 判据根本不成立，按 fail-open 放行
    if trusted.iter().all(|&p| p == 0) {
        return true;
    }
    win::log(&format!(
        "clipguard: 剪贴板 owner pid={opid} 不在可信列表 {trusted:?} 里 —— 判定为别的程序在动剪贴板，忽略"
    ));
    false
}


/// 拖选取词这条路信得过的 owner：**划词目标**（`target_pid`）或**当前前台**。
///
/// 为什么和 Ctrl+C 兜底那条线不一样：拖选这条路**知道用户是在哪个窗口里划的**
/// （`target_pid`），判据比"当前前台是谁"更准。微信 4.x 是「主进程 + 一堆渲染/
/// 助手进程」的多进程架构，"前台窗口"和"真正执行复制的进程"未必同一个 ——
/// 只认前台会把真选区挡掉。
///
/// 判据与 fail-open 原则都在 `clipboard_owner_trusted` 里。
pub(crate) fn clipboard_owned_by(target_pid: u32) -> bool {
    clipboard_owner_trusted(&[target_pid, win::window_pid(win::foreground_window())])
}

/// 这个 pid 是不是微信（Weixin.exe / WeChat.exe，含它的一堆子进程）。
///
/// v2.23.61：微信 4.x 是 MMUI（Qt 5.15 自绘），UIA/MSAA 都拿不到选区，
/// 唯一可行的路是"替用户按一次 Ctrl+C 再读剪贴板"。这条只对微信开。
pub(crate) fn is_wechat_exe_name(name: &str) -> bool {
    let n = name.trim();
    n.eq_ignore_ascii_case("weixin.exe") || n.eq_ignore_ascii_case("wechat.exe")
}

pub(crate) fn is_wechat_pid(pid: u32) -> bool {
    /* ⚠ `process_exe` 只返回**文件名**（如 `Weixin.exe`），不是全路径。
     * 以前这里用 `ends_with("\\weixin.exe")` 判断，永远匹配不到 ——
     * 于是微信专属的 1.5 秒复制预算、延迟还原全部失效，还反过来吃
     * `exact_recently_deaf` 的 5 秒冷却：微信里连续划两次，第二次直接
     * "跳过这一轮"，表现为不弹条。 */
    win::process_exe(pid)
        .map(|p| is_wechat_exe_name(&p))
        .unwrap_or(false)
}

/// 两个 pid 是不是同一个软件（主进程 / 渲染进程 / 助手进程都算）。
///
/// 浏览器和 Electron 是"主进程 + 一堆渲染进程"：鼠标底下的窗口属于渲染进程，
/// 占着前台的却常常是主进程。只比 pid 会把"同一个软件"判成"前台已经换人"，
/// 于是连 Ctrl+C 都不发出去 —— 这是日志里一类"选中文字不弹条"的直接原因。
pub(crate) fn same_app_pid(a: u32, b: u32) -> bool {
    if a == 0 || b == 0 {
        return false;
    }
    if a == b {
        return true;
    }
    match (win::process_exe(a), win::process_exe(b)) {
        (Some(x), Some(y)) => x.eq_ignore_ascii_case(&y),
        _ => false,
    }
}

