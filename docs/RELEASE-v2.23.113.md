# Quick Achieve v2.23.113

> `main.rs` 第十一刀：整份配置恢复/导入 `apply_view`（542 行）搬进
> `src/settings_apply.rs`。`main.rs` 首次降到 2.39 万行。

## 一、拆了什么

`apply_view(&Value, &mut Config)` 现在在 `settings_apply.rs`：

- 恢复备份 / 导入配置时，把前端视图整段写回 `Config`；
- 快捷方式、外部应用、自定义引擎、收藏夹、OCR/翻译名单、启动密码、
  WebDAV、外观等所有字段的净化与迁移；
- 需要立即生效的副作用（重新注册热键、广播主题、合并本机自定义引擎、
  写开机启动项、备份密码暂存）仍按原顺序调用。

为跨模块访问，以下 main 函数改 `pub(crate)`：

`broadcast` / `cfg_snapshot` / `config_view` / `copy_diagnostics` /
`diagnostics_text` / `go_same_target` / `merge_restored_custom_engines` /
`parse_hotkey` / `refresh_steal_hotkeys` / `stash_backup_password`。

## 二、命令表检查同步

`copy_diagnostics` 本身是命令，改成 `pub(crate)` 后，
`tools/check-cmd-map.js` 的正则只认 `fn` / `pub fn`，把它误判成
“后端没有”。已放宽为 `pub(crate) fn`，命令表恢复一致（227 个后端命令）。

## 三、行数

| 文件 | 之前 | 现在 |
|---|---|---|
| `src/main.rs` | 24,448 | **23,907** |
| `src/settings_apply.rs` | 465 | 996 |

`main.rs` 从最初 26,107 行累计 **−2,200 行（−8.4%）**。

## 四、验证

| 项目 | 结果 |
|---|---|
| `cargo test` | **476 passed** / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | **887 / 887** |
| `tools/test-pages.js` | **56 / 56** |
| `tools/check-all.js` | 五项全过（命令表一致） |
| `node tools/e2e/run.js` | **4/4 场景**（磨砂 11 / 恢复默认 6 / 搜索 9） |

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**