# Quick Achieve v2.23.46

> `main.rs` 拆分**开工**：先把耦合最低的两块搬出去，模式跑通（0 警告、451 单测全过）。

## 拆了什么

| 新模块 | 内容 | 行数 |
|---|---|---|
| `src/diag.rs` | 一键诊断文本的生成与复制（原 main.rs 里约 130 行） | 133 |
| `src/favquick.rs` | 收藏夹「一步收」的纯逻辑 + 3 条单测 | 88 |

`main.rs`：26,627 → **26,426** 行。

拆完的形态是"**逻辑出模块、命令留薄壳**"：

```rust
#[crate::t::command]
fn diagnostics_text() -> String {
    crate::diag::text()
}
```

`copy_diagnostics` 同理。收藏命令只负责加锁 / 存盘 / 广播，判断逻辑在 `favquick::quick_add_into`。

## 为什么不是"一口气全拆完"

`main.rs` 里剩下的东西（窗口、钩子、取词、备份）**共享大量私有 static 和辅助函数**。搬一块要同时改：

1. 被搬函数的可见性（`pub(crate)`）；
2. 调用方的路径（`foo()` → `module::foo()`）；
3. `generate_handler!` 里的注册名。

一次性全搬，**编译错误会成百上千条交织在一起**，"哪一行导致不过"无从定位，最后只能整份回滚。所以按"一块一测"推进，今天先跑通模式。

## 剩下的拆分顺序（下一版继续）

1. 剪贴板历史（已在独立模块，只剩命令壳可搬）；
2. 本地答案（同上）；
3. 备份工具（差异 / 恢复 / 快照，最长的一块）；
4. 窗口与钩子（耦合最深，最后做）。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | **451** passed（收藏 3 条测试随模块搬走） |
| `cargo clippy -- -D warnings` | **0 警告** |
| `node tools/check-all.js` | UI 858 / 页面 56 / 命令表一致 / 英文残留 3 —— 全过 |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 托盘图标右键 → 退出；
3. 覆盖 → 双击运行。

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 89FB583BDB46F1A1636F6BB0129351270EDF539DFF15771F6F43BAE1523191FF**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）
