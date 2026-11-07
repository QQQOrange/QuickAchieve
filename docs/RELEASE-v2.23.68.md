# Quick Achieve v2.23.68

> 设置页「横条上的引擎」里补上 **★ 收藏**。

## 原因

横条那颗 ★ 是**固定按钮**，不属于引擎目录 —— `barList` 里根本没有它，设置页按 `barList` 渲染自然看不到。

## 修法

在 `#barList` 上面单独加一行 **「★ 收藏（快捷收藏）」+ 开关**：

- 新增配置 `fav_quick_show`（默认开）；
- `config_view` 暴露 `favQuickShow`，`set_setting` 认这个键；
- 横条 `applyFavQuickVisible()` 在每次配置广播/启动时按它显示或隐藏 `#btnFav`。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test -- --test-threads=1` | **459** passed |
| `node tools/check-all.js` | UI 883 / 页面 56 / 命令表 / 英文覆盖 —— 全过 |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 托盘图标右键 → 退出；
3. 覆盖 → 双击运行。

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 746DCBFE7DAA13FB9A63D203A8E76E6393147F08CA3221AF27C1AB853EC88C3D**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）