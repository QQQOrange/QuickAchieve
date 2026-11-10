# Quick Achieve v2.23.99

> 前端拆分第一刀：把引擎面板的**懒建 / 分片填充**从 7,200 行的 `bar.js`
> 搬到 `ui/bar-panel-lazy.js`。UI 金丝雀 887、页面 56、E2E 3/3 全绿。

## 一、拆了什么

`ui/bar-panel-lazy.js`（99 行）现在负责：

- `panelFillOne`：把一个小类的格子建出来，标 `data-filled`；
- `panelFillAllChunked`：搜索/过滤时按 **8ms 时间片**分批填，避免一次 777 格卡死；
- `panelWatch`：`IntersectionObserver` + `rootMargin: 400px`，滚到跟前才建；
- `schedulePanelPush`：每批填完用 rAF 合并一次 `pushLayout`，窗口跟着面板长高。

留在 `bar.js` 的：`panelFillers` 清单、`renderFlatBody`/`renderPanel` 的布局与过滤、
`panelRowCell` 等格子构造。

## 二、为什么需要 `window.QSBar` 桥

`tools/test-ui.js` 是**逐个 `eval` 脚本**加载的，各文件顶层的 `let/const`
不在同一个全局词法环境里。上一版直接拆，测试立刻报：

```
ReferenceError: engineFilterText is not defined
ReferenceError: panelEl is not defined
```

这次在 `bar.js` 末尾挂一层只读桥：

```js
window.QSBar = {
    panelFillers: () => panelFillers,
    panelBody: () => panelBody,
    panelEl: () => panelEl,
    panelFav: () => panelFav,
    panelRowCell: (r) => panelRowCell(r),
    pushLayout: () => pushLayout(),
    applyEngineFilter: (q) => applyEngineFilter(q),
    engineFilterText: () => engineFilterText,
    panelSearchMode: () => panelSearchMode,
};
```

新文件统一用 `qsb()` 取当前值。getter 每次返回**最新**的 `let` 变量，
不存在快照过期；真实浏览器和 jsdom 都能跑。

加载顺序：`bar-panel-lazy.js` 在 `bar.js` 之前（index.html 与测试 runScripts
都已调整），函数调用都发生在 `bar.js` 执行完之后。

## 三、行数

| 文件 | 之前 | 现在 |
|---|---|---|
| `ui/bar.js` | 7,229 | **7,128** |
| `ui/bar-panel-lazy.js` | — | 99 |

## 四、验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 474 passed / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | **887 / 887** |
| `tools/test-pages.js` | **56 / 56** |
| `tools/check-all.js` | 五项全过 |
| `node tools/e2e/run.js` | **3/3 场景**（图标解码、磨砂、恢复默认） |

---

**SHA256: F8CFA860571D46819F737CB0E39DD0B850B1CA411062E1DF0875DE729D3D2EE0**
