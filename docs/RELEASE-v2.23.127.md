# Quick Achieve v2.23.127

> 修识别结果页选中文字后点淘宝有时不搜索。

## 原因

识别结果窗是本程序自己的 WebView。用户在里面选中文字时，焦点仍在本进程；
取词线程走“合成 Ctrl+C”时会被“前台是我们自己，不能发按键”的保护拦下，
于是 `open_search` 收到 `text=None`，后端兜底也读不到选区 —— 淘宝只打开了首页。
日志里对应的就是：

```
open_search: engine=淘宝 前端文本=None 来源=后端选区兜底 实际查询=""
```

偶尔能搜，是因为那一次文字从别的通道（剪贴板等）恰好拿到了。

## 修复

- `ui/ocr.html` 在 `select` / `mouseup` / `keyup` 时把 textarea 的选区文字
  通过 `ocr_page_selection_set` 写给后端。
- `capture_flow::start_capture` 发现 `target_pid == 自己` 时，直接使用这份
  页面选区文字弹条，不再走剪贴板；最多等 60 ms 等页面事件到达。
- 结果：横条一出现就带着查询词，点淘宝立即搜索。

## 验证

- `--ocr-demo` 隔离实例：选中 5 个字 → `show_bar: 5 字` →
  `start_capture: 识别结果窗内选区 —— 直接用页面选区文字弹条`。
- 498 tests / clippy / check-all 全过。
