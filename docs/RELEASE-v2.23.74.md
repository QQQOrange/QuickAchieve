# Quick Achieve v2.23.74

> 浏览器（Edge / 抖音网页）的复制通道加一条兜底：Ctrl+C 不写剪贴板时改发 **Ctrl+Insert**。

## 依据

上一版已经能把焦点还给 Edge（日志里出现了 `前台是我们自己 —— 已把焦点还给上一个外部窗口再复制`），
但仍然是 `写没写=false` —— 说明 Edge 那个页面**不理会注入的 Ctrl+C**。

## 修法

`capture(exact)` 的等待循环里加两次补试：

- 300ms 还没写剪贴板 → 发 **Ctrl+Insert**（老式复制快捷键，语义完全相同）；
- 600ms 还没写 → 再补一次 Ctrl+C。

日志会写：

- `capture(exact): Ctrl+C 没写剪贴板 —— 改发 Ctrl+Insert 再试`
- `capture(exact): Ctrl+Insert 也没写 —— 再补一次 Ctrl+C`

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test -- --test-threads=1` | 459 passed |
| `cargo clippy --all-targets -D warnings` | 0 告警 |

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**