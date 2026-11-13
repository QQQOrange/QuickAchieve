//! 横条收起 / 挂起收起。
use std::ffi::c_void;
use std::sync::atomic::Ordering;

use windows::Win32::Foundation::HWND;

use crate::clip;
use crate::glass;
use crate::t::Emitter;
use crate::win;
use crate::{app, bar_hwnd, bar_win, st, BAR_DRAG_HOLD, DISMISS_EPOCH, FAV_PINNED, GEN, OFFSCREEN};

pub(crate) static DISMISS_HOLD: std::sync::Mutex<Option<std::time::Instant>> =
    std::sync::Mutex::new(None);
/// 挂起期间有人要求收条 —— 解除挂起时补收一次。
pub(crate) static DISMISS_PENDING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// 收起横条。所有调用点都用这一个；网格页的"等目标显示出来再收"靠
/// [DISMISS_HOLD] 在这里拦截，不必改每一处命令。
pub(crate) fn dismiss_bar() {
    /* v2.23.61：横条收起 = 把"微信那次替用户按的复制"还原回去。
     * 用户口径：「选择文本就是复制，横条消失后恢复剪贴板」。
     * 放在最前面：即使这次被挂起（`dismiss_held`），只要最终收起也会走到这里。 */
    clip::restore_deferred();
    if dismiss_held() {
        // 有人要收，但现在不许：记一笔，等 bar_hold(false) 时补上。
        DISMISS_PENDING.store(true, Ordering::SeqCst);
        win::log("dismiss_bar: 挂起中（网格页还在等目标窗口显示）");
        return;
    }
    dismiss_bar_now();
}

/// 当前是否处于"挂起收起"状态（过期自动失效，见 [DISMISS_HOLD]）。
fn dismiss_held() -> bool {
    let mut g = match DISMISS_HOLD.lock() {
        Ok(g) => g,
        Err(pe) => pe.into_inner(),
    };
    match *g {
        Some(dl) => {
            if std::time::Instant::now() >= dl {
                *g = None;      // 过期：自动解锁，绝不把横条永久钉在屏幕上
                false
            } else {
                true
            }
        }
        None => false,
    }
}

pub(crate) fn dismiss_bar_now() {
    /* 先记一笔"用户收过一次"（见 DISMISS_EPOCH 的注释）—— 必须放在下面那些
     * 提前 return 之前：横条还在读词、压根没显示时用户点了别处，这条也算数。 */
    DISMISS_EPOCH.fetch_add(1, Ordering::SeqCst);
    let h = bar_hwnd();
    if h.0.is_null() {
        return;
    }
    /* ⚠ 收藏面板的「钉住」必须在**所有提前 return 之前**复位。
     *
     * 原来 `FAV_PINNED` 只在 `set_fav_pinned` 里改：用户钉住面板 → 按 Esc 收条 →
     * 下次划词弹出来，点面板外面依旧"已钉住，忽略收起"，一直到用户手动取消钉住。
     * 用户感知就是「横条收不掉了」。钉住的语义本来就是"这一次展开时别自动收"，
     * 收起即结束。
     *
     * 位置很讲究：下面 `!g.shown && !g.focused` 那条会**提前 return**（横条本来
     * 就是收着的），复位放它后面就永远轮不到 —— 必须放最前面。 */
    /* 拖动挂起也要在这里复位：正常松手会自己置 false，但页面若在半路崩了
     * （或被别的窗口抢走），标记会一直挂着 —— 横条就再也不"移远收起"了。 */
    BAR_DRAG_HOLD.store(false, Ordering::SeqCst);
    if FAV_PINNED.swap(false, Ordering::SeqCst) {
        win::log("dismiss_bar: 收藏面板原本钉着，随收起一并解除");
        let _ = app().emit_to("bar", "qs://fav-pinned", serde_json::json!({ "on": false }));
    }
    // 横条收起 = 不再有实时抓屏：无条件解除「对抓屏隐身」。
    // 实时模式开着的时候用户按了 Esc / 点了别处，前端可能来不及发
    // glass_exclude(false)，窗口却已经不接受抓屏了 —— 那时用户截图
    // 录屏里横条会凭空消失。在这里兜底，双保险。
    glass::clear_exclude_from_capture(h);
    let restore = {
        let mut g = match st().geo.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if !g.shown && !g.focused {
            return;
        }
        g.shown = false;
        // 锚点必须一起清掉：前端收到 qs://hidden 后还会回一次 layout_bar，
        // 锚点留着的话 layout_bar 会把窗口重新摆回屏幕上。
        g.anchor = None;
        let r = if g.focused { g.prev_fg } else { 0 };
        g.focused = false;
        r
    };
    // 收起是有代价的：锚点被清掉之后，**之后任何一次 layout_bar 都不会再摆窗**
    // （"尚无锚点，只记录尺寸、不摆放窗口"），面板/菜单要撑窗口就撑不起来了。
    // 这条日志是排查"面板被切一块 / 窗口没跟着长大"那类问题的起点。
    win::log("dismiss_bar: 已收起横条（锚点一并清掉）");
    GEN.fetch_add(1, Ordering::SeqCst);
    win::set_noactivate(h, true);
    match bar_win() {
        Some(bw) => {
            let _ = bw.set_position(crate::t::PhysicalPosition::new(OFFSCREEN, OFFSCREEN));
        }
        None => {
            let (w, hh) = match st().geo.lock() {
                Ok(g) => (g.win_w.max(40), g.win_h.max(30)),
                Err(_) => (600, 100),
            };
            win::move_resize(h, OFFSCREEN, OFFSCREEN, w, hh, false);
        }
    }
    if restore != 0 {
        /* v2.11.0：**别把刚打开的内容顶下去**。
         *
         * `restore` 是"横条抢走焦点之前那个窗口"。网格页点完一项后，目标窗口
         * 往往已经在前台了 —— 这时再把 prev_fg 顶上来，用户看到的就是
         * "点了浏览器，焦点又跳回记事本、浏览器沉到后面"，像是内容没出来。
         * 所以只在"前台还是我们自己"时才回填；已经是别人了就撒手。
         * （`wait_ready` 已经确认目标就绪，这一步只是别把它踹下去。） */
        let fg = win::foreground_addr();
        let fg_ours = fg == 0
            || fg == h.0 as isize
            || fg == restore
            || win::window_pid(HWND(fg as *mut c_void)) == std::process::id();
        if fg_ours {
            win::set_foreground(HWND(restore as *mut c_void));
        } else {
            win::log(&format!(
                "dismiss_bar: 前台已是目标窗口 0x{fg:x}，跳过焦点回填（不抢它的前台）"
            ));
        }
    }
    let _ = app().emit_to("bar", "qs://hidden", ());
}
