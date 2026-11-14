# Quick Achieve v2.23.124

> 修复：网格页点「文字识别」没有反应。

## 根因

WebView2 建窗不是可重入的。`wry` 在创建环境时会泵消息，OCR 触发的
toast 恰好在这个泵里重入 `build()`；两个窗口在同一 user-data-dir 上
并发 `CreateCoreWebView2EnvironmentWithOptions`，实测返回：

```
HRESULT(0x80010108) RPC_E_DISCONNECTED
「被调用的对象已与其客户端断开连接。」
```

框选窗因此建不出来；前端等 2.6 秒后把网格收起，用户看到的就是
「点了没反应」。

## 修复

- **建窗串行化**：`t.rs` 增加进程级 `BUILDING` 标记，嵌套 build 立即
  返回 `QS_WINDOW_BUILD_BUSY`，不碰外层环境。
- **框选窗自动重试**：失败后 250 ms / 700 ms 各重试一次；正常路径不受影响。
- **失败可见**：连续 3 次失败弹原生 MessageBox，提示重启，不再静默。
- `RegionBuildGate` 移到模块级，保持源码契约测试可正确识别建窗点。

## 验证

- `cargo test` 495 passed / 2 ignored；clippy `-D warnings` 干净。
- `tools/check-all.js` 全过；E2E 4/4。
- 隔离实例「网格 → OCR」：`region: built`，页面正常加载。
