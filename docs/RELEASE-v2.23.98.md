# Quick Achieve v2.23.98

> `capture_env.rs` 补上第二个小簇：前台是否仍是取词目标、当前前台是不是本程序、
> 是不是终端窗口。

## 一、拆了什么

| 函数 | 作用 |
|---|---|
| `foreground_still_target` | 发送 Ctrl+C 前确认前台还是目标（允许本程序短暂抢焦点） |
| `foreground_is_self_any` | 当前前台是不是本程序的任一窗口（用于决定要不要走黑白名单） |
| `is_console_window` | 终端窗口判断（绝不向终端发 Ctrl+C） |

这些和上一批的剪贴板 owner / 微信 / 同进程判断同属「取词环境」，
`capture_env.rs` 现在 162 行；`capture_selection` 等编排仍留 main。

## 二、行数

| 文件 | 之前 | 现在 |
|---|---|---|
| `src/main.rs` | 25,397 | **25,338** |
| `src/capture_env.rs` | 99 | 162 |

`main.rs` 从 26,107 行起，六刀累计 **-769 行**。

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

**SHA256: 4C803CF5ECBC20F9485CA86BC58564FEBF13D6B6E2B74AFCD9A7F89C73BE8AC0**
