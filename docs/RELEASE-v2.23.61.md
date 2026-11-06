# Quick Achieve v2.23.61

> 微信专用：**选中即复制 → 弹横条 → 横条收起时还原剪贴板**（按你的方案）。

## 实现

| 环节 | 做法 |
|---|---|
| 触发 | 微信里拖选结束、UIA / 复制都没拿到字 |
| 取词 | 替用户按一次 Ctrl+C；微信给 **1.5 秒**窗口（普通程序 700ms） |
| 显示 | 拿到文字就弹横条 |
| 剪贴板 | **先不还原**——横条开着这段时间剪贴板就是这段选中文字，随时能 Ctrl+V |
| 收起 | `dismiss_bar` 时还原；内容已被别人改过就不动 |
| 其他程序 | 完全不受影响（仍然读完立刻还原） |

微信也不再使用「刚证明不响应合成复制」的 5 秒负缓存（那次失败多半是时序问题）。

## 为什么只能这么做（实测结论）

微信 4.1.15.56 是 **MMUI（Qt 5.15.14 自绘）**：

- **UIA**：树里只有 `mmui::MainWindow → MMUIRenderSubWindowHW / QWidget`，聊天区**没有 TextPattern**；
- **MSAA**：只暴露一个空壳窗口（`accName=微信`、`accValue` 空、`accChild` 全部报错）。

所以「读选区」这条路不存在，只能「替用户复制」。

## 需要你实测的一点

微信是否接受**合成 Ctrl+C** 是前提。日志会写清楚：

- 成功：`capture(exact): 微信 —— 剪贴板先留着选中文字，横条收起时再还原`
- 失败：`capture(exact): 1500ms 内没读到新内容（目标 pid=… 写没写=false）`

失败就说明微信过滤了注入按键，那就走①（右键菜单取词）。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | **454** passed |
| `cargo clippy --all-targets -D warnings` | **0 告警** |
| `node tools/check-all.js` | UI 883 / 页面 56 / 命令表 / 英文覆盖 —— 全过 |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 托盘图标右键 → 退出；
3. 覆盖 → 双击运行。

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）