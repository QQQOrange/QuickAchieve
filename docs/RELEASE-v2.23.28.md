# Quick Achieve v2.23.28

> 配置广播终于发给所有窗口了；另外全新安装不再刷两条假 PANIC。

## 一、配置广播只发给三个窗口，其余窗口永远收不到

`broadcast()` 一直只发 `bar` / `settings` / `ocr` 三个窗口。而 `i18n.js` 在**每个页面**里
都注册了 `qs://config` 监听（跟随界面语言 / 明暗主题），翻译窗、二维码窗、图标编辑器、
锁屏、更新窗因此**从来收不到**：

- 在设置里切成英文 → 这些窗口还是中文，要关掉重开才变；
- 翻译窗更明显：它按参与名单建框，改了引擎名单在开着的情况下完全不生效
  （v2.23.21 给它加的"只认最后一次"序号保护，实际上一次都没触发过）。

现在按"窗口开着才发"逐个发（没开的直接跳过，不产生多余 IPC）。

## 二、全新安装每次启动都刷两条假 PANIC

`runtime_available()` 探测 ONNX 运行时用的是 `catch_unwind` —— 崩是被兜住了，
但 v2.23.10 起 panic hook 改成"每条都写日志"，于是**还没下模型的全新安装每次启动
都会刷两条 `PANIC: ... onnxruntime.dll`**，看起来像程序崩了两次。

修了两处：

- 先看 `onnxruntime.dll` 在不在，不在就直接返回"不可用"，**根本不去触发** ort 的 panic；
- 加 `QUIET_PANIC` 标记，探测期间即使真崩也不写日志（只留 trace）。

实测：全新数据目录启动，`PANIC` 行数 **2 → 0**，首启引导照常弹出。

## 安装

1. 下载 `QuickAchieve.exe`；
2. 覆盖你现有的那份（**数据目录 `Quick Achieve data\` 不受影响**）；
3. 双击运行。首次运行没有窗口是正常的 —— 会弹托盘气泡并打开设置页。

> 从 v2.23.10 起，换上新 exe 后程序会**自己接管**（每 20 秒看一眼磁盘上的版本号，空闲时自动重启）。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 429 passed / 0 failed / 2 ignored |
| `node tools/test-ui.js` / `test-pages.js` | 827 / 56 全过 |
| 全新数据目录启动 | `PANIC` 行数 0；`first-run` 引导正常 |
| 真机启动 | 无 PANIC，托盘 / 钩子正常就绪 |

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 6DE99F3333621CE43118D8009C5668B22C15D5FF6CDA090E8412ACCA97A24DAB**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）
