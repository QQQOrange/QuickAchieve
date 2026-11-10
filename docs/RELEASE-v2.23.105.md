# Quick Achieve v2.23.105

> `bar.js` 第三刀：面板**平铺区渲染**（`renderFlatBody` + `panelSubMeta`）
> 搬到 `bar-panel-lazy.js`。bar.js 首次跌破 7,000 行。

## 一、拆了什么

- `renderFlatBody(groups, filterKey)`：按大类/二级/三级过滤后，生成
  `cat-title`、`cat-sub` 和 `.grid` 空壳，挂进 `panelFillers`；同步填到
  `scrollHeight ≥ 344px`，其余交给观察器；
- `panelSubMeta(groups)`：把分组元数据摊平成 `{sub: {group, gtitle, title}}`。

依赖仍走 `window.QSBar` 桥，新增：

```js
setPanelFillers: (v) => { panelFillers = v; },
PANEL_SUBS: () => PANEL_SUBS,
PANEL_SUB_FALLBACK: () => PANEL_SUB_FALLBACK,
PANEL_GROUP_FALLBACK: () => PANEL_GROUP_FALLBACK,
panelSubFilter: () => panelSubFilter,
panelCatFilter: () => panelCatFilter,
```

## 二、行数

| 文件 | 之前 | 现在 |
|---|---|---|
| `ui/bar.js` | 7,071 | **6,997** |
| `ui/bar-panel-lazy.js` | 161 | 243 |

## 三、验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 474 passed / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | **887 / 887** |
| `tools/test-pages.js` | **56 / 56** |
| `tools/check-all.js` | 五项全过 |
| `node tools/e2e/run.js` | **4/4 场景、31/31 断言**（本轮移远也 5/5 全过） |

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**