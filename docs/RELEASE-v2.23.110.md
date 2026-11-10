# Quick Achieve v2.23.110

> `main.rs` 第八刀：网格的配置读取与三个 setter 搬进 `grid_cmds.rs`。
> 现在整个网格配置命令组只剩「快捷键冲突检查」还留 main。

## 一、拆了什么

| 命令 | 新实现 |
|---|---|
| `grid_config` | `grid_cmds::config_view(&Config)` |
| `set_grid_size` | `grid_cmds::set_size(...)`，返回最终 `(r,c,w,h,gap,radius)` |
| `set_grid_behavior` | `grid_cmds::set_behavior(...)`，返回归一化后的 click |
| `set_grid_items` | `grid_cmds::set_items(...)`，返回写入的页名 |

模块仍是纯函数：只读/改 `config::Config`，不锁、不落盘、不广播、不写日志。
main 命令壳负责锁、`config::save`、`broadcast`、日志；命令名/参数/返回值不变。

保留在 main 的 `set_grid_hotkey`：它要调 `parse_hotkey`、比对收藏夹热键、
识别 `ctrl+alt+o` 保留键（`VK_O`），这些是钩子层的符号，留在 main 更稳。

## 二、行数

| 文件 | 之前 | 现在 |
|---|---|---|
| `src/main.rs` | 25,280 | **25,203** |
| `src/grid_cmds.rs` | 约 210 | 约 310 |

## 三、验证

| 项目 | 结果 |
|---|---|
| `cargo test` | **476 passed** / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | **887 / 887** |
| `tools/test-pages.js` | **56 / 56** |
| `tools/check-all.js` | 五项全过（命令表一致） |
| `node tools/e2e/run.js` | **4/4 场景**（磨砂 11 / 恢复默认 6 / 搜索 9） |

---

**SHA256: B1E5717DD995DE97CE6B3350B81502E81CE773EDE5C6B6F91EB2107F7668FF9F**