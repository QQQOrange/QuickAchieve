# Quick Achieve v2.23.108

> `settings.js` 第四刀：剪贴板历史整段搬到 `ui/settings-cliphist.js`。

## 一、拆了什么

`ui/settings-cliphist.js`（99 行）现在负责：

- `paintClipHist`：渲染记录（时间、来源应用、点击复制回剪贴板）；
- `loadClipHist` / `saveClipHist`：读 `clip_hist_cfg`、写 `clip_hist_set`；
- `bindClipHist`：开关 / 上限 / 持久化 / 清空按钮；
- `clipHistCfg` 状态与 `clipHistTimeText`。

依赖由 settings.js 的包装器显式传入：

```js
window.QSClipHist.bind({ $, invoke, toast });
```

隐私口径不变：只存本机、不参与同步、随时可清空；数据在
`cliphist-data.json`，设置独立存 `cliphist.json`，不进 config、不会被
备份 / WebDAV 带出去。

## 二、行数

| 文件 | 之前 | 现在 |
|---|---|---|
| `ui/settings.js` | 12,615 | **12,536** |
| `ui/settings-cliphist.js` | — | 99 |

`settings.js` 从最初 12,969 行累计 **−433 行**。

## 三、验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 474 passed / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | **887 / 887** |
| `tools/test-pages.js` | **56 / 56** |
| `tools/check-all.js` | 五项全过 |
| `node tools/e2e/run.js` | **4/4 场景**（磨砂 11 / 恢复默认 6 / 搜索 9；移远按环境 SKIP） |

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**