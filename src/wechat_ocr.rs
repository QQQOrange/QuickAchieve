//! 微信本地 OCR（v2.23.125）。
//!
//! 微信 PC 端自带一套离线 OCR 模型：`WeChatOCR.exe` + `mmmojo_64.dll`。
//! 它不提供命令行接口，官方用法是宿主进程通过 `mmmojo` 起一个子进程，
//! 再用 protobuf 在管道上收发消息。这里按公开的 Python 封装
//! （kanadeblisst00/wechat_ocr）把同一套调用翻译成 Rust：
//!
//! ```text
//! InitializeMMMojo
//!   → CreateMMMojoEnvironment
//!   → 设置回调 / kMMExePath / user-lib-dir
//!   → StartMMMojoEnvironment
//!   → SendMMMojoWriteInfo(OcrRequest{task_id, pic_path})
//!   → kMMReadPush 回调里拿 OcrResponse
//! ```
//!
//! 设计取舍：
//! * **不把模型和 exe 打进安装包** —— 它们随微信安装，版本也跟着微信更新；
//! * 只做「一张图、同步等结果」这一种用法，够 OCR 结果窗用；
//! * 所有失败都返回中文原因，交给结果窗显示，不 panic。

use std::ffi::{c_char, c_void};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use libloading::Library;

type Handle = *mut c_void;
type ReadCb = unsafe extern "C" fn(u32, *mut c_void, *mut c_void);
type ConnectCb = unsafe extern "C" fn(u8, *mut c_void);
type VoidCb = unsafe extern "C" fn(*mut c_void);
type FailCb = unsafe extern "C" fn(i32, *mut c_void);
type ErrorCb = unsafe extern "C" fn(*mut c_void, i32, *mut c_void);

const CB_READ_PUSH: i32 = 1;
const CB_READ_PULL: i32 = 2;
const CB_READ_SHARED: i32 = 3;
const CB_REMOTE_CONNECT: i32 = 4;
const CB_REMOTE_DISCONNECT: i32 = 5;
const CB_REMOTE_LAUNCHED: i32 = 6;
const CB_REMOTE_LAUNCH_FAILED: i32 = 7;
const CB_REMOTE_MOJO_ERROR: i32 = 8;

const PARAM_HOST_PROCESS: i32 = 0;
const PARAM_EXE_PATH: i32 = 2;

const METHOD_PUSH: i32 = 1;
const REQUEST_ID_OCR_PUSH: u32 = 1;

/// 微信 OCR 的安装位置（两个文件就够跑）。
#[derive(Clone, Debug)]
pub struct Install {
    pub exe: PathBuf,
    pub lib_dir: PathBuf,
}

struct Api {
    _lib: Library,
    initialize: unsafe extern "C" fn(i32, *const *const c_char),
    create_env: unsafe extern "C" fn() -> Handle,
    set_cb: unsafe extern "C" fn(Handle, i32, Handle),
    set_param: unsafe extern "C" fn(Handle, i32, usize),
    append_switch: unsafe extern "C" fn(Handle, *const c_char, *const u16),
    start: unsafe extern "C" fn(Handle),
    stop: unsafe extern "C" fn(Handle),
    remove_env: unsafe extern "C" fn(Handle),
    get_read: unsafe extern "C" fn(Handle, *mut u32) -> *const c_void,
    remove_read: unsafe extern "C" fn(Handle),
    create_write: unsafe extern "C" fn(i32, u8, u32) -> Handle,
    get_write_request: unsafe extern "C" fn(Handle, u32) -> *mut c_void,
    send_write: unsafe extern "C" fn(Handle, Handle) -> u8,
}

struct Ctx {
    api: Arc<Api>,
    tx: Sender<Vec<u8>>,
    connected: AtomicBool,
}

unsafe impl Send for Ctx {}
unsafe impl Sync for Ctx {}

struct Engine {
    api: Arc<Api>,
    env: Handle,
    ctx: *mut Ctx,
    rx: Receiver<Vec<u8>>,
    next_task: u32,
    _install: Install,
    /// mmmojo 设置参数时传的是宽字符指针；不确认它是否复制，这里保活。
    _wide: Vec<Vec<u16>>,
}

