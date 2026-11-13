//! 自动明暗：看**当前页面 / 当前软件的 UI** 是深色还是浅色，横条与网格跟着换皮。
//!
//! # 为什么要"多帧"
//!
//! 直接对光标周围截一张图取平均亮度，是最容易踩坑的做法：网页里正在播视频、
//! 桌面有动画/滚动，画面一亮一暗，横条就会跟着闪。
//!
//! 这里的判据是**时间稳定性**：先快速抓 3 帧（约 60ms），只有"基本没变"的
//! 像素才算 UI；帧间变化大的像素（视频、动画、滚动内容）**整块丢掉**。剩下的
//! 稳定像素取中位数亮度决定明暗。3 帧判不出来（视频/灰区）才补到 5 帧 ——
//! 常见静态界面不再等满 5 帧，判不出来的情况仍保留原来的稳定性。
//!
//! # 为什么是"光标周围"而不是整屏
//!
//! 用户实际在看的、横条要叠上去的，就是光标附近那块 —— 浏览器里是网页内容，
//! 别的软件里就是它自己的界面。取整屏会把任务栏 / 别的窗口也算进来。

use std::sync::Mutex;
use std::time::Instant;

use crate::glass;
use crate::win;

/// 最多抓几帧（判不出来时才补到这么多）
const FRAMES: usize = 5;
/// 先抓几帧做快速判定；静态 UI 通常到这一步就够了
const FAST_FRAMES: usize = 3;
/// 快速路径的帧间隔（毫秒）：30ms 足以让 30/60fps 的视频换帧，又不至于等太久
const FAST_GAP_MS: u64 = 30;
/// 判不出来、补帧时的间隔（毫秒）
const EXTRA_GAP_MS: u64 = 55;
/// 采样步长（像素）：每 6×6 取一个点，够用且快
const STEP: i32 = 6;
/// 任一通道帧间差超过它 → 这个像素"在动"（视频/动画），不参与判断
const UNSTABLE: i32 = 16;
/// 稳定像素少于此比例 → 这地方几乎全是动的，别猜
const MIN_STABLE: f64 = 0.25;
/// 亮度阈值：> HIGH 判浅色，< LOW 判深色，中间不猜（交给系统偏好）
const LUM_LIGHT: f64 = 0.56;
const LUM_DARK: f64 = 0.44;

/// 采样窗口（CSS 像素，逻辑尺寸）：以光标为中心的一块
const SAMPLE_W: i32 = 420;
const SAMPLE_H: i32 = 260;

/// 在当前光标周围判断"这块 UI 是深色还是浅色"。
///
/// 返回 `Some(true)` = 深色 UI（横条该用深色皮），`Some(false)` = 浅色，
/// `None` = 判断不了（截屏失败 / 画面全在动 / 亮度落在灰区）—— 交给调用方兜底。
pub fn detect() -> Option<bool> {
    let c = win::cursor_pos();
    let (x, y, w, h) = sample_rect(c.x, c.y);
    let mut frames: Vec<glass::Snap> = Vec::with_capacity(FRAMES);

    /* 快速路径：先抓 3 帧（约 60ms 等待 + 3 次抓屏）。
     * 静态 UI 到这里就能判出来；判不出来（视频/动画/灰区）再补 2 帧，
     * 这样既保留原来 5 帧的稳定性，又让常见情况少等 160ms。 */
    for i in 0..FAST_FRAMES {
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(FAST_GAP_MS));
        }
        if let Some(s) = glass::capture_rect(x, y, w, h) {
            frames.push(s);
        }
    }
    let mut out = decide_inner(&frames, false);

    if out.is_none() && frames.len() >= 2 {
        while frames.len() < FRAMES {
            std::thread::sleep(std::time::Duration::from_millis(EXTRA_GAP_MS));
            match glass::capture_rect(x, y, w, h) {
                Some(s) => frames.push(s),
                None => break,
            }
        }
        out = decide(&frames);
    }

    if let Some(dark) = out {
        win::log(&format!(
            "autotheme: 光标附近判定为 {} UI（{} 帧）",
            if dark { "深色" } else { "浅色" },
            frames.len()
        ));
    }
    out
}

