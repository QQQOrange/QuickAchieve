# Quick Achieve v2.23.57

> 修：**把 OCR 从横条移除后，横条上还剩一颗**。

## 根因

`syncOcrBtn()` 里有一条为老用户留的兼容分支：

```js
// 以前：bar_list 里没有 @ocr → 退回右侧那颗固定按钮
btnOcr.hidden = !enabled || ocrInBarList();
```

于是"从横条移除"只是删掉了 `@ocr` 这个列表项，**固定按钮立刻顶上来** —— 对用户来说等于永远移除不干净。

## 现在

OCR **只认 `bar_list`**：

| 状态 | 横条上 |
|---|---|
| `@ocr` 在列表里 | 引擎区那颗（位置可拖、可换图标） |
| `@ocr` 不在列表里 | **没有 OCR**，固定按钮永远不显示 |

`#btnOcr` 元素还留在 DOM 里（别处有引用，删了要炸），但 `syncOcrBtn()` 无条件把它藏起来。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | **453** passed |
| `node tools/check-all.js` | UI **881** / 页面 56 / 命令表 / 英文覆盖 —— 全过 |
| 新增用例 | 移除后固定按钮不显示 + 引擎区里也没有 OCR 那一格 |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 托盘图标右键 → 退出；
3. 覆盖 → 双击运行。

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: F1908EEF7282C45629D0431F77A81E80A717F39B62F72B896CCC62251CFF7E9F**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）