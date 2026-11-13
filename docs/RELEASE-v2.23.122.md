# Quick Achieve v2.23.122

> 三项一起做：**继续拆 `main.rs`**、**性能优化**、**改善 Chrome / Electron 取词（不用 OCR）**。

## 一、结构：`main.rs` 从 25,033 行降到 23,429 行

这一版没有新增功能，只是把混在 `main.rs` 里的选区与横条逻辑按职责搬出去：

| 文件 | 行数 | 职责 |
|---|---:|---|
| `src/selection.rs` | 875 | 选区取词 / 确认 / 补字；剪贴板三级通道、编辑控件、UIA、终端保护 |
| `src/capture_flow.rs` | 313 | 取词调度：先弹空条、等上一次取词、后台补字、精确复核 |
| `src/bar_show.rs` | 344 | 横条展示实体：锚点、让位、主题、发事件 |
| `src/bar_lifecycle.rs` | 139 | 横条收起 / 挂起 |
| `src/bar_geo.rs` | 70 | 横条矩形、内外判定、显示时刻 |

所有被搬走的函数签名与行为不变；`main.rs` 只保留钩子、窗口和 `pub(crate) use`。

## 二、修复

### 取词焦点交还

Chrome / Edge / Electron 的 UIA 内容树无法唤醒（实测 25 个框架元素、没有 TextPattern），
只能走合成 Ctrl+C。失败日志里看不到“前台是我们自己”，但按键确实打空了 ——
怀疑横条异步抢焦点，或者 `SetForegroundWindow(root)` 成功、键盘焦点却没回到网页内容。

新增 `win::gui_focus()` 与 `win::focus_for_copy()`：

- `AttachThreadInput` 到目标线程后 `SetForegroundWindow(root)`；
- 用 `GetGUIThreadInfo` 取目标线程自己的 `hwndFocus`，`SetFocus` 交回去（不硬塞 root）；
- 逆序 detach，返回前台是否真的切过去了。

`capture_selection_fast()` 在发送前、300 ms / 600 ms 重试前都调用它；
如果发现焦点还在本程序，先打日志再交还，30 ms 后重发。

### 液态玻璃恢复

`ui/glass.js` 里 `var frostMode = isFrost(c);` 还在，但 `isFrost()` 在 v2.23.85 被删了。
从此每次 `draw()` 都抛 `ReferenceError: isFrost is not defined`，画布停在第一帧，
玻璃样式看起来“不生效”。本版补回：

```js
function isFrost(c) {
  return !!c && c.barGlass === false && (Number(c.barBlur) || 0) > 0;
}
```

## 三、性能

- **自动明暗缓存**：`autotheme::detect_cached()` 按前台窗口缓存 3 秒，只缓存判出来的结果
  （`None` 不缓存，避免视频/灰区把静态页面带偏）。同一页面连续划词：
  `主题` 255 ms → 0 ms，横条端到端 **265 ms → 9 ms**；首次仍需判定。
- **启动清扫后台化**：`sweep_stale_webview_dirs()` 不再挡在第一个 WebView2 窗口之前，
  旧缓存（实测 140 ms）交给后台线程删。
- **热路径窄读**：`app_allowed` 与 `bar_show` 不再整份克隆 `Config`（收藏夹 dataURL 可达数 MB），
  改用 `cfg_with` 只读需要的字段。

## 四、验证

- `cargo test`：**493 passed / 2 ignored**。
- `cargo clippy --all-targets -- -D warnings`：干净（仅 vendored `ureq` 一条 warning）。
- E2E：4/4（away 5 / frost 11 / reset 6 / search 9）；UI 金丝雀 887/887；页面冒烟 56/56。
- 真机（Windows 11 28000，200% 缩放）：
  - Edge 文本框里“先弹空条 → 后台复制补字”拿到 **113 字**；
  - 同一窗口连续划词第二次端到端 **9 ms**；
  - 玻璃绘制日志有 `glass: 已抓背景 …`，不再有 `isFrost is not defined`。

## 五、说明

- 浏览器 / Electron 的取词仍以合成复制为主，UIA 只在支持 TextPattern 的程序里用；
  本次没有引入 OCR 兜底。
- 测试覆盖的是 Chromium 文本框与普通网页段落，不宣称覆盖所有站点；
  合成拖选偶尔不产生选区，属于测试环境限制。
