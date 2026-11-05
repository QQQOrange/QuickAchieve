# Quick Achieve v2.23.48

> **备份工具那块拆出去了**（第三块），用的是上一版失败后查清的正确做法。

## 这次怎么成的

上一版直接把 `#[crate::t::command] fn backup_diff` 整个搬进新模块，编译报：

```
error[E0425]: cannot find function `__qs_reg_backup_diff` in this scope
```

原因：命令宏会在**定义所在模块**生成 `__qs_reg_*` 符号，而 `generate_handler!`
在 main.rs 里按名字引 —— 搬到别的模块后两边对不上。

这次改成和 `diag` / `favquick` 一样的模式：**逻辑出模块、命令留薄壳**。

```rust
// main.rs
#[crate::t::command]
fn backup_diff(name: String) -> Result<Value, String> {
    crate::backupdiff::diff(name)   // 实体在 src/backupdiff.rs
}
```

注册表只认 main.rs 里的名字，所以不会再碰到 `__qs_reg_*` 的问题。

## 现在的模块划分

| 模块 | 内容 | 行数 |
|---|---|---|
| `src/diag.rs` | 一键诊断文本生成 + 复制 | 133 |
| `src/favquick.rs` | 收藏夹一步收逻辑（含 4 条单测） | 96 |
| `src/backupdiff.rs` | 备份差异预览逻辑 | 64 |
| `src/cliphist.rs` | 剪贴板历史（本来就在模块里） | 268 |
| `src/localanswer.rs` | 本地答案（本来就在模块里） | 800+ |

`main.rs` 里剩的是**窗口 / 钩子 / 取词** —— 它们共享 `STATE`、`APP` 和十几个全局
static，不是"搬一段代码"能解决的，得先把全局状态改成显式传参。那是下一轮的**设计活**，
不是机械搬运。

## 顺手记一笔：为什么 main.rs 的 diff 很大但实质只改了 57 行

这次重写文件时把行尾从 CRLF 变成了 LF，`git diff` 一度显示 26k 行全改。
已经把行尾恢复成 CRLF，现在 `git diff --stat` 是 **5 insertions / 58 deletions** ——
只动了该动的。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | **453** passed |
| `cargo clippy -- -D warnings` | **0 警告** |
| `node tools/check-all.js` | UI 858 / 页面 56 / 命令表一致 / 英文残留 3 —— 全过 |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 托盘图标右键 → 退出；
3. 覆盖 → 双击运行。

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 618CE06AB56191B71FFF0C42FBFB92D57507CF0F38F184BCD12332C4B82D45DD**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）
