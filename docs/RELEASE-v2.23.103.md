# Quick Achieve v2.23.103

> `bar.js` 第二刀：把「最近使用」一栏搬到 `bar-panel-lazy.js`。
> 仍是 `QSBar` 桥，不引入跨脚本全局。

## 一、拆了什么

`renderRecentSection`（约 60 行）现在在 `ui/bar-panel-lazy.js`：

- 按 `CONF.recentEngines` + `recentMax` 取最近使用的引擎；
- 排除横条上已经固定的（`barList()`），避免重复；
- 最多 20 颗，直接建格（不走懒建，观察器开销比省下的 DOM 还大）；
- 打 `data-filled="1"`，保持面板懒建的账目平衡。

依赖通过 `window.QSBar` 桥取当前值：

```js
barList: () => barList(),
panelTree: () => panelTree(),
barT: (s) => barT(s),
panelBody: () => panelBody(),
panelRowCell: (r) => panelRowCell(r),
```

## 二、行数

| 文件 | 之前 | 现在 |
|---|---|---|
| `ui/bar.js` | 7,128 | **7,071** |
| `ui/bar-panel-lazy.js` | 99 | 161 |

## 三、验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 474 passed / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | **887 / 887** |
| `tools/test-pages.js` | **56 / 56** |
| `tools/check-all.js` | 五项全过 |
| `node tools/e2e/run.js` | **4/4 场景**（磨砂 11 / 恢复默认 6 / 搜索 9；移远因远程会话指针定位失败按约定 SKIP） |

---

**SHA256: 1CCEA5BD7AF30B79326E6D34EAED8D789967A8E52F48C4F31DDEBC5260636218**
