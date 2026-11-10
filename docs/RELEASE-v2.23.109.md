# Quick Achieve v2.23.109

> `main.rs` 拆分第七刀：网格页的 6 条管理命令搬进 `src/grid_cmds.rs`。
> 命令名、参数、返回值和行为都不变。

## 一、拆了什么

| 命令 | 新实现 |
|---|---|
| `grid_page_add` | `grid_cmds::page_add` |
| `grid_page_rename` | `grid_cmds::page_rename` |
| `grid_page_delete` | `grid_cmds::page_delete` |
| `grid_page_set_default` | `grid_cmds::page_set_default` |
| `grid_app_page_set` | `grid_cmds::app_page_set` |
| `grid_app_page_remove` | `grid_cmds::app_page_remove` |

模块只改 `config::Config`：**不碰全局状态、不落盘、不广播、不写日志**。
main 里保留命令壳，负责：

```rust
let mut cfg = lock...;
grid_cmds::page_add(&mut cfg, name)?;
config::save(&cfg);
broadcast();
win::log(...);
```

这样：
- 命令注册表、前端 invoke 名字完全不变（命令表一致性检查通过）；
- 页面操作可以脱离窗口/全局状态单测；
- 落盘 / 广播仍只有 main 一个出口，不会出现两套保存路径。

保留在 main 的网格命令：`grid_config`、`set_grid_size`、`set_grid_behavior`、
`set_grid_items`、`set_grid_hotkey`（后面继续搬）。

## 二、新增测试

`grid_cmds` 自带 2 条单测：

- 新建/改名/设默认/删除后默认页仍合法；
- 应用绑定大小写不敏感、重复设置是改绑定而不是新增。

Rust 测试从 474 变 **476**（新增的 2 条），全部通过。

## 三、行数

| 文件 | 之前 | 现在 |
|---|---|---|
| `src/main.rs` | 25,338 | **25,280** |
| `src/grid_cmds.rs` | — | 约 210 |

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

**SHA256: FBA932DD0891AAFA6C8FCAD876ECF56CF45B2513BB4060863795783416DC6421**