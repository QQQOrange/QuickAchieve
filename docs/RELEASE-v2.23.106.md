# Quick Achieve v2.23.106

> `bar.js` 第四刀，也是最大的一刀：**面板整体渲染 `renderPanel`（262 行）**
> 搬到 `bar-panel-lazy.js`。`bar.js` 从 7,229 行降到 6,765 行。

## 一、拆了什么

`renderPanel(focusFilter, keepScroll)` 现在在 `ui/bar-panel-lazy.js`：

- 内容指纹（过滤条件 / 自定义引擎 / 收藏 / 最近使用 / 横条成员）；
- 指纹相同 + 归属是引擎面板 → 只复位滚动、还焦点、同步分类高亮，**不重建 DOM**；
- 重建时：sticky 头（收藏入口 + 大类胶囊 + 二三级胶囊）、最近使用、
  `.eng-noresult` 占位、滚动归零、过滤重放、滚动监听只挂一次。

面板**状态仍留在 bar.js**，通过 `QSBar` 桥读写：

```js
getConf / panelGroupFilter / panelSubFilter / panelCatFilter
panelFav / setPanelFav / panelRenderSig / setPanelRenderSig
panelRenderOwner / setPanelRenderOwner / panelScrollBound / setPanelScrollBound
setFavFromEngines / inputEngFilter / QS_FAV_PREFIX
```

好处：以后改面板渲染不用在 7k 行的 bar.js 里翻；`renderPanel` 与懒建/平铺区
现在同处一个文件，职责完整。

## 二、行数

| 文件 | 之前 | 现在 |
|---|---|---|
| `ui/bar.js` | 6,997 | **6,765** |
| `ui/bar-panel-lazy.js` | 243 | 489 |

`bar.js` 从最初的 7,229 行累计 **−464 行**（−6.4%）。

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

**SHA256: 3FE1DDA7D75C75444BD0D20126F70A3B3E1B7654815DA0DA6DF97397BCA2A8B4**