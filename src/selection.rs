//! 选区取词 / 确认 / 补字：从 main.rs 拆出的第一块。
//!
//! 这里放三条取词路径（剪贴板三级通道、标准编辑控件、UIA）以及
//! 「弹条前确认选中了文字」「弹条后补字」两段流程，
//! 不涉及横条布局 / 引擎选择。函数签名与行为跟拆出前完全一致。

use std::ffi::c_void;
use std::sync::atomic::Ordering;
use std::time::Duration;

use windows::Win32::Foundation::HWND;

use crate::win;
use crate::{
    clip, clipboard_owned_by, foreground_still_target, is_bar_shown,
    is_console_window, is_wechat_pid, lock_clip, push_late_text, same_app_pid,
    CaptureInFlight, GEN, LAST_FOREIGN_FG,
};

/* ---------------- 剪贴板复制通道（SendInput → WM_COPY → 按键消息） ---------------- */

/// 松手后、发 Ctrl+C 之前的稳定期（ms）。
///
/// 为什么需要它：很多软件（浏览器富文本区、Electron 应用、Office、
/// 各种 IDE）并不是在收到 mouseup 的那一刻建立选区，而是把「提交选区」
/// 丢进自己的消息队列里异步做。我们钩子一收到 mouseup 就立刻发 Ctrl+C，
/// 复制到的还是上一次的选区（甚至什么都没有）—— 序号没变 = 不弹，
/// 这就是"有时候不触发"最隐蔽的一因。40ms 足够这类软件把选区建好，
/// 又短到用户完全感觉不到延迟。
const SEL_SETTLE_MS: u64 = 40;

/// 重试前额外等待的时间序列（ms）。不同软件响应 Ctrl+C 的快慢能差出
/// 几百毫秒（远程桌面 / 虚拟机 / 大型文档窗口尤其明显）。序号没变就
/// 隔一会儿再来一次，直到有一次真的把选区复制进去。
const SEL_RETRY_BACKOFF_MS: [u64; 2] = [220, 380];

/// 每一轮投递复制请求之后，最多等多久去读剪贴板（ms），按轮次给。
///
/// 第一轮给得最长，因为有些客户端（微信 4.x）的复制是**两步**的：
///   ① `EmptyClipboard` 把剪贴板清空，并且**一直握着不撒手**；
///   ② 内部渲染 / 助手进程把数据准备好，才真的写进去、才关闭剪贴板。
/// 第①步到第②步之间，我们连 `OpenClipboard` 都打不开（别人的剪贴板没关），
/// 只能干等 —— 实测这段时间能到几百毫秒。
///
/// ⚠ 所以窗口内**绝不能再投一次复制请求**：重投会让它重新清空剪贴板、
///   把等待重新计时，于是每一轮都差一点点，永远等不到落地。
///   这正是"微信里十次有九次不弹、偶尔弹一次"的来源。
const SEL_READ_WINDOW_MS: [u64; 3] = [1500, 800, 800];

