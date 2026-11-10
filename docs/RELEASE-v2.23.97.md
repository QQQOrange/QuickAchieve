# Quick Achieve v2.23.97

> 拆 `main.rs` 第五刀：取词环境判断搬到 `src/capture_env.rs` —— 剪贴板
> owner 可不可信、是不是微信、两个 pid 是不是同一软件。

## 一、拆了什么

| 函数 | 作用 |
|---|---|
| `clipboard_owner_trusted` | 剪贴板 owner 是否在可信 pid 列表里（含 fail-open 语义） |
| `clipboard_owned_by` | 拖选取词的目标 pid + 当前前台是否可信 |
| `is_wechat_exe_name` | `Weixin.exe` / `WeChat.exe` 判断 |
| `is_wechat_pid` | 按 pid 查 exe 再判微信 |
| `same_app_pid` | 主进程 / 渲染进程 / 助手进程算同一软件 |

这些函数只依赖 `win` 的窗口/进程查询，不碰配置和状态，是「取词环境」的
纯判断层。`capture_selection` / `capture_selection_fast` / OCR 兜底等编排
仍留在 `main.rs`。

## 二、行数

| 文件 | 之前 | 现在 |
|---|---|---|
| `src/main.rs` | 25,490 | **25,397** |
| `src/capture_env.rs` | — | 99 |

`main.rs` 从 26,107 行起，五刀累计 **-710 行**。

## 三、验证

| 项目 | 结果 |
|---|---|
| `cargo test -- --test-threads=1` | 474 passed / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | 887 / 887 |
| `tools/test-pages.js` | 56 / 56 |
| `tools/check-all.js` | 五项全过 |
| `node tools/e2e/run.js` | **3/3 场景通过** |

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**
