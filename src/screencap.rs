//! 屏幕抓取的底层原语（Rust 侧）。
//!
//! 这个模块**只为自动取色（`autotheme`）服务**：取一帧桌面，判断壁纸/前景
//! 是深还是浅，好决定横条该用亮色还是暗色皮肤。
//!
//! # 它以前叫 `glass`
//!
//! v2.23.131 把「液态玻璃」和「磨砂」两套外观从产品里摘掉了，原先 600 行的
//! `glass.rs` 里有一大半是给它们用的：快照暂存（`take`）、按矩形重抓
//! （`recapture_last`）、零拷贝 IPC 编码（`capture_encoded`）、抓屏隐身
//! （`WDA_EXCLUDEFROMCAPTURE`）……那些全删了。
//!
//! 留下的只有三样：`Snap`、`capture_rect`、`virtual_screen` —— 自动取色原本
//! 就只是借用它们，跟玻璃没关系。模块因此改名，免得以后有人以为还有玻璃。
//!
//! # 抓取手法
//!
//! BitBlt 从屏幕 DC 抓一块矩形，再 GetDIBits 拿像素，转成 RGBA。
//! 与 OCR 截图（`win::grab_region`）是同一套，管理员进程抓同会话桌面
//! 已在生产环境证明可行。
//!
//! ⚠ 横条自己不会被抓进来：`autotheme` 抓的是**横条将要出现的那个位置**，
//! 抓的时候横条还在屏外。这也意味着这里**不需要**抓屏隐身那套东西了。

use std::sync::Mutex;

use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
    SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

/// 一次抓屏的结果：矩形（物理像素，屏幕坐标）+ top-down RGBA 字节块。
#[derive(Clone, Debug, Default)]
pub struct Snap {
    /// 抓的是哪一块。`autotheme` 只用 `w`/`h`/`rgba`，但留着实测日志、
    /// 出错时对得上"这是哪一块屏幕"，所以不删。
    #[allow(dead_code)]
    pub x: i32,
    #[allow(dead_code)]
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// w * h * 4 字节，RGBA 顺序，自上而下。alpha 恒 255（屏幕没有透明）。
    pub rgba: Vec<u8>,
}

/* ------------------------------------------------------------------ */
/* 抓屏资源的复用                                                      */
/*                                                                     */
/* # 为什么要复用                                                        */
/*                                                                     */
/* 自动取色一次要连抓几帧（3 帧快判），每帧都新建一套 GDI 资源           */
/* （CreateCompatibleDC + CreateCompatibleBitmap + 两块 Delete）还要      */
/* 新分配一块 RGBA 缓冲，这些开销和"抓屏本身"（一次 BitBlt + 一次         */
/* GetDIBits）毫无关系，纯粹是重复劳动。                                 */
/*                                                                     */
/* # 怎么复用                                                            */
/*                                                                     */
/* DC 与位图**只在尺寸变化时重建**；RGBA 缓冲同理 —— 尺寸没变就直接往    */
/* 同一块内存里 GetDIBits，省掉分配与释放。                              */
/* ------------------------------------------------------------------ */

/// 常驻的抓屏资源：内存 DC + 位图 + RGBA 缓冲，尺寸不变就一直用。
struct Scratch {
    mem: HdcOrNull,
    bmp: HBITMAP,
    w: i32,
    h: i32,
    rgba: Vec<u8>,
}

/// `HDC` 是带裸指针的句柄类型，默认不满足 `Send`（于是 `Mutex<Scratch>`
/// 也就不是 `Sync`）。但这一组资源**事实上是线程安全的**：它只被
/// `SCRATCH` 那把 `Mutex` 保护着访问，而 GDI 的 DC / 位图本身可以跨线程
/// 使用（同一时刻只从一个线程用就行 —— 正是互斥锁保证的）。
/// 所以手动声明 `Send`，并在 SAFETY 注释里把"为什么可以"写清楚。
struct HdcOrNull(isize);

