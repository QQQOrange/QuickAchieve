# Quick Achieve v2.23.15

> 这一版主要是"让失败看得见"：命令出错会进日志，界面按钮和后端命令对不上会在发布时被拦下来。

## 一、命令失败现在一定进日志

界面里有近百处 `invoke(...)` 是"发了就不管"的。后端返回错误时，前端只会产生一个
**未处理的 Promise rejection** —— 控制台里都看不到，用户那边就是"点了没反应"，
事后完全没法查。

现在 IPC 桥在 reject 之前会顺手把「哪个命令、报了什么」写进 `log.txt`：

```
[ipc] 命令失败 copy_text → 剪贴板被占用
```

以后你再遇到"点了没反应"，把 `log.txt` 里 `[ipc]` 开头的行发出来就能直接定位。

## 二、发布流程多了一道自动检查

新增 `tools/check-cmd-map.js`：把 `ui/` 里所有 `invoke('x')` 和 `src/main.rs` 里所有
`#[crate::t::command]` 对一遍，**缺一个就发布失败**。

这类 bug（界面上有个按钮，点下去后端根本没这个命令）靠人眼极难发现。
当前 190 个前端调用全部对得上。

## 三、OCR 结果窗的建窗标志加了 RAII 兜底

`OCR_WIN_BUILDING` 原来只在 `build()` 之后手动复位：闭包里一 panic
（建窗失败、主题查询炸了）标志就永久为 `true`，之后**所有**结果窗都拒绝创建 ——
日志只有"首窗还在建，等它"，用户只能重启。

## 安装

1. 下载 `QuickAchieve.exe`；
2. 覆盖你现有的那份（**数据目录 `Quick Achieve data\` 不受影响**）；
3. 双击运行。首次运行没有窗口是正常的 —— 在任务栏右下角托盘区找它。

> 从 v2.23.10 起，换上新 exe 后程序会**自己接管**（每 20 秒看一眼磁盘上的版本号，空闲时自动重启）。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 429 passed / 0 failed / 2 ignored |
| `node tools/test-ui.js` | 827 / 827 |
| `node tools/test-pages.js` | 56 / 56 |
| `node tools/check-cmd-map.js` | 190 个前端调用全部有后端实现 |
| 真机启动 | 无 PANIC，托盘 / 钩子正常就绪 |

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: AACE5FE1396F90EAAFA6A0004FABDAC4329195E1AB32FC2E76F4E6C934A3C0B9**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）