/// 同一个窗口的判定结果复用多久（毫秒）。
///
/// 自动明暗的快速路径是 3 帧 / 2×30ms，实测约 90ms；判不出来才补到 5 帧。
/// 用户连续划词时同一个窗口的底色不会变，没必要每次都等。窗口一换就重新
/// 判定，所以不会把上一个程序的肤色带过来。
const CACHE_TTL_MS: u128 = 3000;

struct Cached {
    hwnd: isize,
    at: Instant,
    dark: bool,
}

static CACHE: Mutex<Option<Cached>> = Mutex::new(None);

/// 带缓存的 [`detect`]：同一前台窗口 3 秒内只判定一次。
///
/// 只缓存“判出来了”的结果；判不出来（视频/灰区）不缓存 —— 那种情况本来就
/// 该按当时的画面重新看，缓存 None 会把静态 UI 也一起误判。
pub fn detect_cached() -> Option<bool> {
    let hwnd = win::foreground_window().0 as isize;
    if let Ok(g) = CACHE.lock() {
        if let Some(c) = g.as_ref() {
            if cache_hit(c.hwnd, c.at.elapsed().as_millis(), hwnd, CACHE_TTL_MS) {
                win::log("autotheme: 复用 3 秒内的判定");
                return Some(c.dark);
            }
        }
    }
    let out = detect();
    if let Some(dark) = out {
        if let Ok(mut g) = CACHE.lock() {
            *g = Some(Cached {
                hwnd,
                at: Instant::now(),
                dark,
            });
        }
    }
    out
}

/// 纯逻辑：缓存是否还能用。抽出来是为了能单测时间/窗口两条边界。
fn cache_hit(cached_hwnd: isize, age_ms: u128, hwnd: isize, ttl_ms: u128) -> bool {
    cached_hwnd == hwnd && age_ms < ttl_ms
}

/// 当前主题要不要做屏幕明暗判定。
///
/// 只有「跟随页面」（`theme == "page"`，也就是用户说的自动取色）才需要；
/// 亮色 / 暗色 / 跟随系统都直接按设置或系统偏好算，不抓屏、不等帧。
pub fn needs_detection(theme: &str) -> bool {
    theme == "page"
}

/// 以光标为中心取一块，钳在虚拟屏幕内。
fn sample_rect(cx: i32, cy: i32) -> (i32, i32, i32, i32) {
    let (vx, vy, vw, vh) = glass::virtual_screen();
    let w = SAMPLE_W.min(vw.max(1));
    let h = SAMPLE_H.min(vh.max(1));
    let x = (cx - w / 2).clamp(vx, vx + vw - w);
    let y = (cy - h / 2).clamp(vy, vy + vh - h);
    (x, y, w, h)
}

/// 纯逻辑：从若干帧里判明暗。抽出来是为了能单测（不需要真的抓屏）。
pub fn decide(frames: &[glass::Snap]) -> Option<bool> {
    decide_inner(frames, true)
}

