//! 横条展示：决定内容与位置，发 `qs://show` 给前端。

use std::ffi::c_void;
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::{json, Value};
use windows::Win32::Foundation::{HWND, POINT};

use crate::config;
use crate::t::Emitter;
use crate::win;
use crate::{app, app_locked, bar_hwnd, cfg_with, current_theme, focus_lock_window, is_bar_shown, layout_bar, remember_text, selection_box_from_drag, st, trigger_target_window};
use crate::{sel_urls_snapshot, BAR_ACTIVITY, GEN, GRID_PENDING, OCR_ONLY, OFFSCREEN, SHOW_T0};

/// 横条实体：把「这次要显示什么」决定好，发给前端去画。
///
/// `sel` 是这次拖选的矩形（按下点 → 抬起点，屏幕物理 px），只有划词那条路拿得到；
/// 只用来兜底决定横条在上侧还是下侧，见 [`selection_box_from_drag`]。
pub(crate) fn show_bar_at(
    text: String,
    at_mouse: bool,
    via_copy: bool,
    sel: Option<(i32, i32, i32, i32)>,
) -> usize {
    let t0 = std::time::Instant::now();
    /* v2.11.0：这里原来有一个 `let is_grid_show = GRID_PENDING.load(..)`，
     * 只被函数末尾一个**空的** `if !is_grid_show { }` 用着 —— 那是 v2.10.0
     * 删掉"横条/网格页一弹出就播报"之后留下的壳。通知现在只在用户**真的用了**
     * 某功能时发（见 notify_feature），这里不再需要区分弹的是哪块面板。 */
    /* v1.5.13：锁屏期间横条一律不弹。
     *
     * 这是所有弹出路径的**唯一汇合点**（划词钩子 / Ctrl+C / 托盘 / 收藏夹热键
     * 最后都走到这里），所以门禁放在这一行就够了 —— 但为了不给用户"按了没反应"
     * 的错觉，顺手把锁屏窗顶到前面来（他大概是在别的窗口里试着划词）。
     */
    if app_locked() {
        win::log("show_bar: 未解锁，忽略");
        focus_lock_window();
        return 0;
    }
    /* ⚠ v1.4.0：**日志里不记用户选中的文字**，只记字数。
     *
     * 用户要求「所有的本地数据都要加密，不能以任何明文方式显示任何的用户数据」。
     * 日志是明文文件，以前这一行把划到的原文（最多 60 字）原样写进去 ——
     * 那是货真价实的用户数据。排查"有没有取到词"看字数就够了。 */
    win::log(&format!("show_bar: {} 字", text.chars().count()));
    BAR_ACTIVITY.fetch_add(1, Ordering::SeqCst);
    let a = app();
    // 选区 / 选区 URL 的共享状态走 remember_text，和"迟到补丁"用同一套，
    // 免得两条路各写各的、前后端对不上（点引擎到底搜哪份文字那种事）。
    let url = remember_text(&text);

    // 位置：
    //   · 开了「跟随鼠标」或者这条是 Ctrl+C 触发 -> 贴鼠标；
    //   · 否则贴选中的文字：**优先用这次拖选的轨迹**，没有轨迹（Ctrl+C /
    //     键盘选词）才退回真光标矩形。上下侧由「外观 → 弹出位置」决定，
    //     放不下自动翻侧，那步在 layout_bar 里做（要等前端量出胶囊的真实高度）。
    //
    // ⚠ 为什么拖选优先于真光标（v1.5.5）：
    //   ① 真光标只有原生文本控件才给，浏览器 / Electron 里永远是 None ——
    //      只认它的话「上方/下方」两个选项完全没差别（用户报的 bug）；
    //   ② **多行选区**时真光标在最后一行，按它判「上方」会把横条摆到最后一行
    //      上面、盖住上面几行；拖选轨迹跨所有行，才是"选区整块"的范围。
    /* 只读一个布尔值：整份 Config 里可能有收藏夹图片的 dataURL（几 MB），
     * 每次划词都克隆一遍纯属浪费（这是弹出路径上的热调用）。 */
    let anchor_at_mouse = cfg_with(|c| c.anchor_at_mouse);
    let (caret, caret_src) = if at_mouse || anchor_at_mouse {
        (None, "none")
    } else {
        match sel {
            Some(d) => {
                // 行高按 CSS px 折算，缩放得取**用户本来那个软件**的
                // （横条窗口此刻还停在屏幕外，问它没意义）。
                let sc = win::dpi_scale(win::foreground_window());
                (Some(selection_box_from_drag(d, sc)), "drag")
            }
            None => match win::caret_box() {
                Some(c) => (Some(c), "caret"),
                None => (None, "none"),
            },
        }
    };
    let anchor = match caret {
        Some((cl, _, _, cb)) => POINT { x: cl, y: cb },
        None => win::cursor_pos(),
    };
    win::log(&format!(
        "show_bar: atMouse={at_mouse} cfgMouse={} caret={:?} src={caret_src} anchor=({},{})",
        anchor_at_mouse, caret, anchor.x, anchor.y
    ));
    let t_anchor = t0.elapsed();

    /* —— 记下「这次是在哪个软件、什么位置被叫出来的」——
     *
     * 收藏夹条目点下去要**注入到那个软件的文本框**里（用户口径：输入到鼠标触发
     * 位置所在的其他软件；只有那儿没有可用文本框时才复制到剪贴板）。而收藏面板
     * 一打开**前台窗口就变成我们自己**，那两个问题当场就问不出来了，只能现在留档。
     *
     * 选窗口的顺序：鼠标底下那个**别的进程**的顶层窗口 → 当前前台窗口（同样得是别人）。
     * 两个都指向我们自己（例如从设置页/托盘唤起的）→ 没有可注入的目标，
     * 条目一律走复制到剪贴板。 */
    let TriggerSite { hwnd: trigger_hwnd, has_text: trigger_has_text, fg: trigger_fg, app: trigger_app, pt: trigger_pt } = trigger_site();
    win::log(&format!(
        "show_bar: 触发现场 hwnd=0x{:x} 「{}」 有可用文本框={} 鼠标=({},{})",
        trigger_hwnd.map(|h| h.0 as usize).unwrap_or(0),
        trigger_hwnd.map(win::window_title).unwrap_or_default(),
        trigger_has_text,
        trigger_pt.0,
        trigger_pt.1
    ));

    park_previous_bar();
    let t_off = t0.elapsed();

    let theme = current_theme();
    win::log(&format!(
        "show_bar: 耗时 锚点={}ms 让位={}ms 主题={}ms",
        t_anchor.as_millis(),
        (t_off - t_anchor).as_millis(),
        (t0.elapsed() - t_off).as_millis()
    ));

    {
        let mut g = match st().geo.lock() {
            Ok(g) => g,
            Err(_) => return GEN.load(Ordering::SeqCst),
        };
        g.anchor = Some((anchor.x, anchor.y));
        g.anchor_mouse = caret.is_none();
        g.caret = caret;
        g.trigger_hwnd = trigger_hwnd.map(|h| h.0 as isize).unwrap_or(0);
        g.trigger_has_text = trigger_has_text;
        g.trigger_fg = trigger_fg;
        g.trigger_pt = trigger_pt;
        g.shown = false; // 等前端回 layout_bar 再真正显示
    }

    let gen = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    // 记下这次唤起的起点，layout_bar 里窗口真的可见时把端到端耗时打出来
    if let Ok(mut g) = SHOW_T0.lock() {
        *g = Some(t0);
    }
    /* 横条的「按应用」解析：这次唤起的进程名命中专项配置就用它，否则用默认
     * `bar_list`。判定逻辑全在 `config::resolve_bar` 里（和网格页同一套），
     * 这里只负责把结果塞进载荷。取进程名的口径和网格页完全一致（`trigger_app`）。 */
    let bar_resolved = cfg_with(|c| config::resolve_bar(c, &trigger_app));
    let _ = a.emit_to("bar", "qs://theme", theme);
    let _ = a.emit_to(
        "bar",
        "qs://show",
        json!({
            "text": text,
            "isUrl": url.is_some(),
            // 这一段里的**所有**链接：> 1 条时前端那颗按钮点一下就把它们
            // 分别开成多个标签页（用户要求）。
            "urls": sel_urls_snapshot(),
            "ocrOnly": OCR_ONLY.swap(false, Ordering::SeqCst),
            // 这次横条是「用户刚复制/拖选」叫出来的吗？收藏夹条目点下去要据此分流：
            // 取词语境 → 复制到剪贴板；否则 → 填进搜索框（用户明确要求分清）。
            "viaCopy": via_copy,
            /* 网格页载荷：非 null 时前端**只画网格、不画文字横条**。
             * `swap` 保证这个标记只对本次唤起作用 —— 下一次划词绝不可能
             * 误继承上一次的网格模式（两条触发方式互不干扰的关键）。 */
            /* 横条内容（按应用解析后的结果）。
             * `source` = "app"（用了专项）| "default"（回退默认），
             * `note` 是回退原因（设置页/日志用）。前端只在 source 为 "app" 时
             * 才用 `items` 覆盖 `CONF.barList`，否则沿用默认，保证两条路径同源。 */
            "bar": {
                "items": bar_resolved.items,
                "source": bar_resolved.source,
                "matched": bar_resolved.matched,
                "note": bar_resolved.note,
            },
            "grid": if GRID_PENDING.swap(false, Ordering::SeqCst) {
                let (rows, cols, cw, ch, gap, radius, click, allow_add, show_label, pages, gres) =
                    cfg_with(|c| {
                        /* 按应用选页 + 回退规则都在这一句里（见 config::resolve_grid_page）：
                         * 应用有专项页就严格用它；专项页缺失/为空/内容全不可用 → 默认页。 */
                        let gres = config::resolve_grid_page(c, &trigger_app);
                        (
                            c.grid_rows,
                            c.grid_cols,
                            c.grid_cell_w,
                            c.grid_cell_h,
                            c.grid_gap,
                            c.grid_radius,
                            c.grid_click.clone(),
                            c.grid_allow_add,
                            c.grid_show_label,
                            c.grid_pages
                                .iter()
                                .map(|p| json!({
                                    "id": p.id, "name": p.name, "items": p.items,
                                }))
                                .collect::<Vec<_>>(),
                            gres,
                        )
                    });
                json!({
                    "rows": rows.clamp(1, 10),
                    "cols": cols.clamp(1, 10),
                    "cellW": cw.clamp(40, 260),
                    "cellH": ch.clamp(40, 200),
                    // 间距 / 格子圆角：和设置页预览共用同一个值，否则预览和
                    // 真弹出来的格子对不上（用户点名要求"必须完全一致"）
                    "gap": gap.clamp(0, 24),
                    "radius": radius.clamp(0, 24),
                    // 点格子的行为：auto = 有文字就搜；home = 永远开首页
                    "click": click,
                    // 空位能不能点开「添加内容」面板
                    "allowAdd": allow_add,
                    // 格子里显示名称还是只显示图标（false = 只有图标）
                    "showLabel": show_label,
                    // 当前页的内容（老字段，仍然给）
                    "items": gres.resolved.items,
                    // 多页：整份页列表 + 这次该显示第几页；`app` 是触发它的进程名，
                    // 前端滚轮切页时把它一起回传，才能"记住这个应用看到哪页"。
                    "pages": pages,
                    "active": gres.index,
                    "app": trigger_app,
                    // 这次用的是哪一页、以及"专项 / 默认"的判定结果。
                    // 回退时 `active` 已经是默认页下标，所以页名/页码/编辑
                    // 全都指向同一页，不会自相矛盾。
                    "pageId": gres.id,
                    "pageName": gres.name,
                    "source": gres.resolved.source,
                    "matched": gres.resolved.matched,
                    "fallback": gres.resolved.note,
                })
            } else {
                Value::Null
            },
        }),
    );
    /* v2.11.0：删掉这里那段空的 `if !is_grid_show { }` 与它的注释。
     * 横条/网格页**弹出**不再播报（v2.10.0 的决定，用户原话：「不要再针对每个
     * 应用中出现划词弹出横条的情况各发一次通知」）—— 播报点现在全在"功能真的
     * 跑起来"那一侧：`ocr_start_flow` / `trans_start` / `img_start` / `text_op`
     * / `wrap_text`，见 `notify_feature` 的文档。 */
    win::log(&format!(
        "show_bar: 前置合计 {}ms → 已发事件，等前端量尺寸",
        t0.elapsed().as_millis()
    ));

    spawn_bar_fallback(gen);
    gen
}

