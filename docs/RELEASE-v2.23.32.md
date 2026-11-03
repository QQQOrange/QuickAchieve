# Quick Achieve v2.23.32（紧急修复，接 v2.23.31）

> 找到了系统级 Ctrl+C 受影响的真正元凶：**我们注入的 `Ctrl↑` 会把你按住的 Ctrl 在系统里"松开"**。

## 机制

划词结束时我们会注入一整套 `Ctrl↓ C↓ C↑ Ctrl↑`（四个事件分四次 `SendInput`，
中间各留 8ms —— 那是为了让 Qt / Chromium 系程序认合成按键）。

而"**选完文字立刻按 Ctrl+C**"是最常见的动作。两套按键在系统状态里交错时：

- 我们末尾那次 **`Ctrl↑` 会把你物理按住的 Ctrl 在系统里置成"松开"**；
- 接下来你按的 C 到目标程序那里就是一个光秃秃的 `c`，不构成快捷键 → **复制不发生**；
- 反过来我们的 `C↑` 也可能把你正在按的 C 提前松开。

所以 v2.23.31 之后仍会"有时候复制不上"，而且**不只影响本软件** —— 只要用过划词就会出现。

## 修法

1. **注入前先看物理键状态**：用 `GetAsyncKeyState` 查 Ctrl / Shift / Alt / Win，
   只要有任何一个按着就**不注入**（返回 false，日志留痕）。上层照常走
   UIA / 屏幕识别兜底 —— **漏一次取词，远好过打断你的复制**；
2. **复制前剪贴板已经被改过就不还原**：右键菜单"复制"不按修饰键，那种情况
   内容比对区分不出来（同一段文字），所以再加一道"改动发生在谁之后"的判断；
3. 新增 `--ctrlc-probe` 诊断：只报"此刻有没有修饰键 / 会不会跳过注入"。

## 实测

```
① 什么都不按   → MODS=false  SENT=true     （正常注入）
② 按住 Ctrl    → MODS=true   SENT=false    （跳过 —— 正是要的）
```

## 另外核实过、没问题的

| 检查 | 结论 |
|---|---|
| 键盘钩子会不会吞 Ctrl+C | 不会：`should_swallow_down` 里有专门的提前返回（`vkCode==0x43 && Ctrl && !Alt && !Shift && !Win → false`） |
| 热键独占会不会抢 Ctrl+C | 不会：只独占「网格页」「收藏夹」两个快捷键 |
| 剪贴板有没有被长期占着 | 没有：刚实测连续三次 `OpenClipboard` 都成功；代码里每条打开路径都有配对关闭（预分配失败、`EmptyClipboard` 失败等早退分支也都关了） |
| 有没有卡住的修饰键 | 没有：`GetAsyncKeyState` 实测 Ctrl/Shift/Alt/Win 全部 up |
| 剪贴板监听 / 定时回写 | 不存在（没有 `AddClipboardFormatListener`，没有周期性剪贴板任务） |

## 安装

1. 下载 `QuickAchieve.exe`；
2. **先在托盘图标上右键 → 退出**（旧进程不退，新版本起不来）；
3. 覆盖 → 双击运行。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 430 passed / 0 failed / 2 ignored |
| `node tools/test-ui.js` | 827 / 827 |
| `--ctrlc-probe` 按住 Ctrl | `MODS=true SENT=false` |
| `--ctrlc-probe` 不按键 | `MODS=false SENT=true` |

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: ADEB8A86A0A2B63C96B9757A789470C632AB8047CEE6FF2FE1EB8F21C031112B**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）
