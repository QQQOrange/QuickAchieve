# Quick Achieve v2.23.73

> 修：浏览器（Edge / 抖音网页）里划词不弹条 —— 横条自己抢了前台，Ctrl+C 打在自己身上。

## 日志证据

```
start_capture: 没拿到文字，仍然弹条（兜底）      ← 先显示了横条
capture(exact): 700ms 内没读到新内容（目标 pid=17916 写没写=false）
```

`写没写=false` = 合成 Ctrl+C **没有**写进剪贴板。横条显示后前台变成我们自己，按键全落在横条窗口上，目标程序根本没收到复制。

## 修法

`capture(exact)` 发 Ctrl+C 之前先判断：

- 如果当前前台是本进程（横条），就用 `LAST_FOREIGN_FG` 记着的"用户真正在用的那个外部窗口"把焦点抢回去（`force_foreground` + 40ms 稳定期），再发 Ctrl+C；
- 日志会写：`capture(exact): 前台是我们自己 —— 已把焦点还给上一个外部窗口再复制`。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test -- --test-threads=1` | **459** passed |
| `cargo clippy --all-targets -D warnings` | 0 告警 |

---

**SHA256: 700C5E4711B54FD4B1029B92710DE230C6A25CAF6F9898A994F3E7F63212F104**