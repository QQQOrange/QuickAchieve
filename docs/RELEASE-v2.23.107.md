# Quick Achieve v2.23.107

> `settings.js` 第三刀：搜索的**运行时**（绑定 / 上下键 / 跳转 / 高亮 /
> 最近记录）也搬进 `settings-search.js`。搜索相关代码现在集中在一个文件。

## 一、拆了什么

`settings-search.js` 新增（原在 settings.js）：

- `bind(deps)`：输入即筛、↑↓ 选择、Enter 跳转、Esc 关闭、Ctrl+F 聚焦；
- `findIndex` / `findCursor` 状态；
- `findClose()` / `findReveal()`；
- 「最近改过的」5 条（localStorage `qsFindRecent`）；
- 动作条目点击（走原按钮 / invoke 同一条路径）。

依赖显式传入：

```js
window.QSSearch.bind({ $, gotoPane, invoke, pyInitialsOf });
```

`settings.js` 只留 4 个小包装器（`bindSettingsSearch` / `findReveal` /
`findClose` / `findBuildIndex`），测试里直接 `bindSettingsSearch()`、
`findReveal(el)` 的调用点不用改。

## 二、行数

| 文件 | 之前 | 现在 |
|---|---|---|
| `ui/settings.js` | 12,822 | **12,615** |
| `ui/settings-search.js` | 76 | 279 |

`settings.js` 从最初 12,969 行累计 **−354 行**。

## 三、验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 474 passed / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | **887 / 887** |
| `tools/test-pages.js` | **56 / 56** |
| `tools/check-all.js` | 五项全过 |
| `node tools/e2e/run.js` | **4/4 场景、31/31 断言**（移远也 5/5） |

---

**SHA256: D8CB4C7044C41E50BF4036150900E9D9770255186DB6B96108D32BFE3315A018**