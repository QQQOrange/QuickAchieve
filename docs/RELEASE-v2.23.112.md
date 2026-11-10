# Quick Achieve v2.23.112

> `main.rs` 第十刀：单键写入核心 `apply_one_setting`（467 行）搬到
> `src/settings_apply.rs`。`set_setting` / `set_settings` 命令壳仍留 main。

## 一、拆了什么

`apply_one_setting(&mut Config, key, value)` 现在在 `settings_apply.rs`：

- 白名单式 match：界面开关、主题、热键、引擎名单、收藏夹、启动密码、
  WebDAV、区域动作等；
- 只改内存里的配置；需要立即应用/发事件的项（如开机启动、快捷键、
  `autoStart`）仍按原顺序执行；
- 不落盘、不广播 —— 这两件事只在 main 的命令壳里发生一次。

依赖处理：

- `app()` / `startup_password_make_hash` / `write_local_elevate_pref` /
  `normalize_region_action` 改 `pub(crate)`；
- 模块内 `win::` / `ocr::` / `ocr_engines::` / `webdav::` / `imgsearch::`
  统一走 `crate::`；
- `apply_one_setting` 里用到的 `Emitter` trait 显式导入。

## 二、行数

| 文件 | 之前 | 现在 |
|---|---|---|
| `src/main.rs` | 24,908 | **24,448** |
| `src/settings_apply.rs` | — | 465 |

`main.rs` 从最初 26,107 行累计 **−1,659 行**。

## 三、验证

| 项目 | 结果 |
|---|---|
| `cargo test` | **476 passed** / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | **887 / 887** |
| `tools/test-pages.js` | **56 / 56** |
| `tools/check-all.js` | 五项全过（设置项 0 问题） |
| `node tools/e2e/run.js` | **4/4 场景**（磨砂 11 / 恢复默认 6 / 搜索 9） |

---

**SHA256: C9C9099BA115F89F0626C21EE44FECBD1F5600207885EEBED8235EE6ABCEED1F**