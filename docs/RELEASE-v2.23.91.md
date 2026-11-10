# Quick Achieve v2.23.91

> 再少一条高频轮询线程：右键兜底从独立的 **20ms** 线程并进「移远收起」
> 的 **70ms** 循环。右键菜单的主路（低级鼠标钩子）不受影响，仍然是即时的。

## 一、线程合并：qs-rbtn → qs-baraway

之前：

- `qs-rbtn` 线程在横条显示时每 **20ms** 轮询一次右键（50 次/秒），横条藏着
  时挂在 `RBTN_PARK` 条件变量上等 `layout_bar` 唤醒；
- `qs-baraway` 线程在横条显示时每 **70ms** 一拍，本来就要读指针位置。

现在：

- 右键兜底并进 `qs-baraway`：同一拍里读 `rbutton_down()`，上升沿 + 指针在
  横条内 → 发 `qs://rclick`；
- 少一条常驻线程；横条显示时的定时唤醒从 **50+14 次/秒**降到 **14 次/秒**；
- 右键兜底的延迟上限从 20ms 变成 70ms（最多多等一拍，肉眼无感）；
- **主路不变**：`hook.rs` 的 `Ev::RDown` 仍是即时响应，这条只是防别的
  钩子把 `WM_RBUTTONDOWN` 吃掉；
- 删掉 `RBTN_PARK` 静态、`wake_rbutton_watch`、`layout_bar` 里的唤醒调用，
  以及留下的空 `if first_show {}`。

## 二、验证

| 项目 | 结果 |
|---|---|
| `cargo test -- --test-threads=1` | 474 passed / 0 failed / 2 ignored |
| `tools/test-ui.js` | 887 / 887 |
| `tools/test-pages.js` | 56 / 56 |
| `tools/check-all.js` | 五项全过 |
| `node tools/e2e/run.js` | **3/3 场景通过**（移远/磨砂/恢复默认） |

E2E 运行期间指针被外部移动时，静止断言按约定 `SKIP`，其余断言照常通过。

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**