/// 模拟一次 Ctrl+C 把选区拿走，再立刻把原剪贴板还回去。
///
/// 结构是「3 个回合、每回合换一条取词通道」：
///   ① 真实的合成按键（SendInput）—— 对绝大多数程序有效；
///   ② 窗口消息 `WM_COPY`；
///   ③ 窗口消息 `Ctrl+C`（`WM_KEYDOWN`/`WM_KEYUP`）。
/// 任一回合拿到有效文字即返回。
///
/// 为什么要换通道：`SendInput` 造出来的按键带 `LLKHF_INJECTED` 标记，
/// 有些客户端会据此**无视**这些按键（微信 4.x 是 Qt 写的，属于高危对象），
/// 只走一条路的话这类程序里就会出现「划词永远不弹条」。
///
/// `target_pid` = 用户开始拖动的那个窗口所属进程，给剪贴板归属校验用。
pub(crate) fn capture_selection(target_pid: u32, target_hwnd: Option<HWND>) -> Option<String> {
    // 仅测试用（和 QS_ALLOW_INJECTED / QS_SKIP_SENDINPUT 同一类逃生口）：
    // 跳过全部复制通道、直接按“取不到词”处理，用来单独验证兜底表现。
    if std::env::var("QS_FORCE_OCR").map(|v| v == "1").unwrap_or(false) {
        win::log("capture: QS_FORCE_OCR=1 —— 跳过复制通道（只用于验证取不到词的表现）");
        return None;
    }
    /* ⚠ 完整通道是最后一道兜底，但**不能盲发**：
     *   · 前台已经换人 → 复制回来的是别人的选区；
     *   · 前台是终端 → Ctrl+C 会打断正在跑的命令；
     *   · 横条自己拿着焦点 → 按键全打在我们身上，还要先还给目标窗口。 */
    if win::window_pid(win::foreground_window()) == std::process::id() {
        if let Some(h) = target_hwnd.filter(|h| !h.0.is_null()) {
            win::force_foreground(h);
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let fg = win::foreground_window();
    let fg_pid = win::window_pid(fg);
    if !foreground_still_target(target_pid) {
        win::log(&format!(
            "capture: 前台已经换人（目标 pid={target_pid}，当前 pid={fg_pid}）—— 放弃完整复制"
        ));
        return None;
    }
    if is_console_window(fg) || is_console_window(win::root_window(fg)) {
        win::log("capture: 当前前台是终端 —— 绝不发 Ctrl+C（会被当成中断信号）");
        return None;
    }
    /* v2.23.81：**用户还按着修饰键时绝不模拟复制**。
     *
     * 真实场景：用户用 Ctrl+拖选 / Ctrl+Shift 选了一段文字，松手那一刻
     * Ctrl 往往还没抬起；或者他刚划完选区就顺手按了 Ctrl+C。这时我们再
     * 补一次合成 Ctrl+C，手感就是「Ctrl+C 被劫持」—— 在终端 / 游戏 /
     * 远程桌面里还可能触发别的动作。把这次自动取词让给用户：横条已经
     * 弹出来的保持原样，用户真的复制完，他自己的 Ctrl+C 会写入剪贴板。
     *
     * 只挡 Ctrl / Alt / Win：Shift 在划选收尾时经常还没松，挡它会让
     * 正常的「Shift+拖选」永远取不到词。 */
    if win::ctrl_down() || win::alt_down() || win::win_down() {
        win::log("capture: 用户还按着 Ctrl/Alt/Win —— 不模拟复制（把 Ctrl+C 让给用户）");
        return None;
    }
    // 整段都必须在 CLIP_LOCK 里：中途要和 Ctrl+C 兜底那条线抢同一个剪贴板，
    // 交错的话 restore 的「先清空再写回」会把用户原本的复制内容冲掉。
    let _clip = lock_clip();
    /* ⚠ 快照失败**不再直接放弃**。
     *
     * 以前是「snapshot 拿不到就 return None」—— 而微信这类客户端会长时间占着
     * 剪贴板（它自带剪贴板监听），OpenClipboard 一直拿不到 → 取词在**第一步**
     * 就整个放弃 → 用户看到的现象正是"在微信里划词永远不弹条"。
     *
     * 现在的口径：拿不到快照就不还原（本来也没备到东西），但取词照做。
     * 最坏结果和用户自己按一次 Ctrl+C 一样（剪贴板被选区覆盖），不会更糟；
     * 换来的是"一定能弹"。 */
    /* ⚠ 2026-10-30 改口径：**快照拿不到就不复制**。
     *
     * 这里原先刻意"拿不到快照也照做"（理由是微信会长时间占着剪贴板，放弃会让
     * 微信里永远不弹条）。但用户现在的要求很明确：**复制不能影响原来的剪贴板**。
     * 没有快照就没有还原的依据，做下去等于直接覆盖用户复制的内容 —— 不能接受。
     *
     * 注：这条通道目前只有 `QS_FORCE_COPY=1` 的自检分支在用，改动不影响主链路。 */
    let Some(snap) = clip::snapshot() else {
        win::log("capture: 剪贴板快照失败（被占用）—— 不发 Ctrl+C，保持用户剪贴板原样");
        return None;
    };
    let old_text = snap.text().map(|t| t.trim().to_string());
    /* 这一次取词的"现场记录"。排查时最想知道的三件事就是：目标窗口属于哪个进程、
     * 快照拿没拿到、复制前剪贴板里是什么 —— 一行写全，省得来回要日志。
     *
     * ⚠ v1.4.0：剪贴板内容**只记字数**。以前这里会把前 20 个字原样写进日志，
     * 那等于把用户上一条复制的内容（可能是密码、可能是私信）明文落盘。 */
    win::log(&format!(
        "capture: 开始 目标pid={target_pid} 快照={} 复制前剪贴板={} 字",
        "ok",
        old_text.as_deref().map(|s| s.chars().count()).unwrap_or(0)
    ));
    /* 先等选区落地，再读「取词前」的序号 —— 顺序反了的话，软件在稳定期
     * 里自己提交的选区会把 before 带成新值，等我们自己的 Ctrl+C 反而
     * 「序号没变」，白白漏一次。 */
    std::thread::sleep(Duration::from_millis(SEL_SETTLE_MS));
    let before = clip::seq();
    let last_round = SEL_RETRY_BACKOFF_MS.len();
    let mut got: Option<String> = None;
    /* v2.23.32：**第一次发模拟复制之前**的序号。和 `before` 不同就说明
     * 这段窗口里已经有人写过剪贴板（用户自己复制了），那种情况下不能还原。 */
    let mut pre_send_seq: Option<u32> = None;
    // v2.23.32：这一轮有没有"用户先复制"的情况（见下面还原处的注释）
    let mut user_wrote_first = false;
    for round in 0..=last_round {
        if round > 0 {
            std::thread::sleep(Duration::from_millis(SEL_RETRY_BACKOFF_MS[round - 1]));
        }
        // 仅测试用的逃生口（和 QS_ALLOW_INJECTED / QS_INSTANCE_TAG 同一类）：
        // 跳过 SendInput 那一轮，直接从「窗口消息」通道开始 —— 用来单独验证
        // 新加的通道本身能不能取到词，正式启动不会带上。
        let skip_sendinput = std::env::var("QS_SKIP_SENDINPUT")
            .map(|v| v == "1")
            .unwrap_or(false);
        let copy_round = if skip_sendinput { 0 } else { 1 };
        if pre_send_seq.is_none() {
            pre_send_seq = Some(clip::seq());
        }
        let ch: String = if round == 0 && !skip_sendinput {
            /* v2.23.32：send_ctrl_c 现在会告诉我们"没发"（用户正按着修饰键）。
             * 没发就如实标出来 —— 不假装成功，也不马上换下一条通道去抢：
             * 等一拍再看剪贴板（用户自己那次 Ctrl+C 多半就在这一刻落地）。 */
            if win::send_ctrl_c() {
                "SendInput".to_string()
            } else {
                win::log("capture: 这一轮跳过模拟复制（用户正在按键），先看剪贴板会不会自己变");
                "跳过(用户按键)".to_string()
            }
        } else if round == copy_round {
            let ok = win::post_copy_to_focus();
            format!("WM_COPY{}", if ok { "" } else { "(投递失败)" })
        } else {
            /* ⚠ v2.11.6：这条"窗口消息按键"通道**默认关闭**。
             *
             * 用户报「输入框里会莫名多出字母 c」，就是它干的：
             * `post_ctrl_c_to_focus` 往焦点窗口直接 Post 一条
             * `WM_KEYDOWN(VK_C)`。Ctrl 的按下状态只存在于**线程级**键盘状态里
             * （还要先 AttachThreadInput 才共享），一旦 attach 失败、或者焦点
             * 控件属于另一个线程，目标收到就是**一个光秃秃的 c 键**——
             * 编辑框照单全收，直接打出一个 'c'。
             *
             * 而这条通道实测也没换来什么：微信 4.x 收到它照样不复制
             * （`changed=false`），其它程序有 SendInput / WM_COPY 就够了。
             * v2.23.121 起选区不再做屏幕识别，这条通道本就没有兜底作用，
             * 丢掉它不影响取词。
             * 排障时可以用 QS_ALLOW_KEYMSG=1 临时打开。 */
            let allowed = std::env::var("QS_ALLOW_KEYMSG")
                .map(|v| v == "1")
                .unwrap_or(false);
            if allowed {
                let ok = win::post_ctrl_c_to_focus();
                format!("按键消息{}", if ok { "" } else { "(投递失败)" })
            } else {
                win::log("capture: 跳过「窗口消息按键」通道（它会往输入框里打 c，默认关闭）");
                "跳过".to_string()
            }
        };
        /* 投完复制请求后**轮询读**，而不是"等序号一变就读一次"。
         *
         * 两个理由，都是实测踩出来的：
         *   ① 延迟渲染：有些宿主先 `EmptyClipboard` + `SetClipboardData(NULL)`
         *      声明格式，等别人来要数据时才真的填进去。序号在"声明"那一刻就变了，
         *      可那一刻剪贴板里还是空的 —— 只读一次会稳定读到空。
         *   ② 复制可能是异步的（UI 进程把活儿交给渲染/助手进程），序号变了
         *      不等于数据已经就位。
         * 所以：轮询到 460ms，期间只要出现**和复制前不一样的非空文本**就算成功。
         * 读的时候走 `clip::read_text_any()`（UTF-16 → ANSI → OEM → CF_HTML），
         * 不再只认 CF_UNICODETEXT。 */
        /* 窗口长度分两档：
         *   · 一开始只给 250ms —— "压根没选中东西"（序号都不动）是最常见的
         *     情况，不能让它白占 1.5s（那会把取词线程占住，用户紧接着划第二下
         *     就被顶掉了）；
         *   · 一旦看到序号变了（说明复制**正在发生**，只是数据还没落地），
         *     立刻把窗口拉长到这一轮的上限，安心等它写完。 */
        let long_ms = SEL_READ_WINDOW_MS[round.min(SEL_READ_WINDOW_MS.len() - 1)];
        let t0 = std::time::Instant::now();
        let mut deadline = t0 + Duration::from_millis(250);
        let mut changed = false;
        let mut got_text: Option<String> = None;
        let mut probe: Option<String> = None;
        /* 等待期间按时间采样剪贴板状态。排查"对方到底写没写进来"只有这一条路：
         * 光看结束时的状态分不清"从没写过"和"写过又被谁抹了"。 */
        let mut last_sample = std::time::Instant::now();
        loop {
            if last_sample.elapsed() >= Duration::from_millis(300) {
                last_sample = std::time::Instant::now();
                win::log(&format!(
                    "capture: 等待中 round={round} seqΔ={} 剪贴板={}",
                    clip::seq().wrapping_sub(before) as i32,
                    clip::describe_formats()
                ));
            }
            if clip::seq() != before {
                if !changed {
                    deadline = t0 + Duration::from_millis(long_ms);
                }
                changed = true;
            }
            if changed {
                if let Some(t) = clip::read_text_any() {
                    let t = t.trim().to_string();
                    if !t.is_empty() {
                        /* ⚠ 不能要求"内容必须和复制前不同"：
                         * 用户完全可能选中的就是剪贴板里已有的那段文字
                         * （刚复制过、又回去重新选一次）。这时序号变了、
                         * owner 也是目标程序，却因为内容相同被判"没选中"，
                         * 横条永远是空的 —— 日志里的 `fresh=false valid=false`
                         * 加 `读到=N 字` 就是它。
                         * 序号变化 + 归属校验才是硬证据；内容相同也算。 */
                        if t.chars().count() <= 20000 {
                            got_text = Some(t.clone());
                            probe = Some(t);
                            break;
                        }
                        probe = Some(t);
                    }
                }
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(40));
        }
        /* v2.23.32：**复制前剪贴板就已经被改过** = 用户自己先复制了（右键菜单"复制"、
     * 或者他的 Ctrl+C 抢在我们前面落地）。那种情况下哪怕是同一段文字，
     * 还原也会把他这次复制抹掉 —— 所以只看"改动发生在谁之后"。 */
    user_wrote_first = matches!(pre_send_seq, Some(s) if s != before);
    let owned = changed && clipboard_owned_by(target_pid);
        // 日志里展示"最后读到的那一份"（不管接不接受），排查时信息更全
        let raw = probe.clone();
        let text = got_text.clone();
        /* 「内容确实和复制前不一样」比序号更硬的证据：序号变了但内容没变，
         * 说明是别的程序在还原/整理剪贴板，不是选区。 */
        let fresh = matches!((&old_text, &text), (Some(o), Some(n)) if n != o);
        /* 前两轮严格（归属校验必须过）；**最后一轮放宽到"内容确实变了"** ——
         * 因为再不放这一轮就什么都不弹了，而用户要的是能弹出来。
         * 放宽的代价有界：要在 460ms 窗口里恰好有第三方程序改写剪贴板，
         * 且写进去的内容和复制前不一样，才会误弹一次（日志留痕）。 */
        let valid = text.is_some() && (owned || (round == last_round && fresh));
        // ⚠ v1.4.0：读到的东西**只记字数**（内容属于用户数据，不落明文日志）
        win::log(&format!(
            "capture: round={round} ch={ch} changed={changed} owned={owned} fresh={fresh} valid={valid} \
             目标={target_pid} owner={} 前台={} 读到={} 字",
            win::window_pid(win::clipboard_owner()),
            win::window_pid(win::foreground_window()),
            raw.as_deref().map(|s| s.chars().count()).unwrap_or(0)
        ));
        if !valid {
            /* 读不到就**把剪贴板上挂着的格式列出来**。这次排查就是因为缺这一行
             * 才多花了两轮：微信里"序号变了、owner 也对，内容却是空的"，
             * 有格式列表一眼就能看出该读哪种格式。 */
            win::log(&format!(
                "capture: round={round} 未取到词 changed={changed} 剪贴板={}",
                clip::describe_formats()
            ));
        }
        if valid {
            got = text;
            break;
        }
        // 没拿到有效文字：可能软件还没反应过来，换条通道重试一轮
    }
    /* ⚠ **无条件还原**，快照是空的也要还原。
     *
     * 用户的硬要求是「选中就弹条，不是选中就复制」—— 取词借用了剪贴板，就必须
     * 保证剪贴板**看不出被动过**。快照为空时跳过还原（我上一版为了"别抹掉对端
     * 迟到写入的数据"这么改过）会让选区留在剪贴板里，用户一眼就看到"选中就复制了"，
     * 这是实打实的体验事故。空快照的还原 = EmptyClipboard，正是我们要的"恢复到空"。
     *
     * ⚠ 但「还原」不能无条件：先把剪贴板序号和我们刚写完那一刻比一比，
     *   第三方在中间写过东西的话就**放手**（见 `clip::restore_if_unchanged`）。 */
    /* ⚠ 还原前必须把基准挪到"我们自己刚写完那一刻"。
     *
     * `snapshot()` 记的是**复制前**的序号；而中间那次 `send_ctrl_c()` /
     * `post_copy_to_focus()` 是**我们自己**在写剪贴板，序号必然变。
     * 不重新对齐的话 `restore_if_unchanged` 会把"我们自己的写入"误判成
     * "第三方改过"，于是**永远跳过还原** —— 选中的文字就永久留在剪贴板里，
     * 正是用户明确禁止的「选中就复制」。
     *
     * 对齐之后，基准 = 我们写完那一刻；此后若再有写入（用户手动 Ctrl+C、
     * 别的程序写了东西），才是真的"别人动过"，那时跳过还原才是对的。 */
    /* v2.23.31：**还原前要比内容，不能只信序号**。
     *
     * 这里原来是 `snap.note_our_write()` —— 把"此刻"的序号当成我们写的。
     * 但用户完全可能在这一瞬间自己按了 Ctrl+C（横条弹出来他觉得没复制上、
     * 或者他本来就想复制），那一刻的序号是**他的**写入。基准被骗过去之后
     * 下面的还原会把我们备份的旧内容写回剪贴板 —— 用户刚复制的东西就没了，
     * 表现正是他报的「Ctrl+C 有时候没复制上」。
     *
     * 现在交给 `restore_if_still_holds`：剪贴板里还是我们取走的那段文字才还原，
     * 出现了别的新内容就放手。 */
    if user_wrote_first {
        win::log("capture: 复制前剪贴板就已经变过（用户自己先复制了）—— 不还原，保住他那次复制");
    } else {
        clip::restore_if_still_holds(&snap, got.as_deref());
    }
    let t = got?;
    win::log(&format!("capture: 最终拿到 {} chars", t.chars().count()));
    Some(t)
}

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

/// 「精确复核」专用的轻量版模拟复制：**单轮、有预算**。
///
/// 只为一件事服务：划词落到 OCR 通道时，把认歪的那份文字换成**原文**
/// （触发条件见 `exact_reread_why`）。
///
/// # 为什么没有直接复用 `capture_selection`
///
///   · 它最多跑 3 轮、每轮读窗口 1.5s / 0.8s / 0.8s（最坏 3.7s），而且全程
///     握着 `CLIP_LOCK` —— 复核是**后台补丁**，卡这么久会把用户下一次
///     Ctrl+C 的「迟到补」一起堵在锁外面；
///   · 它没有"预算"这个概念，而这里必须在 `budget_ms` 内收工。
///
/// # 剪贴板纪律（和 `capture_selection` 逐条对齐）
///
///   · 先快照再复制，**读不到也要还原**（用户明确要求「选中不是复制」）；
///   · 还原前用 `note_our_write()` 把基准挪到"我们自己写完那一刻"，
///     否则 `restore_if_unchanged` 会把我们自己的写入误判成"第三方改过"，
///     永远跳过还原 —— 选区就永久留在剪贴板里；
///   · 这一轮**根本没写成功**（序号没动）就整段跳过还原：剪贴板一动没动，
///     去还原反而可能把别人刚写进去的东西抹掉。
/// 「这个窗口最近对合成 Ctrl+C 没反应」——短时备忘，避免对同一目标反复空等。
///
/// # 为什么要它（实测）
///
/// 用户日志里 `capture(exact): 700ms 内没读到新内容（目标 pid=… 写没写=false）`
/// 是**成对**出现的：同一次取词会复核两遍，两遍都注定失败，白等 1.4 秒。
/// 一个程序不会在 100ms 内从"不响应合成按键"变成"响应"，所以记住它、
/// 5 秒内直接跳过。
static EXACT_DEAF: std::sync::OnceLock<std::sync::Mutex<Vec<(u32, std::time::Instant)>>> =
    std::sync::OnceLock::new();

fn exact_recently_deaf(pid: u32) -> bool {
    let m = EXACT_DEAF.get_or_init(|| std::sync::Mutex::new(Vec::new()));
    let Ok(g) = m.lock() else { return false };
    g.iter()
        .any(|(p, at)| *p == pid && at.elapsed() < Duration::from_secs(5))
}

fn exact_mark_deaf(pid: u32) {
    let m = EXACT_DEAF.get_or_init(|| std::sync::Mutex::new(Vec::new()));
    if let Ok(mut g) = m.lock() {
        g.retain(|(p, at)| *p != pid && at.elapsed() < Duration::from_secs(5));
        g.push((pid, std::time::Instant::now()));
    }
}

fn exact_clear_deaf(pid: u32) {
    let m = EXACT_DEAF.get_or_init(|| std::sync::Mutex::new(Vec::new()));
    if let Ok(mut g) = m.lock() {
        g.retain(|(p, _)| *p != pid);
    }
}
/// 「这个窗口读不到选区」——每个进程只提示一次（本次运行内）。
///
/// # 为什么要提示
///
/// 微信 4.x（Qt）、部分 WPF 程序**没有 UIA 文本模式**，而它们又会忽略我们
/// 模拟的 Ctrl+C。用户在那种窗口里划词的结果是：**什么都不发生**——
/// 按用户 2026-10-30 的口径（常规划词不调 OCR，因为慢且不稳），这是
/// 设计内的取舍。
///
/// v2.23.64 起**只记日志、不弹任何提示**（用户口径：「提示好烦人」）。
fn hint_no_uia_once(pid: u32) {
    /* v2.23.64：**只写日志，不弹任何提示**（用户口径：「提示好烦人」）。
     *
     * 这里以前会发一条小提示条 + 一次托盘气泡，告诉用户"这个软件不支持
     * 划词弹条"。听上去有用，实际是噪音：微信里每划一次词就说明一遍，
     * 用户只想安静地用。
     *
     * 现在把结论留在 log.txt 里就够了 —— 真要排查的时候翻日志能看到
     * "这个 pid/exe 读不到选区"，不需要打扰正在用电脑的人。
     * 函数名保留（调用点不用改），语义变成"记一笔"。 */
    if pid == 0 {
        return;
    }
    let exe = win::process_exe(pid).unwrap_or_default();
    win::log(&format!(
        "hint: pid={pid} exe={exe} 读不到选区 —— 该软件不支持划词取词（静默，不提示）"
    ));
}
pub(crate) fn capture_selection_fast(
    target_pid: u32,
    budget_ms: u64,
    target_hwnd: Option<HWND>,
) -> Option<String> {
    /* ⚠ 先确认「前台还是它」再塞 Ctrl+C。
     *
     * 这条复核是在横条**已经弹出来之后**跑的（迟到几十~几百毫秒）。这段时间里
     * 用户完全可能已经点到别的软件里去了 —— 那时我们再把 Ctrl+C 发出去，就会
     * 把**另一个程序的选区**复制回来，然后当成这次划词的文字补进横条：用户点
     * 引擎就会去搜一段他根本没选的文字，比"认错字"更难理解。
     *
     * 允许"前台是我们自己"（横条 / 面板抢到焦点时属正常），只挡第三方。 */
    if target_pid != 0 {
        let fg = win::window_pid(win::foreground_window());
        let me = std::process::id();
        /* v2.11.4：**同一个程序的另一个进程也算"还是它"**。
         *
         * 微信 4.x（Qt，多进程）和 Chromium 系都是「主进程 + 一堆渲染/助手进程」：
         * 鼠标底下的那个窗口属于渲染进程，占着前台的却是主进程 —— pid 天然对不上。
         * 老判据只认 `fg == target_pid`，于是**每一次**补字都走进
         * 「前台已经换人」直接放弃，横条永远是空的（用户看到的就是
         * "在微信里选中文字不弹条"，虽然条其实弹了、只是没字）。
         *
         * 换成按**可执行文件**比对：同一个 exe = 同一个软件，把选区复制回来
         * 正是用户刚才那一下的意图；跨软件仍然照旧拦住。 */
        let same_app = same_app_pid(fg, target_pid);
        if fg != target_pid && fg != me && !same_app {
            /* v2.23.10：把两边的 exe 也写进日志。
             * 光看 pid 分不清"真的是两个软件"还是"同一个软件的不同进程"
             * （浏览器就是主进程 + 一堆渲染进程）—— 上一次排查就是卡在这。 */
            win::log(&format!(
                "capture(exact): 前台已经换人（pid {fg}={:?}，目标 {target_pid}={:?}）—— 不发 Ctrl+C，放弃这次复核",
                win::process_exe(fg),
                win::process_exe(target_pid)
            ));
            return None;
        }
    }
    /* 用户可能还没松开鼠标：`WM_LBUTTONUP` 已经在钩子里了，但目标程序
     * 处理"选区落定"还要一点时间。以前这里只睡 20ms，浏览器 / Electron
     * 经常还没来得及把选区提交，合成的 Ctrl+C 就发出去了 —— 日志里
     * `写没写=false` 大多是这个时序。等物理左键抬起后再给 70ms 稳定期。
     * 这一步放在锁剪贴板**之前**：等待期间不要占着全局剪贴板锁。 */
    let release_deadline = std::time::Instant::now() + Duration::from_millis(350);
    while win::left_down() && std::time::Instant::now() < release_deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(70));
    /* v2.23.81：同上 —— 用户按着 Ctrl/Alt/Win 时把这次复核让给他，
     * 绝不和用户自己的 Ctrl+C 抢时序。 */
    if win::ctrl_down() || win::alt_down() || win::win_down() {
        win::log("capture(exact): 用户还按着 Ctrl/Alt/Win —— 不模拟复制");
        return None;
    }
    let _clip = lock_clip();
    /* ⚠ 快照拿不到就**绝不能发 Ctrl+C**（2026-10-30 用户明确要求：
     *   "复制操作之后不能影响原来的剪贴板"）。
     *
     * 快照失败 = 剪贴板被别的程序占着（`OpenClipboard` 300ms 内都没抢到）。
     * 这时候如果照旧复制，用户原来的内容就**没有备份可还原**，直接被这次选区覆盖 ——
     * 那正是"复制完原来的内容没了"。宁可这次取不到字（横条保持空的），
     * 也不能动用户的剪贴板。 */
    let Some(snap) = clip::snapshot() else {
        win::log("capture(exact): 剪贴板快照失败（被占用）—— 不发 Ctrl+C，保持用户剪贴板原样");
        return None;
    };
    let before = clip::seq();
    /* v2.23.32：用户按着修饰键时**不发**（见 `win::modifiers_down`）——
     * 宁可这一轮复核拿不到字，也不能把他的 Ctrl 松开、把复制搅坏。 */
    /* v2.23.73：**横条自己抢了前台时，先把焦点还给目标窗口再发 Ctrl+C**。
     *
     * 实测日志（Edge / 抖音网页）：`start_capture: 没拿到文字，仍然弹条（兜底）`
     * 先把横条显示出来，紧接着 `capture(exact)` 发 Ctrl+C —— 但那时前台已经是我们
     * 自己的横条窗口，按键全打在自己身上，剪贴板当然"写没写=false"，于是一整类
     * 浏览器 / 应用都表现成"划词不弹条"。
     * 有精确的目标 hwnd（拖选起点）就用它；没有才退回 `LAST_FOREIGN_FG`。 */
    let fg_now = win::foreground_window();
    if win::window_pid(fg_now) == std::process::id() {
        let exact = target_hwnd
            .map(win::root_window)
            .filter(|h| !h.0.is_null());
        let last = LAST_FOREIGN_FG.load(Ordering::Relaxed);
        let restore = exact.or({
            if last != 0 {
                Some(HWND(last as *mut c_void))
            } else {
                None
            }
        });
        if let Some(h) = restore {
            /* v2.23.122：不只切前台，还要把键盘焦点交回目标线程 ——
             * 横条的 WebView 可能还攥着焦点，Ctrl+C 会打在我们自己身上。 */
            let ok = win::focus_for_copy(h);
            let t = std::time::Instant::now();
            while t.elapsed() < Duration::from_millis(250) {
                if win::window_pid(win::foreground_window()) != std::process::id() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let now = win::window_pid(win::foreground_window());
            let focus = win::gui_focus(win::foreground_window());
            win::log(&format!(
                "capture(exact): 前台是我们自己 —— 已把焦点还给目标窗口再复制（ok={ok} pid={now} focus={:?} class={}）",
                focus.0,
                win::class_name(focus)
            ));
        }
    }
    /* 等待/还焦点期间用户可能又切走了；发送前最后确认一次。 */
    let fg_final = win::foreground_window();
    if win::window_pid(fg_final) == std::process::id() {
        win::log("capture(exact): 还焦点失败，前台仍是本程序 —— 不发 Ctrl+C");
        return None;
    }
    if !foreground_still_target(target_pid) {
        win::log(&format!(
            "capture(exact): 发送前前台已换人（目标 pid={target_pid}）—— 放弃"
        ));
        return None;
    }
    if is_console_window(fg_final) || is_console_window(win::root_window(fg_final)) {
        win::log("capture(exact): 前台是终端 —— 绝不发 Ctrl+C");
        return None;
    }
    /* 诊断：前台 vs 键盘焦点。只在前台/焦点不是目标时多打一行，避免日志噪音；
     * 真出问题时能一眼看出"前台是 Edge，焦点却还在横条"。 */
    let focus_final = win::gui_focus(fg_final);
    if win::window_pid(focus_final) == std::process::id() {
        win::log(&format!(
            "capture(exact): 焦点还在本程序（fg pid={} focus={:?} class={}）—— 先交还焦点再发",
            win::window_pid(fg_final),
            focus_final.0,
            win::class_name(focus_final)
        ));
        let _ = win::focus_for_copy(win::root_window(fg_final));
        std::thread::sleep(Duration::from_millis(30));
    }
    if win::send_ctrl_c() {
        // 正常通道
    } else {
        /* 用户正按着 Ctrl/Shift（扩选、Ctrl 触发模式很常见）。不能等死：
         * 先用不依赖键盘状态的 WM_COPY 试一次；循环里 600ms 时还会再试
         * 一次 Ctrl+C —— 那时用户多半已经松手了。 */
        win::log("capture(exact): 用户正按着修饰键 —— 先改发 WM_COPY，并继续等松手");
        let _ = win::post_copy_to_focus();
    }
    /* v2.23.39：这个窗口 5 秒内刚证明过"对合成 Ctrl+C 没反应"就直接认输，
     * 不再陪着空等一整轮（实测同一目标会被复核两遍）。 */
    /* v2.23.61：**微信不吃"刚证明不响应"这套缓存**。它的失败多半是那一轮
     * 时序问题（窗口没抢到前台 / 菜单遮挡），不是真的不支持复制；直接跳过
     * 会让用户"第一次失败之后再也试不出来"。 */
    let wechat = is_wechat_pid(target_pid);
    if !wechat && exact_recently_deaf(target_pid) {
        win::log(&format!(
            "capture(exact): 目标 pid={target_pid} 刚证明不响应合成复制，跳过这一轮（不再空等）"
        ));
        clip::restore_if_still_holds(&snap, None);
        return None;
    }
    /* 微信给更长的窗口：它要先做一次内部复制（Qt 事件循环 + MMUI 排版），
     * 700ms 实测不够；1.5 秒能覆盖绝大多数情况，用户也不会觉得慢。 */
    let budget_ms = if wechat { budget_ms.max(1500) } else { budget_ms };
    let t0 = std::time::Instant::now();
    /* v2.23.74：Ctrl+C 没反应就补一次 Ctrl+Insert（见 win::send_ctrl_insert）。
     * 实测 Edge 某些页面（抖音网页评论区）对注入的 Ctrl+C 不写剪贴板，
     * 但对 Ctrl+Insert 有反应。两者语义一样，都只是"复制选中内容"。 */
    let mut tried_insert = false;
    let mut resent_ctrlc = false;
    let mut got: Option<String> = None;
    let mut left_target = false;
    while t0.elapsed() < Duration::from_millis(budget_ms) {
        /* v2.23.39：**用户已经切到别的窗口了就别再等**。
         *
         * 实测（用户日志）：这条复核经常"等满 700ms 然后失败"——
         * 因为它对着一份**早就不会再响应合成 Ctrl+C** 的程序傻等
         * （日志里 `写没写=false` 成对出现，每对 1.4 秒）。
         * 而窗口已经换了人，这次复核拿到的也只会是别人的剪贴板。
         * 提前收手还有一个好处：`CLIP_LOCK` 少被占 700ms，
         * 用户自己按 Ctrl+C 时不会撞上我们。 */
        let fg_pid = win::window_pid(win::foreground_window());
        if fg_pid != target_pid
            && fg_pid != std::process::id()
            && !same_app_pid(fg_pid, target_pid)
        {
            left_target = true;
            break;
        }
        if clip::seq() != before {
            if let Some(t) = clip::read_text_any() {
                let t = t.trim().to_string();
                // 「和复制前不一样」才是硬证据（见 capture_selection 同一段注释）
                /* 同上：内容与复制前相同也可能是"重新选中了同一段"。
                 * 只要求非空 + 剪贴板归属可信；序号已经在上面确认变过。 */
                if !t.is_empty()
                    && t.chars().count() <= 20000
                    && clipboard_owned_by(target_pid)
                {
                    got = Some(t);
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(25));
            /* 300ms 还没动静 → 换 Ctrl+Insert；600ms 还没动静 → 再补一次 Ctrl+C */
            let spent = t0.elapsed().as_millis();
            if !tried_insert && got.is_none() && clip::seq() == before && spent >= 300 {
                tried_insert = true;
                /* 横条 / 面板可能刚把前台抢走。降级通道发送前再把目标窗口
                 * 顶回前台一次 —— 否则 Ctrl+Insert 和 WM_COPY 都打在我们自己
                 * 身上，日志只会留下"两个通道都没写"。 */
                if win::window_pid(win::foreground_window()) == std::process::id() {
                    if let Some(h) = target_hwnd.map(win::root_window) {
                        let ok = win::focus_for_copy(h);
                        std::thread::sleep(Duration::from_millis(40));
                        win::log(&format!(
                            "capture(exact): 降级通道前把目标顶回前台（ok={ok} 现在 pid={}）",
                            win::window_pid(win::foreground_window())
                        ));
                    }
                }
                win::log("capture(exact): Ctrl+C 没写剪贴板 —— 改发 Ctrl+Insert + WM_COPY 再试");
                /* WM_COPY 走窗口消息，不依赖输入队列，能绕过一部分
                 * 对合成按键的过滤（Chromium / Qt 都值得试一次）。 */
                let _ = win::post_copy_to_focus();
                let _ = win::send_ctrl_insert();
            } else if !resent_ctrlc && got.is_none() && clip::seq() == before && spent >= 600 {
                resent_ctrlc = true;
                win::log("capture(exact): Ctrl+Insert 也没写 —— 再补一次 Ctrl+C");
                let _ = win::send_ctrl_c();
            }
    }
    let wrote = clip::seq() != before;
    /* 记一笔：没写进剪贴板 = 这个窗口不响应合成复制，5 秒内别再来一次 */
    if !wrote && got.is_none() {
        exact_mark_deaf(target_pid);
    } else if wrote {
        exact_clear_deaf(target_pid);
    }
    if wrote {
        /* v2.23.31：**同样不能只信序号**（和 capture_selection 一个道理）。
         * 用户在这一小段时间里自己 Ctrl+C 的话，序号同样会变，而"变"既可能
         * 是我们那次合成复制、也可能是他这次手动复制 —— 用内容判断才安全：
         * 剪贴板里还是我们取到的那段文字才还原，否则说明他复制了新东西。
         *
         * v2.23.61（用户口径）：**微信延迟到横条收起时再还** ——
         * 「选择文本就是复制，横条消失后恢复剪贴板」。这段时间里剪贴板就是
         * 那段选中文字，用户随时可以 Ctrl+V；其他程序仍然立刻还原。 */
        if wechat && got.is_some() {
            clip::defer_restore(snap, got.clone());
            win::log("capture(exact): 微信 —— 剪贴板先留着选中文字，横条收起时再还原");
        } else {
            clip::restore_if_still_holds(&snap, got.as_deref());
        }
    }
    if got.is_none() {
        win::log(&format!(
            "capture(exact): {} 内没读到新内容（目标 pid={target_pid} 写没写={wrote}）{}",
            if left_target {
                format!("提前收手（已用了 {}ms）", t0.elapsed().as_millis())
            } else {
                format!("{budget_ms}ms")
            },
            if left_target { "，前台已经换了窗口" } else { "" }
        ));
    }
    got
}

/// 划词取词（**全程不碰剪贴板**）：标准编辑控件 / UIA 直读选区。
///
/// 用户的口径说得很明确：**「选中文字就弹条，不是选中就复制」**。
/// 老实现是靠「模拟一次 Ctrl+C + 立刻还原剪贴板」把文字拿到手的 —— 剪贴板终究被
/// 动过（还原失败、或者被别的程序占着还原不了时，用户就能看到选区留在剪贴板里），
/// 而且遇到会过滤合成按键的程序（微信）直接哑火。
///
/// 现在两条通道都不碰剪贴板：
///   ① 标准编辑控件（EDIT / RichEdit）：直接读控件自己的选区；
///   ② `UIA`：用系统无障碍接口直接问程序"你选了什么" —— 取到的是**精确原文**。
///
/// 两条都读不到就放弃（v2.23.121 起不再用屏幕 OCR 兜底：屏幕上有字
/// 不等于用户选中了它）。
///
/// 返回 `(文字, 通道名)`，通道名进日志，方便一眼看出这次走的是哪条通道。
pub(crate) fn capture_without_clipboard(drag: (i32, i32, i32, i32)) -> Option<(String, &'static str)> {
    let pt_end = windows::Win32::Foundation::POINT { x: drag.2, y: drag.3 };
    let pt_start = windows::Win32::Foundation::POINT { x: drag.0, y: drag.1 };
    /* ⓿ 选区落地稳定期：快速划选（dt<30ms）时 mouseup 刚到目标窗口的消息
     *    队列，此时查 EM_GETSEL 读到的是**旧选区**（快速划选取词实测 A1/A2
     *    因此读到旧选区、把「QuickAchieve」认成「search」丢了开头）。
     *    与 Ctrl+C 链同口径（见 main.rs:1223 的稳定期常量），60ms 对弹条
     *    总耗时（几百毫秒）无感。 */
    std::thread::sleep(Duration::from_millis(60));
    /* ① 标准编辑控件（EDIT / RichEdit）：直接读控件自己的选区，**精确**且不碰剪贴板。
     *    记事本、各种输入框走这条 —— 它们**没有 UIA 文本模式**，UIA 问不到。 */
    if let Some(t) = win::edit_selection_text_at(pt_end) {
        if t.chars().count() <= 20000 {
            return Some((t, "编辑控件"));
        }
    }
    /* ② UIA 直读选区。**浏览器整体跳过**（r34 实证：Chromium 系 UIA 内容树
     *    无法唤醒，见 win::uia_foreground_is_browser 的长注释）—— 不跳的话
     *    每次划词要白等两次 UIA 查询（合计最多 5s），横条弹出直接被拖垮。 */
    if win::uia_foreground_is_browser() {
        win::log("capture: 前台是浏览器（UIA 内容树无法唤醒）—— 跳过 UIA，按取不到处理");
    } else {
        /* 选区的提交有时比 mouseup 晚一拍（浏览器要把"选区变了"从渲染进程同步回来），
         *    立刻问会问到一个空选区。重试两次、间隔 120ms，够它落地了。
         *
         *    ⚠ 但**超时不算"晚一拍"**：超时说明对方的 UIA 压根不应答，再等一次只是
         *    又白花 0.5~2.2s（实测），用户看到的是"选完字要等四五秒才出条"。
         *    所以超时直接跳出重试、马上放弃，不再白等。 */
        for attempt in 0..2 {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(120));
            }
            match win::uia_selection_text(pt_end, pt_start) {
                win::UiaTry::Text(t) => {
                    if t.chars().count() <= 20000 {
                        return Some((t, "UIA"));
                    }
                }
                win::UiaTry::TimedOut => {
                    win::log("capture: UIA 查询超时（对方 UIA 不应答）—— 不重试，按取不到处理");
                    break;
                }
                win::UiaTry::Retryable => {}
            }
        }
    }
    /* ⚠ 常规划词**不调用 OCR**（2026-10-30 与 v2.23.121 用户口径）。
     *
     * 旧链路是：UIA 读不到选区 → 截屏 OCR 兜底。它有两个害处：
     *   ① **慢**：一次兜底可能把取词卡住十几秒，期间后续划选全被 BUSY 丢弃，
     *      用户看到的就是「划了不弹」（实测日志连着三条
     *      `上一次取词超过 4.5s 仍未结束，本次放弃`）；
     *   ② **不稳**：浏览器里屏幕认字对中英混排小字本就不可靠，认不出等于
     *      没取到文字，再叠加「没取到文字就不弹」，横条干脆不出现。
     *
     * 现在 OCR 只在用户**主动**点 OCR 按钮 / 识图按钮时才执行（走
     * open_ocr_window 那条独立链路：region.html → ocr.rs），常规流程完全绕过。
     * 取舍写在明处：浏览器这类 UIA 读不到选区的程序，划词将拿不到文字。 */
    win::log("capture: UIA 读不到选区（该程序没有 UIA 文本模式）—— 常规流程不走 OCR 兜底");
    None
}

/* ================================================================== */
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

