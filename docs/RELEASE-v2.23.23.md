# Quick Achieve v2.23.23

> 横条审计剩下的五条 —— 都是"偶发，但一旦发生就很怪"的。

## 一、网格里快速连点两个 ×，会吞掉一次

`gridRemoveAt` / `gridAddItem` 都是"读当前页 → 改数组 → `await set_grid_items` → 回写"。
两次调用会在**任何一次 await 返回之前**读到同一份 `pg.items`，后返回的那次用旧数组
整份覆盖 —— 前一次的结果被吞掉（移出的项又回来了、先加的那项没加上），
前后端内容此后不一致，要等下次唤起才对齐。

现在用一个 promise 链把这些写操作排成队。

## 二、长按拖动没有异常收尾

只有 `pointerup` 一条收尾路径。长按那 320ms 里横条被重画（配置广播 / tip 刷新），
或触控被系统手势打断时，`barDrag` 会挂在那儿：幽灵留在页面上、`active` 一直是 true。
现在补了 `pointercancel` / `blur` 兜底，`setPointerCapture` 也包了 try。

## 三、默认引擎可能是功能哨兵

`barList()[0]` 可能是 `@ocr` / `@fav:xxx` / `@go:xxx`。首次使用、还没记录 `lastEngine`、
横条第一格又是功能格时，回车会发一条**注定被拒**的 `open_search`，界面毫无反应。
现在跳过所有 `@` 开头的项，实在找不到就明确提示。

## 四、钉住被静默取消

`togglePanel` 里那段"取消钉住"在**打开**面板时也会跑 —— 你刚钉住收藏面板，
再按一次收藏夹快捷键（打开面板）就被取消了，而且完全看不出来。现在只在收起时清。

## 五、网格可能半透明登场

上一次文字横条在"淡出延迟"到点前被收起时，`qs-dim` 会留在横条上 ——
网格随后登场就是半透明的，要等鼠标进出一次才恢复。现在渲染网格时先摘掉。

## 安装

1. 下载 `QuickAchieve.exe`；
2. 覆盖你现有的那份（**数据目录 `Quick Achieve data\` 不受影响**）；
3. 双击运行。首次运行没有窗口是正常的 —— 在任务栏右下角托盘区找它。

> 从 v2.23.10 起，换上新 exe 后程序会**自己接管**（每 20 秒看一眼磁盘上的版本号，空闲时自动重启）。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 429 passed / 0 failed / 2 ignored |
| `node tools/test-ui.js` / `test-pages.js` | 827 / 56 全过 |
| 真机启动 | 无 PANIC，托盘 / 钩子正常就绪 |

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）
