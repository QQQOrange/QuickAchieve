//! 横条几何：屏幕矩形、实体/外框判定、显示时刻与单调时钟。


use crate::win;
use crate::{bar_hwnd, st};

/// 「藏起来」的实现方式：窗口本身一直是显示状态（这样 WebView2 才会一直渲染），
/// 只是被挪到屏幕外面去了。直接 ShowWindow(SW_HIDE) 会绕过 Tauri/wry 的可见性
/// 状态机，WebView2 控制器就停在隐藏态不再复绘 —— 踩过这个坑。
pub(crate) const OFFSCREEN: i32 = -20000;

pub(crate) fn in_rect(x: i32, y: i32, r: (i32, i32, i32, i32)) -> bool {
    x >= r.0 && x < r.2 && y >= r.1 && y < r.3
}

/// 屏幕坐标下的一块矩形 (left, top, right, bottom)。
pub(crate) type Rect = (i32, i32, i32, i32);

/// 返回 (窗口整体矩形, 实体矩形)，都在屏幕坐标
pub(crate) fn bar_rects() -> Option<(Rect, Rect)> {
    let h = bar_hwnd();
    if h.0.is_null() {
        return None;
    }
    let g = st().geo.lock().ok()?;
    if !g.shown {
        return None;
    }
    let r = win::window_rect(h)?;
    let outer = (r.left, r.top, r.right, r.bottom);
    let inner = (
        r.left + g.inner_x,
        r.top + g.inner_y,
        r.left + g.inner_x + g.inner_w.max(1),
        r.top + g.inner_y + g.inner_h.max(1),
    );
    Some((outer, inner))
}

/// 横条**最近一次真正显示**的时刻（进程内单调毫秒）。
///
/// 存在的理由是一个真实竞态（2026-10-30 实测抓到）：
/// `layout_bar` 里先把 `g.shown = true`，紧接着才 `SetWindowPos` 把窗口从
/// 屏外的 `(-20000,-20000)` 挪到光标附近。中间的这几十毫秒里，`bar-away`
/// 看门狗（70ms 一跳）可能插进来读 **旧的**窗口矩形 —— 距鼠标约
/// 29000px —— 于是把刚弹出来的横条当场收掉。窗口随后又被挪到屏幕上，
/// 就成了"逻辑上已收起、屏幕上还挂着"的鬼影，用户看到的是"闪一下没了"。
pub(crate) static BAR_SHOWN_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 进程启动时刻，用于 `mono_ms()`（不依赖系统时间，不受改时钟影响）。
static PROC_START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

pub(crate) fn mono_ms() -> u64 {
    PROC_START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis() as u64
}

pub(crate) fn is_bar_inner(x: i32, y: i32) -> bool {
    bar_rects().map(|(_, i)| in_rect(x, y, i)).unwrap_or(false)
}

pub(crate) fn is_bar_outer(x: i32, y: i32) -> bool {
    bar_rects().map(|(o, _)| in_rect(x, y, o)).unwrap_or(false)
}

pub(crate) fn is_bar_shown() -> bool {
    st().geo.lock().map(|g| g.shown).unwrap_or(false)
}