// SAFETY：`HdcOrNull` / `HBITMAP` 只是内核对象的句柄数值。跨线程传递句柄
// 本身是允许的（Win32 的 DC 与位图可以在另一个线程里继续用，只要不同时用）。
// 真正的并发保护由 `static SCRATCH: Mutex<...>` 提供。
unsafe impl Send for Scratch {}

static SCRATCH: Mutex<Option<Scratch>> = Mutex::new(None);

impl Drop for Scratch {
    fn drop(&mut self) {
        unsafe {
            if !self.bmp.0.is_null() {
                let _ = DeleteObject(HGDIOBJ(self.bmp.0));
            }
            if self.mem.0 != 0 {
                let _ = DeleteDC(windows::Win32::Graphics::Gdi::HDC(self.mem.0 as *mut _));
            }
        }
    }
}

/// 确保常驻资源是按 `w × h` 准备的，返回可用的引用。
///
/// 尺寸变了就整组重建（旧资源在 `Drop` 里被删）。返回 `None` = 建不出来
/// （极端情况下 GDI 失败），调用方按"这次抓屏失败"处理。
fn with_scratch<R>(w: i32, h: i32, f: impl FnOnce(&mut Scratch) -> R) -> Option<R> {
    let mut guard = match SCRATCH.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let need_rebuild = match guard.as_ref() {
        Some(s) => s.w != w || s.h != h,
        None => true,
    };
    if need_rebuild {
        unsafe {
            let screen = GetDC(None);
            if screen.is_invalid() {
                return None;
            }
            let mem = CreateCompatibleDC(Some(screen));
            if mem.is_invalid() {
                ReleaseDC(None, screen);
                return None;
            }
            let bmp = CreateCompatibleBitmap(screen, w, h);
            ReleaseDC(None, screen);   // 位图已建好，屏幕 DC 可以还了
            if bmp.is_invalid() {
                let _ = DeleteDC(mem);
                return None;
            }
            // 旧的那块先让 Drop 清掉，再换新的
            *guard = None;
            *guard = Some(Scratch {
                mem: HdcOrNull(mem.0 as isize),
                bmp,
                w,
                h,
                rgba: Vec::with_capacity((w as usize) * (h as usize) * 4),
            });
        }
    }
    guard.as_mut().map(f)
}

