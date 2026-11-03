# Quick Achieve v2.23.36

> 一键把环境信息复制出来（出问题时不用再猜）；另外修掉我自己的检查工具一个盲点，连带补了 17 处漏翻。

## 一、「复制诊断信息」

**设置 → 关于 → 诊断 → 复制诊断信息**。一键汇总成一段纯文本：

```
Quick Achieve: 2.23.36
exe 文件版本 / 路径
Windows: Windows 11 build 28000 (64 位)
架构 / WebView2 版本
管理员权限 / 提权偏好
数据目录 / config.json 体积 / log.txt 体积
备份目录 / 备份份数
界面语言 / 主题 / 触发方式 / Ctrl+C 触发
横条格数 / 自定义引擎 / 外部软件 / OCR 引擎
网格页 + 热键 / 收藏夹热键 / WebDAV
-- 日志里最近 12 条异常 --
```

- **不含任何用户内容**：不读剪贴板、不读选过的文字，日志只留判定过形态的几行；
- 复制动作在**后端**做（`clip::set_text`），不依赖 WebView2 里 `navigator.clipboard` 的权限；
- 另加 `--diag-probe`：不开界面直接打到 stdout，你遇到怪问题时可以跑一次把结果贴出来。

顺带补了个靠谱的 Windows 版本号：用 `RtlGetVersion`（`GetVersionExW` 在没有应用清单时会一律谎报 6.2）。

## 二、我自己的 i18n 检查工具原来有盲点

`tools/i18n-coverage.js` 里有一句 `if (el.hidden) return true;` —— 而设置页的分页**全是 `hidden`** 的，于是非当前分页里的中文一个都检查不到。这就是为什么新加的「复制诊断信息」整块漏翻，工具却报"只剩 1 条"。

修法：走查前先把所有 `[hidden]` 亮出来。修完立刻暴露 **17 处**一直漏翻的文案：

| 位置 | 例子 |
|---|---|
| 「关于」页 | 主要能力那句长串能力列表 |
| 「数据」页 | 被 `<code>` 断开的 4 段路径说明 |
| 引擎行帮助 | 上一版我改过的那半句 |
| `title=` 提示 ×4 | 网格分页 / 换图标 / 万能框选 |
| `placeholder` ×2 | 新页名字、搜索框 |

全部补上英文。现在真正剩下的只有两类**应当保留中文**的：语言自称「简体中文」、以及引擎名（`title=百度`，专有名词）。

## 三、顺手加的工具

`tools/find-missing-i18n.js`：静态扫页面，列出「含中文但词典里没有」的片段 —— 和运行时检查互补（不依赖可见性）。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 430 passed / 0 failed / 2 ignored |
| `node tools/test-ui.js` / `test-pages.js` | 819 / 56 全过 |
| `node tools/check-cmd-map.js` | 前端 invoke 与后端命令表一致 |
| `node tools/i18n-coverage.js` | 残留 2（语言自称 + 引擎名，均应有行为） |
| `--diag-probe` | 实跑输出完整、Windows 版本正确 |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 托盘图标右键 → 退出；
3. 覆盖 → 双击运行。

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 992A69A760057C7D01140A39CC6DF68D134F20EB6EBBE239C06D8830C7017CA7**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）
