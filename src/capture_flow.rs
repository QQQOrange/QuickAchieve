/* ================================================================== */
/* 取词调度：从「这一下算不算划词」到「横条里出现文字」                  */
/*                                                                    */
/* `selection` 只负责「怎么拿到选区文字」；这里负责「什么时候取、取不到   */
/* 怎么办、拿到之后补到哪根横条上」。拆出来是为了让 `main.rs` 不再同时    */
/* 承担窗口、钩子、设置和取词四件事；签名与行为跟拆出前完全一致。        */
/* ================================================================== */

use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::selection::{
    capture_selection, capture_without_clipboard, confirm_selection_text, post_show_copy_fill,
};
use crate::win;
use crate::{
    cfg_with, exact_reread_trigger, is_bar_shown, push_late_text, show_bar_at, spawn_exact_reread,
    trigger_allowed, BusyGuard, CaptureInFlight, BUSY, DISMISS_EPOCH, GEN, INPUT_EPOCH, OCR_ONLY,
};

pub(crate) fn start_capture(
    target_pid: u32,
    drag: (i32, i32, i32, i32),
    dbl: bool,
    text_surface: bool,
) {
    // 用户**开始**划词的那个窗口才是"他在哪个软件里操作"，
    // 黑名单按它判。按松手点判会在"拖出窗口边界"时换成别的程序。
    if !trigger_allowed(Some((drag.0, drag.1))) {
        return;
    }
    /* 钩子里的 `text_surface` 是按下瞬间采的：WinUI / 浏览器有时在
     * WM_LBUTTONDOWN 那一刻还返回一层透明的 DesktopChildSiteBridge，
     * 等消息消费线程跑到这里时窗口已经稳定，再问一次才问得到真正的
     * RichEdit / 文本控件。两次取 OR，既不增加误判，也补上这一类漏弹。 */
    let text_surface = text_surface || win::text_surface_at(drag.0, drag.1);
    /* 文字横条绝不能继承上一轮图片复制的 OCR-only 形态（否则 🔗 会被
     * `syncLinkBtn` 的 `ocrOnly` 条件藏掉）。 */
    OCR_ONLY.store(false, Ordering::SeqCst);

    /* 从"先弹空条"到"补字结束"整段都算取词在途：`bar-away` 看到计数 > 0
     * 就不会把正在补字的横条收走。这个守卫会跟着取词线程一起活到线程结束，
     * 任何提前 return 都由 RAII 递减。 */
    let capture_hold = CaptureInFlight::acquire();
    let dismiss0 = DISMISS_EPOCH.load(Ordering::SeqCst);

    /* ══════════════════════════════════════════════════════════════════
     * 先弹条，再取词（v2.23.75，用户口径：「任何情况下选中文字都要弹」）
     *
     * 旧顺序是"三条取词通道全部跑完 → 有字才弹"。问题是浏览器 / Electron
     * 里合成 Ctrl+C 经常被吞、UIA 偶尔读空，于是**取词失败 = 横条根本不出现**。
     * 用户看到的正是"有时候选中文字不弹"。
     *
     * 现在只要这一下**像在选择文字**（起点落在文字表面 / 编辑控件 / 浏览器
     * 渲染区，或用户把"没取到文字就不弹"关了），就先把空横条弹出来；文字在后台补。
     *   · `show_bar_at` 是线程安全的，这里是 `qs-capture` 线程，不是 UI 线程；
     *   · 取词成功 → `push_late_text` 原地补字，不重新摆窗；
     *   · 取词失败 → `post_show_copy_fill` 继续尝试复制/OCR，横条至少已经在了。
     * 这样"取词慢"只影响字什么时候出现，不再决定横条弹不弹。
     * ══════════════════════════════════════════════════════════════════ */
    let need_text = cfg_with(|c| c.bar_need_text);
    let dragged = (drag.2 - drag.0).abs() > 4 || (drag.3 - drag.1).abs() > 4;
    let early_gen = if text_surface || !need_text {
        let gen = show_bar_at(String::new(), false, true, Some(drag));
        if gen == 0 {
            // 锁屏 / 状态未就绪：show_bar_at 已经处理过提示，别再往下取词。
            return;
        }
        win::log(&format!(
            "start_capture: 先弹空条 gen={gen}（textSurface={text_surface} dbl={dbl} needText={need_text}）"
        ));
        Some(gen)
    } else {
        None
    };

    /* 上一次取词还没结束怎么办？以前是 BUSY.swap 拿到 true 就 return
     * —— 直接丢弃本次。但连续划选（第一段的横条还没出来、第二段已经
     * 划完了）时，用户看到的就是"划了却不弹"，丢一次就够判死刑。
     * 现在改成最多等 4.5s：等到了就正常做，等不到（软件卡死级慢）
     * 也**已经弹了空条**（early_gen），用户至少不会以为软件坏了。 */
    /* 等上一次取词结束的上限。必须 >= 一次取词的最坏耗时：
     * 3 轮 × 读窗口(1500+800+800) + 两段退避(220+380) ≈ 3.7s。
     * 给小了，用户"连着划两下"的第二下会被直接丢弃 —— 那是最容易被骂的体验。 */
    let deadline = std::time::Instant::now() + Duration::from_millis(4500);
    loop {
        if BUSY
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            break;
        }
        if std::time::Instant::now() >= deadline {
            win::log("start_capture: 上一次取词超过 4.5s 仍未结束，本次只保留已弹出的空条");
            return;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    // ⚠ 守卫交给子线程带着：线程里无论 panic 还是提前 return，BUSY 都会归位。
    //   （以前是在线程末尾手工 store(false)，一次 panic 就让取词功能永久失效。）
    let Ok(handle) = std::thread::Builder::new()
        .name("qs-capture".into())
        .spawn(move || {
            let _capture_hold = capture_hold;
            let _g = BusyGuard;
            /* 取词开始时的"用户收起过几次"。三条通道（编辑控件 → UIA → OCR）
             * 加起来可能要几百毫秒，这期间用户完全可能点了别处、按了 Esc ——
             * 那他就不要这条了，读完也别再弹（见 DISMISS_EPOCH 的注释）。 */
            let epoch0 = INPUT_EPOCH.load(Ordering::SeqCst);
            /* QS_FORCE_COPY=1：**仅测试用**，走"模拟复制 + 还原"的老通道 ——
             * 用来把 UIA 通道的结果和老通道并排对比。平时永远走下面那条。 */
            let out = if std::env::var("QS_FORCE_COPY").map(|v| v == "1").unwrap_or(false) {
                win::log("capture: QS_FORCE_COPY=1 —— 走模拟复制的旧通道");
                capture_selection(target_pid, None).map(|t| (t, "复制"))
            } else {
                /* ① 先走不碰剪贴板的通道：编辑控件 / UIA。绝大多数编辑器走这条。
                 * ② 拿不到（浏览器、自绘 UI 这类没有 UIA 文本模式的程序）→
                 *    **回退到复制通道**：模拟一次 Ctrl+C，读剪贴板，读完立刻还原。
                 *    这正是老版能在浏览器里出字的原因 —— 上一版把 OCR 兜底拆掉
                 *    之后这条链断了，表现为"选中文字不弹条"。 */
                /* ⚠ 这里先走**不碰剪贴板**的两条通道（编辑控件 / UIA）。
                 * 记事本、各种输入框、大多数原生控件走这条 —— 一次复制都不会发生。
                 *
                 * 拿不到文字时才轮到"确认选区"（`confirm_selection_text`）：
                 * v2.12.6 起它被挪到**弹条之前**，因为用户明确要求"确实选中了文字
                 * 才显示"。以前是先弹空条再复制补字，代价就是**任何一次拖动**
                 * 都会冒出一条空横条。顺序换回来，剪贴板纪律（快照 → 复制 →
                 * 还原）一条没变。 */
                capture_without_clipboard(drag)
            };
            match &out {
                Some((t, via)) => win::log(&format!(
                    "start_capture: {via} 通道拿到 {} chars",
                    t.chars().count()
                )),
                None => win::log("start_capture: 没拿到文字，仍然弹条（兜底）"),
            }
            /* r89：**取不到文字也要弹横条**。
             *
             * 用户原话：「我要求是说我在任何情况下我选中文字就一定要换出横条，
             * 除非我把这个软件加入了黑名单。」
             *
             * 原来的写法是 `if let Some((t, _)) = out { show_bar(...) }` ——
             * 三条取词通道（编辑控件 / UIA / OCR）**全都失败就不弹**。
             * 而这在真实软件里并不罕见：
             *   · 自绘 UI（游戏、部分 Electron 应用、Java/Qt 程序）没有标准编辑控件；
             *   · 没有 UIA 文本模式，或对方的 UIA 实现不应答（超时）；
             *   · OCR 兜底要"这块区域里有看得清的字"—— 小字、彩色背景、
             *     半透明浮层上都可能认不出来。
             * 用户看到的就是"划了半天，横条有时候不出来"，而且**没有任何提示**
             * 告诉他为什么 —— 这是最糟的一类体验。
             *
             * 现在改成：拿不到文字就弹一个**空文字的横条**。它有实际用处
             * （这正是"兜底"要保住的东西）：
             *   · 用户还能点横条上的「识图搜索」对刚才那块区域做识图；
             *   · 还能点「OCR」重新框选识别；
             *   · 至少**一定能打开**，用户知道软件在工作，不会以为坏了。
             *
             * ⚠ 唯一的例外是黑名单（`app_allowed`），那个判断在
             *   `trigger_allowed` 里、比这里更早，没有被我这条改动绕过。 */
            CaptureFinish { target_pid, drag, dbl, need_text, dragged, early_gen, dismiss0 }
                .run(epoch0, out);
        })
    else {
        BUSY.store(false, Ordering::SeqCst);
        win::log("start_capture: 起线程失败，BUSY 已复位");
        return;
    };
    drop(handle);   // 故意不等：JoinHandle 丢弃即 detach
}

/// 一次取词从「已经决定要弹条」到「文字补进横条」的收尾上下文。
///
/// `start_capture` 只负责调度（能不能触发、要不要先弹空条、等上一次取词）；
/// 这里负责收尾：确认选区、原地补字、没字时补一次复制、必要时精确复核。
/// 字段都是 `start_capture` 当场算好的，行为与拆分前逐行一致。
struct CaptureFinish {
    target_pid: u32,
    drag: (i32, i32, i32, i32),
    dbl: bool,
    need_text: bool,
    dragged: bool,
    early_gen: Option<usize>,
    dismiss0: usize,
}

impl CaptureFinish {
    fn run(self, epoch0: usize, out: Option<(String, &'static str)>) {
        let (mut text, mut via) = match out {
            Some((t, v)) => (t, v),
            None => (String::new(), ""),
        };
        /* 已经先弹了空条时，不再做"先确认选区再弹"这一步：
         * 用户已经看到横条，剩下的只是补字；`post_show_copy_fill` 会继续
         * 走复制 / 屏幕识别，成功就原地补上。没先弹的那条路（起点不是
         * I-beam，也不是双击）才需要确认，避免拖滚动条也冒空条。 */
        if text.trim().is_empty() && self.need_text {
            /* v2.23.65：**双击 / 三击选词也算"选中"**（`dbl`）。 */
            if self.early_gen.is_some() {
                win::log("start_capture: 空条已先弹出，继续后台补字（复制 / UIA）");
            } else {
                if !self.dragged && !self.dbl {
                    win::log(
                        "start_capture: 点击且没取到任何文字，且开着「没取到文字就不弹」—— 这次不弹",
                    );
                    return;
                }
                match confirm_selection_text(self.target_pid, self.drag) {
                    Some(t) => {
                        win::log(&format!(
                            "start_capture: 复核确认选区里有 {} 字 —— 弹条",
                            t.chars().count()
                        ));
                        text = t;
                        via = "复核";
                    }
                    None => {
                        win::log(
                            "start_capture: 拖选但没有选区（复制与 UIA 都没拿到字）—— 这次不弹",
                        );
                        return;
                    }
                }
            }
        }
        if text.trim().is_empty() {
            win::log("start_capture: 拖选但没拿到文字 —— 按用户要求仍然弹一条空条");
        }
        /* r98：OCR 认出来的文字**经常把网址认错**（用户实测：选中的是
         * `https://github.com/xanderfrangos/twinkle-tray`，OCR 给的是
         * `nttos.'//altnuo.corrvxanaenranaos/twlnl<le-tray`），于是
         * 「这段文字里有没有链接」判不过、直达链接按钮不出现 ——
         * 而复制路径拿到的是剪贴板原文，按钮正常。这就是用户报的
         * 「选中网址不弹按钮、复制了却弹」。
         *
         * 判据放宽（r97）救不了"认错字"这种噪声，所以这里补一条
         * **精确复核**：先照旧把横条弹出来（文字是 OCR 的，用户零等待），
         * 后台再模拟一次 Ctrl+C 把原文读回来，读到了就替换掉 ——
         * 走的是 Ctrl+C「迟到补」那条现成通道，按钮会跟着亮起来。
         *
         * 只在「这段文字里可能藏着网址/路径」时才动剪贴板（见
         * `exact_reread_why` 的注释），普通中文划词一个字节都不碰。 */
        let in_browser = win::uia_foreground_is_browser();
        /* 已经先弹空条的那条路：横条在取词之前就已可见，用户随后可能
         * 点了别处 / 按 Esc 把它收掉。此时**绝不能**再补弹 —— 但也不能
         * 因为"取词期间又有一次 mouseup"就把已经显示的那条误判成不要了。
         * 只以"这条横条现在还在不在、代次对不对"为准。 */
        let gen = if let Some(g) = self.early_gen {
            /* 用户可能在窗口还没完成 layout 时就按了 Esc / 点了别处。
             * `dismiss_bar_now` 在这种情况下不会把 `g.shown` 置 true，
             * 但会递增 `DISMISS_EPOCH` —— 必须认这个信号，否则取词结束后
             * 横条会"自己冒出来"。 */
            if DISMISS_EPOCH.load(Ordering::SeqCst) != self.dismiss0 {
                win::log("start_capture: 取词期间用户已收起先弹的横条 —— 本次结果丢弃");
                return;
            }
            /* `show_bar_at` 只是投递了 `qs://show`，真正可见要等前端量完
             * 尺寸回填 `geo.shown`。取词很快时（UIA 20~60ms）这里直接
             * 查一次会把"横条还没显示"误判成"已被收起"，白丢一份好结果。 */
            if !win::wait_until(500, || {
                GEN.load(Ordering::SeqCst) == g && is_bar_shown()
            }) {
                win::log("start_capture: 先弹的横条 500ms 内未显示/已被换新 —— 本次结果丢弃");
                return;
            }
            if DISMISS_EPOCH.load(Ordering::SeqCst) != self.dismiss0 {
                win::log("start_capture: 横条显示后被收起 —— 本次结果丢弃");
                return;
            }
            if !text.trim().is_empty() {
                // 原地补字，不重新摆窗、不改变用户已经看到的横条位置。
                push_late_text(text.clone(), false);
                win::log(&format!(
                    "start_capture: 先弹后取到 {} 字 —— 已原地补进横条",
                    text.chars().count()
                ));
            }
            g
        } else {
            /* 没有先弹空条（起点不是 I-beam，也不是双击）：仍按旧逻辑，
             * 取词期间用户点了别处就不要这条了。 */
            if INPUT_EPOCH.load(Ordering::SeqCst) != epoch0 {
                win::log("start_capture: 取词期间你又点击/划选了一次 —— 这一次不弹");
                return;
            }
            /* 把这次拖选的矩形一并带过去：浏览器里真光标取不到，
             * 「上方/下方」只能靠它。 */
            show_bar_at(text.clone(), false, true, Some(self.drag))
        };
        /* ── 方案 C：复制**只在两个条件同时成立时**才执行 ──────────
         *   ① 横条已经出现：上面刚弹完 / 先弹的空条还在，`gen` 就是它的代次；
         *   ② 没抓到文字：`text` 为空。
         *
         * 顺序是硬性的：**先弹条 → 再判断 → 最后才复制**。
         * 文字已经正常拿到时，这里根本不会进，一次复制都不会发生。 */
        if text.trim().is_empty() {
            let pid = self.target_pid;
            std::thread::spawn(move || post_show_copy_fill(gen, pid, self.drag));
        }
        /* ⚠ `via` 为空串 = 三条通道**全都没拿到文字**（OCR 也失败）——
         * 那也是 OCR 这条路上的失败，同样要复核：用户日志里"选中的是网址，
         * 横条却是空的"就是这么来的（`show_bar: ""`）。 */
        if !text.trim().is_empty() && matches!(via, "OCR" | "") {
            /* 要不要复核、为什么复核，全在 `exact_reread_trigger` 里
             * （三条规则 + 理由都写在那儿；2026-10-23 扩的第 ② ③ 条
             *   就是用户报的「马尾辫skill安装 → 《马尾辫 sk 泔安装、」）。 */
            if let Some(why) = exact_reread_trigger(&text, self.drag, in_browser) {
                win::log(&format!("capture(exact): {why} —— 后台精确复核一次"));
                spawn_exact_reread(gen, self.target_pid, text);
            }
        }
    }
}