/// 这次唤起是在哪个软件、什么位置发生的 —— 收藏夹注入与网格页「按应用」都要用。
///
/// 必须在横条弹出**之前**采：横条一出现前台就变成我们自己，那两个问题当场就问不出来了。
struct TriggerSite {
    hwnd: Option<HWND>,
    has_text: bool,
    fg: isize,
    app: String,
    pt: (i32, i32),
}

fn trigger_site() -> TriggerSite {
    let trigger_pt = {
        let c = win::cursor_pos();
        (c.x, c.y)
    };
    let trigger_hwnd = trigger_target_window();
    let trigger_has_text = trigger_hwnd.map(win::has_text_focus).unwrap_or(false);
    /* 前台窗口（我们自己不算）：键盘触发时它才是"用户正在操作的那个软件"，
     * 详见 Geo::trigger_fg 的注释。 */
    let trigger_fg = {
        let fg = win::foreground_window();
        if fg.0.is_null() || win::window_pid(fg) == std::process::id() {
            0
        } else {
            fg.0 as isize
        }
    };
    /* 触发这次唤起的**应用进程名**（如 `chrome.exe`）。
     * 网格页「按应用切换」全靠它：判断"这次该给哪一页"。
     * 取不到（受保护进程 / 已退出）就是空串 —— 空串永远走默认页，不会误绑。 */
    let trigger_app = {
        let h = if trigger_fg != 0 {
            trigger_fg
        } else {
            trigger_hwnd.map(|x| x.0 as isize).unwrap_or(0)
        };
        if h == 0 {
            String::new()
        } else {
            win::process_exe(win::window_pid(HWND(h as *mut c_void))).unwrap_or_default()
        }
    };
    TriggerSite {
        hwnd: trigger_hwnd,
        has_text: trigger_has_text,
        fg: trigger_fg,
        app: trigger_app,
        pt: trigger_pt,
    }
}