unsafe impl Send for Engine {}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            (self.api.stop)(self.env);
            (self.api.remove_env)(self.env);
            drop(Box::from_raw(self.ctx));
        }
    }
}

static ENGINE: OnceLock<Mutex<Option<Engine>>> = OnceLock::new();

fn engine_slot() -> &'static Mutex<Option<Engine>> {
    ENGINE.get_or_init(|| Mutex::new(None))
}

/* ------------------------------------------------------------------ */
/* 路径探测                                                             */
/* ------------------------------------------------------------------ */

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn appdata_tencent() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("Tencent"))
}

fn roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for (key, name) in [
        ("Software\\Tencent\\WeChat", "InstallPath"),
        ("Software\\Tencent\\Weixin", "InstallPath"),
        ("Software\\Tencent\\WeChat", "InstallDir"),
    ] {
        if let Some(p) = crate::win::hkcu_get_sz(key, name) {
            if !p.trim().is_empty() {
                out.push(PathBuf::from(p));
            }
        }
    }
    for env in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        if let Some(p) = std::env::var_os(env) {
            out.push(PathBuf::from(&p).join("Tencent").join("WeChat"));
            out.push(PathBuf::from(&p).join("Tencent").join("Weixin"));
        }
    }
    if let Some(t) = appdata_tencent() {
        out.push(t.join("WeChat"));
        out.push(t.join("Weixin"));
    }
    out
}

