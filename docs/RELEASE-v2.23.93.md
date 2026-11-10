# Quick Achieve v2.23.93

> 再少一条常驻线程：`binwatch`（20s）和设置窗回收（5s）合并成一条
> `qs-house`，按最近到期时间唤醒；顺手修掉一个被静态闸门抓到的建窗调用。

## 一、housekeeping 合并

以前：

- `qs-binwatch`：首轮 45s，之后每 20s 看一次磁盘 exe 版本；
- `qs-settings-reaper`：每 5s 看一次设置窗是否该销毁；
- 两条线程各睡各的。

现在：

- 一条 `qs-house` 线程，维护 `next_bin` / `next_reap` 两个到期时间，
  每次睡到**最近的那个**再醒；
- 两条任务互不阻塞；设置窗回收关闭时（`QS_SETTINGS_IDLE_SECS=0`）只按
  binwatch 的 20s 节奏醒，不空转；
- 少一条常驻线程和一份调度；binwatch 的 45s 首延时、20s 间隔、
  "忙时不交班"逻辑原样保留。

## 二、静态闸门抓到的一个建窗调用

`window_building_must_be_off_command_stack` 这条测试在合并后报出：

```
建窗点 open_settings_win 自身没有 spawn，
且存在未脱开命令栈的调用方（ocr_start_flow）
```

`ocr_start_flow` 的三个入口其实都在 `std::thread::spawn` 里，但
`open_settings_win` 与后面的"900ms 后聚焦 OCR 栏"原本是两段：建窗同步调用、
聚焦再起一条线程。现在合并成**同一个后台线程**：

```rust
std::thread::spawn(|| {
    open_settings_win(None, None);
    sleep(900ms);
    emit focus-tab = "ocr";
});
```

- 建窗明确不在命令栈上（静态闸门可证明）；
- 少一条一次性线程；
- 行为不变：设置页仍然打开，900ms 后切到 OCR 栏。

## 三、验证

| 项目 | 结果 |
|---|---|
| `cargo test -- --test-threads=1` | 474 passed / 0 failed / 2 ignored |
| `tools/test-ui.js` | 887 / 887 |
| `tools/test-pages.js` | 56 / 56 |
| `tools/check-all.js` | 五项全过 |
| `node tools/e2e/run.js` | **3/3 场景通过**（移远/磨砂/恢复默认） |

---

**SHA256: 5DD8B4A45C46D7049019AFDF13248F97CF177EC9B798526E5A2BFABFB788C214**