/// [`decide`] 的实现；`verbose` 控制“稳定像素太少”的日志。
/// 快速路径先静默试 3 帧，避免判不出来时同一条日志打两遍。
fn decide_inner(frames: &[glass::Snap], verbose: bool) -> Option<bool> {
    if frames.len() < 2 {
        return None;
    }
    let w = frames.iter().map(|f| f.w).min()?;
    let h = frames.iter().map(|f| f.h).min()?;
    if w <= 0 || h <= 0 {
        return None;
    }
    let mut lums: Vec<f64> = Vec::new();
    let mut sampled = 0usize;
    let mut y = 0;
    while y < h {
        let mut x = 0;
        while x < w {
            sampled += 1;
            let idx = ((y * w + x) * 4) as usize;
            // 帧间最大通道差
            let mut min = [255i32; 3];
            let mut max = [0i32; 3];
            let mut ok = true;
            for f in frames {
                if idx + 3 >= f.rgba.len() {
                    ok = false;
                    break;
                }
                for c in 0..3 {
                    let v = f.rgba[idx + c] as i32;
                    if v < min[c] {
                        min[c] = v;
                    }
                    if v > max[c] {
                        max[c] = v;
                    }
                }
            }
            if !ok {
                x += STEP;
                continue;
            }
            // 只要有一个通道在跳，就算"不是静态 UI"（视频/动画）
            if (0..3).any(|c| max[c] - min[c] > UNSTABLE) {
                x += STEP;
                continue;
            }
            let f = &frames[0];
            let (r, g, b) = (
                f.rgba[idx] as f64,
                f.rgba[idx + 1] as f64,
                f.rgba[idx + 2] as f64,
            );
            lums.push((0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0);
            x += STEP;
        }
        y += STEP;
    }
    if sampled == 0 {
        return None;
    }
    let ratio = lums.len() as f64 / sampled as f64;
    if ratio < MIN_STABLE {
        if verbose {
            win::trace(&format!(
                "autotheme: 稳定像素只有 {:.0}% —— 画面几乎全在动，不猜",
                ratio * 100.0
            ));
        }
        return None;
    }
    lums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = lums[lums.len() / 2];
    if mid >= LUM_LIGHT {
        Some(false)
    } else if mid <= LUM_DARK {
        Some(true)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(w: i32, h: i32, rgb: [u8; 3]) -> glass::Snap {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
        glass::Snap {
            x: 0,
            y: 0,
            w,
            h,
            rgba,
        }
    }

    #[test]
    fn 纯白界面判浅色纯黑判深色() {
        let white = [snap(60, 60, [250, 250, 250]), snap(60, 60, [250, 250, 250])];
        assert_eq!(decide(&white), Some(false), "白底 = 浅色 UI");
        let black = [snap(60, 60, [12, 12, 12]), snap(60, 60, [12, 12, 12])];
        assert_eq!(decide(&black), Some(true), "黑底 = 深色 UI");
    }

    #[test]
    fn 视频变化要被忽略() {
        // 上半屏是白的静态 UI，下半屏是"视频"：第 0 帧全白、第 1 帧全黑
        let make = |video: [u8; 3]| {
            let w = 60;
            let h = 60;
            let mut s = snap(w, h, [250, 250, 250]);
            for y in h / 2..h {
                for x in 0..w {
                    let i = ((y * w + x) * 4) as usize;
                    s.rgba[i] = video[0];
                    s.rgba[i + 1] = video[1];
                    s.rgba[i + 2] = video[2];
                }
            }
            s
        };
        let frames = [make([255, 255, 255]), make([0, 0, 0]), make([255, 255, 255])];
        assert_eq!(
            decide(&frames),
            Some(false),
            "下半屏在闪（视频）必须被丢掉，只剩上半屏的浅色 UI"
        );
    }

    #[test]
    fn 灰区不猜() {
        let grey = [snap(60, 60, [128, 128, 128]), snap(60, 60, [128, 128, 128])];
        assert_eq!(decide(&grey), None, "中间亮度交给系统偏好兜底，别硬猜");
    }

    #[test]
    fn 帧数不够不猜() {
        assert_eq!(decide(&[]), None);
        assert_eq!(decide(&[snap(10, 10, [0, 0, 0])]), None);
    }

    #[test]
    fn 只有跟随页面才需要抓屏判定() {
        assert!(needs_detection("page"), "自动取色才做明暗判定");
        for fixed in ["light", "dark", "system", ""] {
            assert!(!needs_detection(fixed), "{fixed} 不该抓屏");
        }
    }

    #[test]
    fn 三帧静态界面就够判() {
        let frames = vec![
            snap(40, 40, [250, 250, 250]),
            snap(40, 40, [250, 250, 250]),
            snap(40, 40, [250, 250, 250]),
        ];
        assert_eq!(decide(&frames), Some(false), "快速路径 3 帧应能判浅色");
    }

    #[test]
    fn 缓存只认同一窗口且未过期() {
        assert!(cache_hit(7, 0, 7, CACHE_TTL_MS), "同一窗口、刚判定过");
        assert!(cache_hit(7, CACHE_TTL_MS - 1, 7, CACHE_TTL_MS), "差 1ms 到期");
        assert!(!cache_hit(7, CACHE_TTL_MS, 7, CACHE_TTL_MS), "到期必须重判");
        assert!(!cache_hit(7, 0, 8, CACHE_TTL_MS), "换了窗口必须重判");
    }
}