/// 找版本号最大的 `WeChatOCR.exe`（XPlugin 下每个版本一个目录）。
fn find_ocr_exe() -> Option<PathBuf> {
    let mut found: Vec<(Vec<u32>, PathBuf)> = Vec::new();
    let bases = [
        appdata_tencent().map(|t| t.join("WeChat").join("XPlugin").join("Plugins").join("WeChatOCR")),
        appdata_tencent().map(|t| t.join("Weixin").join("XPlugin").join("Plugins").join("WeChatOCR")),
    ];
    for base in bases.into_iter().flatten() {
        let Ok(rd) = std::fs::read_dir(&base) else { continue };
        for ent in rd.flatten() {
            let p = ent.path();
            let exe = p.join("extracted").join("WeChatOCR.exe");
            if !exe.is_file() {
                continue;
            }
            let ver: Vec<u32> = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .split(['.', '_', '-'])
                .filter_map(|x| x.parse().ok())
                .collect();
            found.push((ver, exe));
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().next().map(|(_, p)| p)
}

fn find_mmmojo(ocr_exe: &Path) -> Option<PathBuf> {
    // 1) exe 所在目录及向上 5 层（XPlugin\...\extracted → WeChat 根）
    let mut cur = ocr_exe.parent();
    for _ in 0..6 {
        let Some(d) = cur else { break };
        for name in ["mmmojo_64.dll", "mmmojo.dll"] {
            let p = d.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
        cur = d.parent();
    }
    // 2) 注册表 / 常见安装目录，最多再向下找两层
    for root in roots() {
        for name in ["mmmojo_64.dll", "mmmojo.dll"] {
            let p = root.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
        for sub in ["bin", "WeChat", "Weixin"] {
            for name in ["mmmojo_64.dll", "mmmojo.dll"] {
                let p = root.join(sub).join(name);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    // 3) 微信 4.x 把一部分运行库放在 XPlugin 下；在 %APPDATA% 的
    //    Tencent\WeChat / Weixin 里做一次有深度上限的搜索（最多 4 层）。
    for base in [
        appdata_tencent().map(|t| t.join("WeChat")),
        appdata_tencent().map(|t| t.join("Weixin")),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(p) = find_named(&base, "mmmojo_64.dll", 4) {
            return Some(p);
        }
        if let Some(p) = find_named(&base, "mmmojo.dll", 4) {
            return Some(p);
        }
    }
    None
}

/// 在 `base` 下按名字找文件，最多 `depth` 层（0 = 只看当前目录）。
fn find_named(base: &Path, name: &str, depth: u8) -> Option<PathBuf> {
    if depth == 0 {
        let p = base.join(name);
        return p.is_file().then_some(p);
    }
    let Ok(rd) = std::fs::read_dir(base) else { return None };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_file() {
            if p.file_name().and_then(|s| s.to_str()) == Some(name) {
                return Some(p);
            }
        } else if p.is_dir() {
            if let Some(hit) = find_named(&p, name, depth - 1) {
                return Some(hit);
            }
        }
    }
    None
}

/// 探测本机微信 OCR。找不到时返回可读的原因。
pub fn detect() -> Result<Install, String> {
    /* 调试 / 绿色版：允许显式指定两个路径（也方便自动化测试用
     * 参考项目的 bin 目录跑通 FFI，不必真装微信）。 */
    if let (Ok(exe), Ok(lib)) = (
        std::env::var("QS_WECHAT_OCR_EXE"),
        std::env::var("QS_WECHAT_LIB_DIR"),
    ) {
        let exe = PathBuf::from(exe.trim());
        let lib = PathBuf::from(lib.trim());
        if exe.is_file() && lib.is_dir() {
            return Ok(Install { exe, lib_dir: lib });
        }
    }
    let exe = find_ocr_exe().ok_or_else(|| {
        "没找到微信 OCR：需要安装微信 PC 版（3.9 或 4.x），并至少启动过一次微信".to_string()
    })?;
    let lib = find_mmmojo(&exe).ok_or_else(|| {
        format!(
            "找到了 {}，但旁边没有 mmmojo_64.dll —— 请确认微信安装完整",
            exe.display()
        )
    })?;
    let mut lib_dir = lib
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| exe.parent().unwrap_or(Path::new(".")).to_path_buf());
    /* Python 封装传的是微信安装根目录（mmmojo_64.dll 通常在根或 bin/ 下）。
     * 如果 dll 在 bin/ 里，user-lib-dir 要给上一级，否则 WeChatOCR 找不到
     * 同级的其它依赖。 */
    if lib_dir
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("bin"))
        .unwrap_or(false)
    {
        if let Some(parent) = lib_dir.parent() {
            lib_dir = parent.to_path_buf();
        }
    }
    Ok(Install { exe, lib_dir })
}

/* ------------------------------------------------------------------ */
/* 回调                                                                 */
/* ------------------------------------------------------------------ */

unsafe extern "C" fn on_read(_rid: u32, info: *mut c_void, user: *mut c_void) {
    if info.is_null() || user.is_null() {
        return;
    }
    let ctx = &*(user as *const Ctx);
    let mut size = 0u32;
    let p = (ctx.api.get_read)(info, &mut size);
    if !p.is_null() && size > 0 {
        let bytes = std::slice::from_raw_parts(p as *const u8, size as usize).to_vec();
        let _ = ctx.tx.send(bytes);
    }
    (ctx.api.remove_read)(info);
}

unsafe extern "C" fn on_connect(ok: u8, user: *mut c_void) {
    if user.is_null() {
        return;
    }
    let ctx = &*(user as *const Ctx);
    ctx.connected.store(ok != 0, Ordering::SeqCst);
}

unsafe extern "C" fn on_disconnect(user: *mut c_void) {
    if user.is_null() {
        return;
    }
    (*(user as *const Ctx)).connected.store(false, Ordering::SeqCst);
}

unsafe extern "C" fn on_noop(_user: *mut c_void) {}

unsafe extern "C" fn on_launch_failed(code: i32, user: *mut c_void) {
    if !user.is_null() {
        (*(user as *const Ctx)).connected.store(false, Ordering::SeqCst);
    }
    crate::win::log(&format!("wechat-ocr: WeChatOCR.exe 启动失败（{code}）"));
}

unsafe extern "C" fn on_mojo_error(buf: *mut c_void, size: i32, _user: *mut c_void) {
    let msg = if !buf.is_null() && size > 0 {
        let b = std::slice::from_raw_parts(buf as *const u8, size as usize);
        String::from_utf8_lossy(b).to_string()
    } else {
        String::new()
    };
    crate::win::log(&format!("wechat-ocr: mmmojo 错误：{msg}"));
}

/* ------------------------------------------------------------------ */
/* 启停                                                                 */
/* ------------------------------------------------------------------ */

impl Engine {
    fn start(install: Install) -> Result<Self, String> {
        let lib_path = install
            .lib_dir
            .join("mmmojo_64.dll");
        let lib_path = if lib_path.is_file() {
            lib_path
        } else {
            install.lib_dir.join("mmmojo.dll")
        };
        let lib = unsafe { Library::new(&lib_path) }
            .map_err(|e| format!("加载 {} 失败：{e}", lib_path.display()))?;
        // 逐个取符号；每个类型单独 transmute，避免把不同 fn 指针混在一个泛型里。
        let initialize = unsafe {
            *lib.get::<unsafe extern "C" fn(i32, *const *const c_char)>(
                b"InitializeMMMojo\0",
            )
            .map_err(|e| format!("mmmojo 缺少 InitializeMMMojo：{e}"))?
        };
        let create_env = unsafe {
            *lib.get::<unsafe extern "C" fn() -> Handle>(b"CreateMMMojoEnvironment\0")
                .map_err(|e| format!("mmmojo 缺少 CreateMMMojoEnvironment：{e}"))?
        };
        let set_cb = unsafe {
            *lib.get::<unsafe extern "C" fn(Handle, i32, Handle)>(
                b"SetMMMojoEnvironmentCallbacks\0",
            )
            .map_err(|e| format!("mmmojo 缺少 SetMMMojoEnvironmentCallbacks：{e}"))?
        };
        let set_param = unsafe {
            *lib.get::<unsafe extern "C" fn(Handle, i32, usize)>(
                b"SetMMMojoEnvironmentInitParams\0",
            )
            .map_err(|e| format!("mmmojo 缺少 SetMMMojoEnvironmentInitParams：{e}"))?
        };
        let append_switch = unsafe {
            *lib.get::<unsafe extern "C" fn(Handle, *const c_char, *const u16)>(
                b"AppendMMSubProcessSwitchNative\0",
            )
            .map_err(|e| format!("mmmojo 缺少 AppendMMSubProcessSwitchNative：{e}"))?
        };
        let start = unsafe {
            *lib.get::<unsafe extern "C" fn(Handle)>(b"StartMMMojoEnvironment\0")
                .map_err(|e| format!("mmmojo 缺少 StartMMMojoEnvironment：{e}"))?
        };
        let stop = unsafe {
            *lib.get::<unsafe extern "C" fn(Handle)>(b"StopMMMojoEnvironment\0")
                .map_err(|e| format!("mmmojo 缺少 StopMMMojoEnvironment：{e}"))?
        };
        let remove_env = unsafe {
            *lib.get::<unsafe extern "C" fn(Handle)>(b"RemoveMMMojoEnvironment\0")
                .map_err(|e| format!("mmmojo 缺少 RemoveMMMojoEnvironment：{e}"))?
        };
        let get_read = unsafe {
            *lib.get::<unsafe extern "C" fn(Handle, *mut u32) -> *const c_void>(
                b"GetMMMojoReadInfoRequest\0",
            )
            .map_err(|e| format!("mmmojo 缺少 GetMMMojoReadInfoRequest：{e}"))?
        };
        let remove_read = unsafe {
            *lib.get::<unsafe extern "C" fn(Handle)>(b"RemoveMMMojoReadInfo\0")
                .map_err(|e| format!("mmmojo 缺少 RemoveMMMojoReadInfo：{e}"))?
        };
        let create_write = unsafe {
            *lib.get::<unsafe extern "C" fn(i32, u8, u32) -> Handle>(
                b"CreateMMMojoWriteInfo\0",
            )
            .map_err(|e| format!("mmmojo 缺少 CreateMMMojoWriteInfo：{e}"))?
        };
        let get_write_request = unsafe {
            *lib.get::<unsafe extern "C" fn(Handle, u32) -> *mut c_void>(
                b"GetMMMojoWriteInfoRequest\0",
            )
            .map_err(|e| format!("mmmojo 缺少 GetMMMojoWriteInfoRequest：{e}"))?
        };
        let send_write = unsafe {
            *lib.get::<unsafe extern "C" fn(Handle, Handle) -> u8>(b"SendMMMojoWriteInfo\0")
                .map_err(|e| format!("mmmojo 缺少 SendMMMojoWriteInfo：{e}"))?
        };

        let api = Arc::new(Api {
            _lib: lib,
            initialize,
            create_env,
            set_cb,
            set_param,
            append_switch,
            start,
            stop,
            remove_env,
            get_read,
            remove_read,
            create_write,
            get_write_request,
            send_write,
        });
        let (tx, rx) = channel();
        let ctx = Box::into_raw(Box::new(Ctx {
            api: api.clone(),
            tx: tx.clone(),
            connected: AtomicBool::new(false),
        }));

        unsafe {
            (api.initialize)(0, std::ptr::null());
            let env = (api.create_env)();
            if env.is_null() {
                drop(Box::from_raw(ctx));
                return Err("CreateMMMojoEnvironment 失败（微信 OCR 环境起不来）".into());
            }
            (api.set_cb)(env, 0, ctx as Handle);
            (api.set_cb)(env, CB_READ_PUSH, on_read as ReadCb as Handle);
            (api.set_cb)(env, CB_READ_PULL, on_read as ReadCb as Handle);
            (api.set_cb)(env, CB_READ_SHARED, on_read as ReadCb as Handle);
            (api.set_cb)(env, CB_REMOTE_CONNECT, on_connect as ConnectCb as Handle);
            (api.set_cb)(env, CB_REMOTE_DISCONNECT, on_disconnect as VoidCb as Handle);
            (api.set_cb)(env, CB_REMOTE_LAUNCHED, on_noop as VoidCb as Handle);
            (api.set_cb)(env, CB_REMOTE_LAUNCH_FAILED, on_launch_failed as FailCb as Handle);
            (api.set_cb)(env, CB_REMOTE_MOJO_ERROR, on_mojo_error as ErrorCb as Handle);

            (api.set_param)(env, PARAM_HOST_PROCESS, 1);
            let exe_w = wide(install.exe.to_string_lossy().as_ref());
            (api.set_param)(env, PARAM_EXE_PATH, exe_w.as_ptr() as usize);
            let key = b"user-lib-dir\0";
            let val = wide(install.lib_dir.to_string_lossy().as_ref());
            (api.append_switch)(env, key.as_ptr() as *const c_char, val.as_ptr());
            (api.start)(env);

            let deadline = Instant::now() + Duration::from_secs(12);
            while !(*ctx).connected.load(Ordering::SeqCst) {
                if Instant::now() >= deadline {
                    (api.stop)(env);
                    (api.remove_env)(env);
                    drop(Box::from_raw(ctx));
                    return Err("微信 OCR 子进程 12 秒内没有连上（可能被安全软件拦住）".into());
                }
                std::thread::sleep(Duration::from_millis(50));
            }

            Ok(Engine {
                api,
                env,
                ctx,
                rx,
                next_task: 1,
                _install: install,
                _wide: vec![exe_w, val],
            })
        }
    }

    fn recognize(&mut self, png: &[u8], timeout: Duration) -> Result<String, String> {
        let dir = std::env::temp_dir();
        let ext = if png.starts_with(&[0xFF, 0xD8, 0xFF]) { "jpg" } else { "png" };
        let path = dir.join(format!(
            "QuickAchieve-wechat-{}-{}.{ext}",
            std::process::id(),
            self.next_task
        ));
        std::fs::write(&path, png).map_err(|e| format!("写临时图片失败：{e}"))?;
        let task = self.next_task;
        self.next_task = if self.next_task >= 32 { 1 } else { self.next_task + 1 };

        let req = encode_request(task as i32, &path.to_string_lossy());
        let deadline = Instant::now() + timeout;
        let result = unsafe {
            let wi = (self.api.create_write)(METHOD_PUSH, 0, REQUEST_ID_OCR_PUSH);
            if wi.is_null() {
                Err("CreateMMMojoWriteInfo 失败".to_string())
            } else {
                let buf = (self.api.get_write_request)(wi, req.len() as u32);
                if buf.is_null() {
                    Err("GetMMMojoWriteInfoRequest 返回空".to_string())
                } else {
                    std::ptr::copy_nonoverlapping(req.as_ptr(), buf as *mut u8, req.len());
                    let ok = (self.api.send_write)(self.env, wi) != 0;
                    if !ok {
                        Err("SendMMMojoWriteInfo 失败（微信 OCR 管道断了）".to_string())
                    } else {
                        wait_response(self, task as i32, deadline)
                    }
                }
            }
        };
        let _ = std::fs::remove_file(&path);
        result
    }
}

fn wait_response(engine: &Engine, task: i32, deadline: Instant) -> Result<String, String> {
    // 回调把响应塞进 channel；Engine 持有唯一的接收端。识别是串行化的
    // （外层 Mutex），队列里不会有别的任务的响应。
    let rx = &engine.rx;
    let mut saw_empty = false;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            if saw_empty {
                return Ok(String::new()); // 图里确实没有字
            }
            return Err("微信 OCR 超时（20 秒没有返回）".into());
        }
        match rx.recv_timeout(left.min(Duration::from_millis(200))) {
            Ok(bytes) => {
                if bytes.len() <= 10 {
                    /* 微信先回一个 6 字节的「收到」ack（type=1/task/err=0），
                     * 真正的结果在后面；Python 封装同样用 >10 过滤。 */
                    continue;
                }
                if let Some((id, err, text)) = decode_response(&bytes) {
                    if id != task {
                        continue; // 上一轮迟到的响应
                    }
                    if err != 0 {
                        return Err(format!("微信 OCR 返回错误码 {err}"));
                    }
                    if !text.trim().is_empty() {
                        return Ok(text);
                    }
                    saw_empty = true;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err("微信 OCR 管道已断开".into())
            }
        }
    }
}

/* ------------------------------------------------------------------ */
/* protobuf（只覆盖用到的字段）                                          */
/* ------------------------------------------------------------------ */

fn put_varint(out: &mut Vec<u8>, mut n: u64) {
    loop {
        let b = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
}

fn encode_request(task: i32, path: &str) -> Vec<u8> {
    let mut pic = Vec::new();
    pic.push(0x0a); // PicPaths.pic_path = 1
    put_varint(&mut pic, path.len() as u64);
    pic.extend_from_slice(path.as_bytes());

    let mut out = Vec::new();
    out.push(0x08); // unknow = 1
    put_varint(&mut out, 0);
    out.push(0x10); // task_id = 2
    put_varint(&mut out, task as u64);
    out.push(0x1a); // pic_path = 3
    put_varint(&mut out, pic.len() as u64);
    out.extend_from_slice(&pic);
    out
}

fn get_varint(b: &[u8], i: &mut usize) -> Option<u64> {
    let mut n = 0u64;
    let mut shift = 0;
    loop {
        let byte = *b.get(*i)?;
        *i += 1;
        n |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some(n);
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
}

/// 解析 `OcrResponse`：返回 `(task_id, err_code, 文本)`。
fn decode_response(b: &[u8]) -> Option<(i32, i32, String)> {
    let mut i = 0usize;
    let mut task = 0i32;
    let mut err = 0i32;
    let mut lines: Vec<String> = Vec::new();
    while i < b.len() {
        let key = get_varint(b, &mut i)?;
        let field = key >> 3;
        let wire = key & 7;
        match (field, wire) {
            (2, 0) => task = get_varint(b, &mut i)? as i32,
            (3, 0) => err = get_varint(b, &mut i)? as i32,
            (4, 2) => {
                let n = get_varint(b, &mut i)? as usize;
                let sub = b.get(i..i + n)?;
                collect_result(sub, &mut lines);
                i += n;
            }
            (1, 0) | (1, 2) => skip_field(b, &mut i, wire)?,
            (_, 0) => {
                let _ = get_varint(b, &mut i)?;
            }
            (_, 1) => i += 8,
            (_, 2) => {
                let n = get_varint(b, &mut i)? as usize;
                i += n;
            }
            (_, 5) => i += 4,
            _ => return None,
        }
        if i > b.len() {
            return None;
        }
    }
    Some((task, err, lines.join("\n")))
}

fn skip_field(b: &[u8], i: &mut usize, wire: u64) -> Option<()> {
    match wire {
        0 => {
            let _ = get_varint(b, i)?;
        }
        1 => *i += 8,
        2 => {
            let n = get_varint(b, i)? as usize;
            *i += n;
        }
        5 => *i += 4,
        _ => return None,
    }
    Some(())
}

/// `OcrResult.single_result[]`（field 1），每项取 `single_str_utf8`（field 2）。
fn collect_result(b: &[u8], lines: &mut Vec<String>) {
    let mut i = 0usize;
    while i < b.len() {
        let Some(key) = get_varint(b, &mut i) else { return };
        let field = key >> 3;
        let wire = key & 7;
        if field == 1 && wire == 2 {
            let Some(n) = get_varint(b, &mut i) else { return };
            let Some(sub) = b.get(i..i + n as usize) else { return };
            if let Some(t) = single_text(sub) {
                let t = t.trim().to_string();
                if !t.is_empty() {
                    lines.push(t);
                }
            }
            i += n as usize;
        } else if skip_field(b, &mut i, wire).is_none() {
            return;
        }
    }
}

fn single_text(b: &[u8]) -> Option<String> {
    let mut i = 0usize;
    while i < b.len() {
        let key = get_varint(b, &mut i)?;
        let field = key >> 3;
        let wire = key & 7;
        if field == 2 && wire == 2 {
            let n = get_varint(b, &mut i)? as usize;
            let raw = b.get(i..i + n)?;
            return Some(String::from_utf8_lossy(raw).to_string());
        }
        skip_field(b, &mut i, wire)?;
    }
    None
}

/* ------------------------------------------------------------------ */
/* 对外入口                                                             */
/* ------------------------------------------------------------------ */

/// 用微信本地 OCR 识别一张 PNG/JPEG。首次调用会启动微信的 OCR 子进程。
pub fn recognize(png: &[u8]) -> Result<String, String> {
    let mut slot = engine_slot()
        .lock()
        .map_err(|_| "微信 OCR 引擎锁坏了".to_string())?;
    if slot.is_none() {
        let install = detect()?;
        crate::win::log(&format!(
            "wechat-ocr: 使用 {}（lib={}）",
            install.exe.display(),
            install.lib_dir.display()
        ));
        *slot = Some(Engine::start(install)?);
    }
    let eng = slot.as_mut().ok_or_else(|| "微信 OCR 未启动".to_string())?;
    eng.recognize(png, Duration::from_secs(25))
}

/// 诊断用：返回「能不能用 + 路径 + 失败原因」。
pub fn probe() -> serde_json::Value {
    match detect() {
        Ok(i) => serde_json::json!({
            "ok": true,
            "exe": i.exe.to_string_lossy(),
            "lib_dir": i.lib_dir.to_string_lossy(),
        }),
        Err(e) => serde_json::json!({ "ok": false, "error": e }),
    }
}

/* ------------------------------------------------------------------ */
/* 测试                                                                 */
/* ------------------------------------------------------------------ */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_request_fields() {
        let b = encode_request(7, r"C:\a b\图.png");
        assert_eq!(b[0], 0x08);
        assert_eq!(b[1], 0x00);
        assert_eq!(b[2], 0x10);
        assert_eq!(b[3], 7);
        let mut i = 4;
        assert_eq!(b[i], 0x1a);
        i += 1;
        let n = get_varint(&b, &mut i).unwrap() as usize;
        assert_eq!(b[i], 0x0a);
        assert!(b.len() >= i + n);
    }

    #[test]
    fn decodes_text_response() {
        // OcrResponse{task_id=3, ocr_result{single_result{
        //   single_str_utf8="第一行" }}}
        let mut single = vec![0x12, 9];
        single.extend_from_slice("第一行".as_bytes());
        let mut result = vec![0x0a, single.len() as u8];
        result.extend_from_slice(&single);
        let mut resp = vec![0x10, 3, 0x22, result.len() as u8];
        resp.extend_from_slice(&result);
        let (task, err, text) = decode_response(&resp).unwrap();
        assert_eq!(task, 3);
        assert_eq!(err, 0);
        assert_eq!(text, "第一行");
    }

    #[test]
    fn ignores_unknown_fields() {
        let b = [0x08, 1, 0x10, 9, 0x1a, 0];
        let (task, _err, text) = decode_response(&b).unwrap();
        assert_eq!(task, 9);
        assert!(text.is_empty());
    }
}
