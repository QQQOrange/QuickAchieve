# Quick Achieve v2.23.89

> 少一个常驻线程：置顶守护并进「移远收起」循环；设置项对账扩到全部
> 77 个控件；真机 E2E 正式进仓库。

## 一、线程合并：qs-bartop → qs-baraway

- 原先 `spawn_bar_topmost_watch` 自己一条线程，每 800ms 醒一次：
  横条藏着也照醒，只为判断 `is_bar_shown()`；
- 现在置顶重申并进 `qs-baraway` 的循环（横条显示时才跑，70ms 一拍）：
  - 每 ≥800ms 重申一次 `SetWindowPos(HWND_TOPMOST)`；
  - 横条没显示时**一次系统调用都不做**；
  - 少一条常驻线程、少 800ms 一次的空唤醒；
- 行为不变：别的置顶浮层仍然压不住横条。

## 二、设置项全量对账

- 新增 `tools/settings-schema.json`：把「控件 key ≠ 配置 key」的 8 个别名
  和 15 个本地/专用命令控件显式登记（如 `barIdle → barIdleSec`、
  `aiPasteDelay → imgAiPasteDelay`、`clipHist*` 走 `clip_hist_*`）；
- `tools/check-settings.js` 从「只查 18 个滑块」扩到：
  - **77 个控件**：每个都必须直接是 `config_view_of` 的 key，或在 aliases，
    或在 external 里写明“为什么不在配置视图里”；
  - 18 个滑块的控件 / 标签 / key / range 类型；
  - 134 个配置 key 的下发列表；
  - aliases 目标必须存在、external 必须有对应控件；
- 接进 `check-all.js`，前端自检从 4 项变 **5 项**。

## 三、真机 E2E 进仓库

- `tools/e2e/`：`away` / `frost` / `reset` 三个场景，19 条断言；
- 独立数据目录 + CDP 端口，`QS_NO_ELEVATE=1`，清理只杀
  `target/release` 那一份 exe，不碰正式版；
- 这台机器是活的：静止场景如果检测到外部指针移动，会 `SKIP` 并重开一次，
  不再把“用户碰了鼠标”误报成产品失败；
- CI 不加 GUI 场景（默认 runner 没有交互桌面），说明写在 README。

## 四、验证

| 项目 | 结果 |
|---|---|
| `cargo test -- --test-threads=1` | 474 passed / 0 failed / 2 ignored |
| `tools/test-ui.js` | 887 / 887 |
| `tools/test-pages.js` | 56 / 56 |
| `tools/check-all.js --quiet` | 五项全过（设置项 0 问题） |
| `node tools/e2e/run.js` | **3/3 场景、19/19 断言** |

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**