fn park_previous_bar() {
    /* 上一根横条还亮着就先把它挪出屏幕。
     *
     * ⚠ 一是挪动必须**同步**：以前用 Tauri 的 `set_position`（投递到事件循环的
     *    异步调用），然后盲等 36ms 猜它什么时候生效 —— 既慢又不准。改成直接
     *    `win::move_only`（一次 SetWindowPos，返回时窗口已经在屏幕外了）。
     * ⚠ 二是「为什么还需要挪」：自动取色删掉后（#148），「采到自己」的动机
     *    没了，但**液态玻璃的抓屏还吃这一趟** —— layout_bar 首次上屏前要
     *    BitBlt 一块干净桌面当玻璃纹理（见那儿的注释），旧横条若留在原地，
     *    抓到的就是它自己上一帧的玻璃渲染（纹理自拍）。所以挪照旧，
     *    无条件挪；只为抓屏干净，不需要再等 DWM 合成。 */
    if is_bar_shown() {
        let h = bar_hwnd();
        if !h.0.is_null() {
            win::move_only(h, OFFSCREEN, OFFSCREEN);
        }
    }
}

/// 前端 420ms 内没回 layout_bar 就用上次尺寸兜底显示，别让用户觉得没反应。
fn spawn_bar_fallback(gen: usize) {
    // 前端万一没回，就用上次的尺寸兜底显示，别让用户觉得没反应
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(420));
        if GEN.load(Ordering::SeqCst) != gen || is_bar_shown() {
            return;
        }
        let (w, h, ix, iy, iw, ih, bh, px, py, sx, ox, oy) = {
            let g = match st().geo.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            if g.css_w > 1.0 {
                // 钉住点没存过（理论上不会，防御性）就退回包围盒左缘
                let px = if g.css_pin_x > 0.0 { g.css_pin_x } else { g.css_ix };
                let py = if g.css_pin_y > 0.0 { g.css_pin_y } else { g.css_iy };
                (g.css_w, g.css_h, g.css_ix, g.css_iy, g.css_iw, g.css_ih, g.bar_h,
                 px, py, g.css_shift_x, g.css_off_x, g.css_off_y)
            } else {
                (552.0, 52.0, 10.0, 10.0, 532.0, 32.0, g.bar_h, 10.0, 10.0, 0.0, 0.0, 0.0)
            }
        };
        layout_bar(w, h, ix, iy, iw, ih, bh, px, py, sx, ox, oy, None);
    });
}
