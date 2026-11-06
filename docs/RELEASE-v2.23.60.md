# Quick Achieve v2.23.60

> 撤掉 OCR 兜底（影响精确性）；改成**明确提示"这个软件不支持划词弹条"**，并带上程序名。

## 一、撤掉 OCR 兜底

v2.23.56 加的那套全部移除：

| 项 | 处理 |
|---|---|
| `confirm_selection_text` 里的"拿屏幕识别顶替"分支 | 整段删除 |
| `barOcrFallback` 配置字段 | 删除 |
| 设置页「取不到文字时用屏幕识别兜底」 | 删除 |
| `config_view` / `set_setting` 入口 | 删除 |

现在只有两条**非 OCR** 通道：**系统无障碍（UIA TextPattern）→ 合成 Ctrl+C 读剪贴板**。
两条都拿不到 = 这个程序不支持，**不弹条**（不猜、不 OCR）。

## 二、新增"这个软件不支持"的提示

以前的提示走 `toast_to_settings`，只往**设置窗口**发——用户在微信里划词时设置窗没开，等于没提示。

现在两条都发，且带程序名：

- **小提示条**（和「已使用：xxx」同一套）：每次划词都提示
  `「微信」不支持划词弹条 —— 用 Ctrl+Alt+O 截图识别`，同一程序 4 秒内只发一条；
- **托盘气泡**：同一程序只弹一次，内容更完整（说明是"不响应系统选区接口"）。

## 三、关于"让所有软件都能划词"

不借助 OCR 的话，程序必须**自己暴露选区**（UIA 文本模式）或**响应合成 Ctrl+C** 才有字可拿。
微信 4.x 这类 Electron 自绘界面两样都不给——外部程序没有合法途径读到它选中的文字。
这不是本软件的 bug，是那个程序没实现可访问性接口。

所以这版的方向是：**能支持的做到最好，不能支持的明确告诉你**，而不是用 OCR 猜一段可能不准的文字塞进横条。

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

**SHA256: A57041DAD8ED12CFA9BC6B3954A14408730366405E35D820FFC01E8D0F2CB9C6**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）