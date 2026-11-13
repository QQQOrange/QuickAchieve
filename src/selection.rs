//! 选区确认与补字：从 main.rs 拆出的第一块。
//!
//! 这里只放「弹条前确认选中了文字」和「弹条后补字」两段流程，
//! 不涉及横条布局 / 引擎选择。函数签名与行为跟拆出前完全一致。

use std::sync::atomic::Ordering;

use crate::win;
use crate::{
    capture_selection, capture_selection_fast, foreground_still_target, hint_no_uia_once,
    is_bar_shown, is_console_window, push_late_text, CaptureInFlight, GEN,
};

pub(crate) fn post_show_copy_fill(gen: usize, fg_pid: u32, drag: (i32, i32, i32, i32)) {
    let _busy = CaptureInFlight::acquire();
    /* ⚠ 条件①必须**等横条真的可见**再判，不能直接 `is_bar_shown()`：
     *   `show_bar_at` 只是"发了事件"，`geo.shown` 要等前端量完尺寸回填
     *   （日志里 `after-show: visible=true` 那一行）。直接判会出现
     *   `post-copy` 抢在 `after-show` 之前、误判成"横条不在"。 */
    if !win::wait_until(500, is_bar_shown) {
        win::log("post-copy: 条件①不成立（500ms 内横条没显示出来）—— 不复制");
        return;
    }
    if GEN.load(Ordering::SeqCst) != gen {
        win::log("post-copy: 条件①不成立（横条已换新的一条）—— 不复制");
        return;
    }
    win::log("post-copy: 条件①弹条已出现 + 条件②未捕获文字 —— 开始复制补字");
    /* 终端里 Ctrl+C 是 SIGINT，不是复制。先弹空条的路径完全绕过了
     * `confirm_selection_text` 的那道保护，这里必须再挡一次。 */
    if drag_in_console(drag) {
        /* v2.23.121：不再用 OCR 兜底（用户口径：「选择文字不能靠 OCR 兜底」）。
         * 终端里 Ctrl+C 是中断信号，不能发；屏幕识别又已被要求去掉 ——
         * 这一支只能放弃补字，横条保持空着。 */
        win::log("post-copy: 起点在终端里 —— 不发 Ctrl+C，也不做屏幕识别；不补字");
        return;
    }
    /* 用户已经切走：不要拿新前台的选区冒充这一次划词的结果。 */
    if !foreground_still_target(fg_pid) {
        win::log("post-copy: 前台已经换人 —— 放弃补字");
        return;
    }
    /* v2.11.4：快通道（SendInput）拿不到就退到**完整三级通道**
     * （合成按键 → WM_COPY → WM_KEYDOWN/KILLFOCUS 那套，见 `capture_selection`）。
     *
     * 为什么需要这条：微信 4.x（Qt）会**忽略带 LLKHF_INJECTED 标记的按键**，
     * 只发 SendInput 的话它一声不响 —— 横条弹出来了，字永远补不上。
     * 完整通道里那两条窗口消息不依赖按键注入，Qt 的编辑控件照样认。
     * 代价只落在"快通道失败"这一支，成功路径完全不受影响。 */
    let target_hwnd = {
        let h = win::window_under_point_app(drag.0, drag.1);
        if h.0.is_null() {
            None
        } else {
            Some(win::root_window(h))
        }
    };
    let got = capture_selection_fast(fg_pid, 700, target_hwnd)
        .filter(|t| !t.trim().is_empty())
        .or_else(|| {
            /* 快通道可能因为"前台换人"而返回 None；那种情况下绝不能再走
             * 完整通道 —— 它会把 Ctrl+C 发给新前台。 */
            if !foreground_still_target(fg_pid) {
                win::log("post-copy: 快通道失败且前台已换人 —— 不再发完整复制");
                return None;
            }
            win::log("post-copy: 快通道没拿到 —— 退到完整复制通道（合成键 / WM_COPY / WM_KEYDOWN）");
            capture_selection(fg_pid, target_hwnd).filter(|t| !t.trim().is_empty())
        });
    match got {
        Some(t) if !t.trim().is_empty() => {
            if !is_bar_shown() || GEN.load(Ordering::SeqCst) != gen {
                win::log("post-copy: 复制成功但横条已换新 —— 丢弃这次结果");
                return;
            }
            let n = t.chars().count();
            push_late_text(t, true);
            win::log(&format!("post-copy: 补字成功（{n} 字）"));
        }
        _ => {
            /* v2.23.121：复制 / UIA 都没拿到字 → **不再看屏幕**（用户口径：
             * 「选择文字不能靠 OCR 兜底」）。屏幕上有字和"你选中了它"是两回事，
             * OCR 既会误弹、又会把界面上的字当成选区内容。这里只明确告诉用户
             * 这个程序不支持划词取词，横条保持空着。 */
            win::log("post-copy: 复制没拿到文字 —— 不做屏幕识别（按用户要求），放弃补字");
            hint_no_uia_once(fg_pid);
        }
    }
}

/// 鼠标落点（以及它所在的顶层窗口）是不是控制台/终端。
///
/// 终端里 Ctrl+C 是 SIGINT 而不是复制：误发会把用户正在跑的编译/脚本打断。
/// 这里同时查"鼠标底下"和"用户以为自己在操作"的窗口 —— 后者防的是
/// 贴纸 / 悬浮球这类穿透浮层盖在终端上的情况。
fn drag_in_console(drag: (i32, i32, i32, i32)) -> bool {
    let start = win::window_under_point(drag.0, drag.1);
    let app_win = win::window_under_point_app(drag.0, drag.1);
    [start, win::root_window(start), app_win, win::root_window(app_win)]
        .iter()
        .any(|h| is_console_window(*h))
}

/// **弹条之前**确认「这次拖选是不是真的选中了文字」。
///
/// 只认两条**有选区**的证据：复制通道（快通道，700ms）与 UIA。
/// v2.23.121 起不再用屏幕 OCR 兜底（用户口径：「选择文字不能靠 OCR 兜底」）：
/// 屏幕上有字 ≠ 用户选中了它 —— 拖滚动条 / 拖画布 / 拖空白时 OCR 会误认，
/// 也会把界面上的固定文字当成选区内容。拿不到就认定该程序不支持划词取词，
/// 提示一次、不弹条。
///
/// 终端里连 Ctrl+C 都不发（会打断正在跑的命令），直接按"取不到"处理。
pub(crate) fn confirm_selection_text(target_pid: u32, drag: (i32, i32, i32, i32)) -> Option<String> {
    if drag_in_console(drag) {
        win::log("confirm: 起点在终端里 —— 不模拟 Ctrl+C（会打断正在跑的命令），也不做屏幕识别，不弹");
        return None;
    }
    let target_hwnd = {
        let h = win::window_under_point_app(drag.0, drag.1);
        if h.0.is_null() {
            None
        } else {
            Some(win::root_window(h))
        }
    };
    if let Some(t) = capture_selection_fast(target_pid, 700, target_hwnd)
        .filter(|t| !t.trim().is_empty())
    {
        return Some(t);
    }
    /* v2.23.60：**不用屏幕识别兜底**（用户口径：OCR 会影响精确性）。
     *
     * 复制 / UIA 两条通道都拿不到选区 → 认定这个程序不支持划词取词。
     * 不弹条，但**给一次明确提示**，让用户知道"是这个软件不行"，而不是以为
     * 软件坏了、或者自己没选中。提示里带程序名，一眼能认出来。 */
    win::log("confirm: 复制与无障碍都没拿到选区 —— 该程序不支持划词取词，提示一次");
    hint_no_uia_once(target_pid);
    None
}

