# Quick Achieve v2.23.62

> 修：微信那条「延迟还原」**没还原剪贴板**。

## 根因

判断"剪贴板里还是不是我们复制的那段文字"用的是**逐字节比较**：

```rust
Some(t) => now_text.as_deref() == Some(t)   // t 是 trim 过的
```

微信复制出来的文本**常带行尾 `\r\n`**（我们读到的 `got` 是 trim 过的）。
两边一比不相等 → 判成"用户又复制了别的东西" → **跳过还原**，日志只写一句"跳过还原"。

## 修法

1. 比较前**归一化**（`\r\n` → `\n`、去掉两头空白），新增 `norm_text()` + 单测；
2. `restore_deferred()` **重试 6 次**（每次 80ms）：收起那一刻剪贴板常还被微信握着（它复制完会占用几百毫秒），一次打不开就放弃也会表现成"没还原"。

日志现在会明确写：

- `延迟还原完成（第 N 次尝试）`
- `收起时剪贴板已被别人改过 —— 不还原`
- `一直被占用（6 次重试都打不开）`

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test -- --test-threads=1` | **455** passed |
| `cargo clippy --all-targets -D warnings` | **0 告警** |
| `node tools/check-all.js` | UI 883 / 页面 56 / 命令表 / 英文覆盖 —— 全过 |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 托盘图标右键 → 退出；
3. 覆盖 → 双击运行。

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）