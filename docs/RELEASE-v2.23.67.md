# Quick Achieve v2.23.67

> 两件事：换主题**不再渐变**；引擎页里补上「★ 收藏」入口。

## 1. 换主题不再有"切换过程"

`.bar` / `.panel` 原本有 `transition: background-color .12~.14s`。横条每次弹出都可能判成另一种明暗，于是先按上次的颜色画一帧、再用 120ms 渐变过去 —— 就是你看到的"弹出来还变个色"。

修法：`applyThemePayload` 先挂 `qs-instant` 关掉所有过渡，换完主题强制结算一次，两帧后摘掉。换色瞬时到位。

## 2. 引擎页补上「★ 收藏」

`#panel` 头部新增 **★ 收藏**，和横条那颗 ★ 同一个动作（存进「快速收藏」）。没选中文字会提示，收藏成功后自动收起面板。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test -- --test-threads=1` | **459** passed |
| `cargo clippy --all-targets -D warnings` | 见上一版（本版只改前端） |
| `node tools/check-all.js` | UI 883 / 页面 56 / 命令表 / 英文覆盖 —— 全过 |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 托盘图标右键 → 退出；
3. 覆盖 → 双击运行。

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: C64E4B369D1D91563D27E011E45B0C3DE48F594B0A18C9738C0DAB7974680850**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）