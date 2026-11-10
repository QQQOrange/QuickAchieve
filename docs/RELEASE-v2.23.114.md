# Quick Achieve v2.23.114

> 面板搜索从「全建再隐藏」改成**按数据过滤、只建命中格**。
> 这是“补强引擎面板虚拟化”的最后一环。

## 一、改了什么

以前在「更多引擎」里输入过滤词：

1. `panelFillAllChunked()` 把**所有 260 个格子**分批建出来（每格还要做图标测量）；
2. `applyEngineFilter()` 再按 DOM 把没命中的 `.cell` 隐藏。

现在：

- 每个 filler 记住当前过滤词 `filteredFor` 和只含命中行的 `renderList`；
- 过滤词变化 → 清空该 grid、只建命中格；一个都没命中 → 标 `filled` + `hidden`，
  **一个格子都不建**；
- 过滤词清空 → 恢复完整 list；面板通常已隐藏，直接同步补齐（用户看不到），
  保证下次打开是完整 DOM（UI 金丝雀第 887 条就是钉这个）；
- 非搜索态不调用这个函数，滚动懒建逻辑完全不变。

## 二、边界处理

- 搜索词从「a」改成「ab」：`filteredFor` 不同 → 重新按新词过滤，不会漏掉新命中的行；
- 清空过滤词：`applyEngineFilter('')` 发现 `hasFilteredFill()` 才触发恢复，
  不会在普通渲染时把没滚到的格子全建出来；
- 没命中的 grid 必须标 `filled='1'` 再隐藏，否则 `applyEngineFilter` 会把它当成
  “还没填的空壳”而放行（懒建 + 过滤的老坑）。

## 三、验证

| 项目 | 结果 |
|---|---|
| `cargo test` | **476 passed** / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | **887 / 887**（含「收起再打开列表完整」） |
| `tools/test-pages.js` | **56 / 56** |
| `tools/check-all.js` | 五项全过 |
| `node tools/e2e/run.js` | **4/4 场景**（磨砂 11 / 恢复默认 6 / 搜索 9） |

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**