/// 抓一块屏幕矩形，返回 top-down RGBA。失败（DC 建不了 / BitBlt 被拒 /
/// 矩形出屏）返回 `None` —— 调用方（自动取色）换一帧或放弃即可。
pub fn capture_rect(x: i32, y: i32, w: i32, h: i32) -> Option<Snap> {
    if w <= 0 || h <= 0 || w > 8192 || h > 8192 {
        return None;
    }
    // 矩形必须完整落在虚拟屏幕内：BitBlt 的源矩形出屏会直接失败，
    // 而"抓一半、补一半"会得到一块错位的图，判出的明暗比没有更糟。
    let (vx, vy, vw, vh) = virtual_screen();
    if x < vx || y < vy || x + w > vx + vw || y + h > vy + vh {
        return None;
    }
    let rgba = with_scratch(w, h, |s| unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let mem = windows::Win32::Graphics::Gdi::HDC(s.mem.0 as *mut _);
        let old = SelectObject(mem, HGDIOBJ(s.bmp.0));
        let ok = BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY).is_ok();
        ReleaseDC(None, screen);
        // 必须先从 DC 里摘出来，否则 GetDIBits 不干活（同 OCR 的 grab_region）
        SelectObject(mem, old);
        if !ok {
            return None;
        }

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // 负数 = 自上而下
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0, // BI_RGB
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            ..Default::default()
        };
        let need = (w as usize) * (h as usize) * 4;
        /* ⚠ 复用缓冲时不能靠 `set_len` 那套"未初始化"技巧：`Vec::with_capacity`
         * 的 len 一直是 0，而我们要往同一块内存反复写。这里先 `resize` 把长度
         * 标出来 —— 只在尺寸变化时做一次（`with_scratch` 重建时才新建 Vec），
         * 之后每次都是直接复用。
         *
         * SAFETY 同前：`GetDIBits` 会把 lines 行的像素全部填满，
         * 填不满时下面 `lines != h` 会直接返回 None，不会把未初始化的
         * 字节交出去。 */
        if s.rgba.len() != need {
            s.rgba.resize(need, 0);
        }
        let lines = GetDIBits(
            mem,
            s.bmp,
            0,
            h as u32,
            Some(s.rgba.as_mut_ptr() as *mut _),
            &mut info,
            DIB_RGB_COLORS,
        );
        if lines != h {
            return None;
        }
        // BitBlt 抓来的是 BGRA（alpha 通道是垃圾值）→ 现场换成 RGBA，
        // alpha 拍成 255。一次线性扫描，几万像素，微秒级。
        for px in s.rgba.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
            px[3] = 255;
        }
        /* ⚠ 这里必须**克隆一份**：复用缓冲是常驻内存，交出去会被后面几帧
         * 覆盖掉 —— 表现是"图像内容在变、但变了之后又被冲掉"。 */
        Some(s.rgba.clone())
    });
    /* `with_scratch` 返回的是 `Option<Option<Vec<u8>>>`：
     *   外层 = "常驻资源建没建起来"（GDI 失败时才 None）；
     *   内层 = "这一次抓屏成没成功"（BitBlt 被拒 / GetDIBits 行数不够时 None）。
     * 所以要 `flatten` 一层 —— 直接 `map` 会把 `Option` 当成 rgba 塞进 Snap。 */
    rgba.flatten().map(|rgba| Snap { x, y, w, h, rgba })
}

/// 虚拟屏幕边界（多显示器 / 负坐标桌面）。抓屏矩形先和它求交，
/// 免得 BitBlt 源矩形出屏直接失败。
pub fn virtual_screen() -> (i32, i32, i32, i32) {
    unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        (x, y, w, h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 抓得到屏的环境才断言（无桌面会话里 BitBlt 会失败，那不是产品的错）。
    fn can_capture(x: i32, y: i32, w: i32, h: i32) -> bool {
        capture_rect(x, y, w, h).is_some()
    }

    /// 抓一块小矩形，字节数、alpha、矩形回填都要对。
    #[test]
    fn capture_returns_rgba() {
        let (vx, vy, vw, vh) = virtual_screen();
        if vw < 8 || vh < 8 || !can_capture(vx, vy, 8, 8) {
            eprintln!("环境抓不了屏，跳过");
            return;
        }
        let s = capture_rect(vx, vy, 8, 8).expect("刚刚 can_capture 过，这里不该失败");
        assert_eq!((s.x, s.y, s.w, s.h), (vx, vy, 8, 8), "矩形要原样回填");
        assert_eq!(s.rgba.len(), 8 * 8 * 4, "字节数 = w*h*4");
        assert!(s.rgba.chunks(4).all(|p| p[3] == 255), "alpha 一律拍成 255");
    }

    /// 出屏的矩形直接拒绝（BitBlt 会失败，不如早点说清楚）。
    #[test]
    fn capture_rejects_out_of_screen() {
        let (vx, vy, vw, vh) = virtual_screen();
        assert!(
            capture_rect(vx + vw + 10, vy, 8, 8).is_none(),
            "完全在屏幕右侧之外，必须拒绝"
        );
    }

    /// 尺寸非法也拒绝，别把 0 宽的东西传给 CreateCompatibleBitmap。
    #[test]
    fn capture_rejects_bad_size() {
        let (vx, vy, ..) = virtual_screen();
        assert!(capture_rect(vx, vy, 0, 8).is_none());
        assert!(capture_rect(vx, vy, 8, -1).is_none());
        assert!(capture_rect(vx, vy, 9000, 8).is_none());
    }
}
