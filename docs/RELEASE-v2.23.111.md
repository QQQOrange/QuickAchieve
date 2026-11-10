# Quick Achieve v2.23.111

> `main.rs` 第九刀：设置页配置视图搬到 `src/settings_view.rs`。
> `config_view_of` 是 269 行的纯投影，终于从 2.5 万行的 main 里出来。

## 一、拆了什么

`src/settings_view.rs`（约 295 行）：

- `config_view_of(&Config) -> Value`：把配置投影成前端 `CONF` 的 camelCase 视图；
- `ocr_creds_view(&Config)` / `trans_creds_view(&Config)`：每家引擎的凭据视图
  （只给设置页渲染，键 = 引擎 id）；
- 依赖 `crate::icon_data_url_cached`（图标 data URL），已改 `pub(crate)`。

留在 main 的：

- `config_view()` / `defaults_view()` 两个命令壳（锁配置 / 用默认配置）；
- `apply_view` / `apply_one_setting` / `set_setting` / `set_settings` 写入链路
  （下一刀）。

## 二、配套改动

`tools/check-settings.js` 原来只读 `src/main.rs` 找 `fn config_view_of(`；
现在改为读 `src/settings_view.rs`，并用 `\n}` 定位函数体（兼容 CRLF）。
设置项一致性恢复 **77 控件 / 18 滑块 / 134 key，问题 0**。

## 三、行数

| 文件 | 之前 | 现在 |
|---|---|---|
| `src/main.rs` | 25,203 | **24,908** |
| `src/settings_view.rs` | — | 295 |

## 四、验证

| 项目 | 结果 |
|---|---|
| `cargo test` | **476 passed** / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | **887 / 887** |
| `tools/test-pages.js` | **56 / 56** |
| `tools/check-all.js` | 五项全过（设置项 0 问题） |
| `node tools/e2e/run.js` | **4/4 场景**（磨砂 11 / 恢复默认 6 / 搜索 9） |

---

**SHA256: FDD090EE42637850592911D3434CC52F0A629A5B9C2A50F0EC4C3C5894F30462**