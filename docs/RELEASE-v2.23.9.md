# Quick Achieve v2.23.9

> **单击 / 双击不会再弹横条了。** 这次是顺着日志找到的真根因，不是猜的。

## 问题

用户报：「双击也会出现横条，哪怕没选中文字……横条只会以设置里面规定的两种方式出现」。

拿他机器上的 `log.txt` 一看就清楚了：

```
uia: 读到选区（耗时 4ms）
start_capture: UIA 通道拿到 1 chars      ← 只拿到 1 个字符
mouseup: drag=false (move=false moved_far=false) at=(2236,804)   ← 就是一次单击
```

## 根因

Chromium 系（`Chrome_RenderWidgetHostHWND`）在**只有光标、没有任何选区**时，
无障碍接口 `GetSelection()` **照样返回一个范围** —— 那个范围落在光标处，
`GetText()` 会把它读成 **1 个字符**。

我们原来的判据是"有没有文本"：拿到 1 个字符 = "有字" = 当成选区 → 弹条。
所以**每一次单击都弹**，双击只是按了两次；跟"选中没选中"根本无关。

## 修法

判断这个范围是不是**折叠**的（起止是同一个端点）：

```rust
r.CompareEndpoints(TextPatternRangeEndpoint_End, &r, TextPatternRangeEndpoint_Start)
// == 0 → 只有光标，不是选区 → 跳过
```

两点保证它不会误伤：

- 真的只选了**一个字**时，起止端点不同，不会被当成光标跳过；
- `CompareEndpoints` 取不到结果时**退回原判据**——宁可放过，也不能把真实选区弄丢。

浏览器、WebView2（包括本软件自己的界面、Electron 系）都走这套判据。

## 实测确认

用内置的 UIA 诊断探针对着本机正在运行的 Electron 窗口跑了一次：

```
uia: [2] 「Keyboard Enhance — DeepSeek Harness」→ 有 TextPattern
uia:     选区数 = 1
uia:     选区[0] = ""　折叠=true（折叠=true 表示只有光标，不是选区）
```

就是这条被当成"选区"的东西。顺手给探针补了 `折叠=true/false` 这一位输出 ——
以后再遇到"没选中却弹条"，跑一次 `--uia-probe` 就能直接看出是不是它。

## 现在的口径

横条只在**真的选到了文字**时出现：

| 动作 | 结果 |
|---|---|
| 单击 / 双击（没选中文字） | **不弹** |
| 双击选中一个词 | 弹（那确实是一次选中） |
| 拖选文字 | 弹 |
| 「按住 Ctrl 才弹」模式下没按 Ctrl | 不弹 |
| 没取到文字就按 Ctrl+C | 不弹（v2.23.8 起） |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 覆盖你现有的那份（**数据目录 `Quick Achieve data\` 不受影响**）；
3. 双击运行。首次运行没有窗口是正常的 —— 在任务栏右下角托盘区找它。

> ⚠ 如果软件正在运行，Windows 会拒绝覆盖。先退出（托盘右键 → 退出），再替换。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 见发布日志 |
| `node tools/test-ui.js` / `test-pages.js` | 见发布日志 |
| UIA 探针实测 | 折叠选区被正确识别（`折叠=true`） |
| 真机启动 | 无 PANIC，托盘 / 钩子正常就绪 |

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 4929942FE321405859BF65EEBCE4FA499DD21C16D7FC8A5D52338AC6B0D7F359**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）
