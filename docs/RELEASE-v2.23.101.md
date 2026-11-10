# Quick Achieve v2.23.101

> 前端拆分第三刀：设置页搜索的**索引构建**搬到 `ui/settings-search.js`。
> 搜索框的绑定/跳转交互仍留 `settings.js`。

## 一、拆了什么

`ui/settings-search.js`（76 行）现在负责：

- `findPaneName`：从顶栏 tab / 分页 chip 取中文分页名（去掉 emoji 图标）；
- `FIND_ACTIONS`：搜索里可直接执行的动作（导出 / 导入 / 恢复默认 / 诊断 / 打开配置目录）；
- `findActionEntries`：把动作并进索引；
- `buildIndex(pyInitialsOf)`：扫 `.pane[data-pane]` 下的 `.row / .tools-title / .help-item > summary`，生成条目。

`settings.js` 只留一个 5 行包装器：

```js
function findBuildIndex() {
    const out = window.QSSearch ? window.QSSearch.buildIndex(pyInitialsOf) : [];
    findIndex = out;
    return out;
}
```

`findIndex` / `findCursor` 状态、输入框绑定、上下键选择、跳转高亮、动作点击
仍在 `settings.js` —— 这次只搬“建索引”这块纯逻辑。
`pyInitialsOf` 由调用方传入，新文件不依赖 settings.js 的顶层函数。

## 二、行数

| 文件 | 之前 | 现在 |
|---|---|---|
| `ui/settings.js` | 12,884 | **12,822** |
| `ui/settings-search.js` | — | 76 |

## 三、验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 474 passed / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | **887 / 887** |
| `tools/test-pages.js` | **56 / 56** |
| `tools/check-all.js` | 五项全过 |
| `node tools/e2e/run.js` | **3/3 场景**（磨砂 11、恢复默认 6；移远因当前远程会话指针定位失败按约定 SKIP） |

---

**SHA256: B73B224D7488AFF56D4CFCDD9E11055793F0B5FC4753997C100AEE410BAFCC12**
