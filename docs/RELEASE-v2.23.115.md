# Quick Achieve v2.23.115

> 最后一项：后台任务统一调度。新增 `src/tasks.rs`（工作池 + 定时线程），
> 把窗口就绪前的 show/focus 与探针弹条从“每次一条线程”收进池子。

## 一、调度器

`src/tasks.rs`（158 行）：

- `spawn(f)`：短任务进 **2 条命名工作线程**（`qs-task-0/1`），单任务 panic
  被 `catch_unwind` 拦住，不会带走工作线程；
- `spawn_later(delay, f)`：所有延迟任务进**一条** `qs-timer` 线程的最小堆，
  到期后把任务投进工作池；不再为每个 `sleep + show/focus` 起一条线程；
- 池/定时线程都是懒启动，程序不触发就不占线程；
- 阻塞型任务（OCR 网络、文件框、长等待）**不塞池子** —— 2 条工作线程会被
  互相拖死，这类仍走专用线程。

## 二、迁移了哪些

| 场景 | 之前 | 现在 |
|---|---|---|
| 锁屏窗 180ms 后 show/focus | 每次一条线程 | `spawn_later` |
| 更新窗 180ms 后 show/focus | 每次一条线程 | `spawn_later` |
| 解锁后 140ms 弹横条 | 每条一次线程 | `spawn_later` |
| OCR / QR / QRGen / 图标编辑 / 设置窗 2.5s 兜底显示（5 处） | 每窗一条线程 | `spawn_later` |
| `__probe_bg_show` 后台弹条探针 | 一条线程 | `spawn`（工作池） |

重复开关这些窗口时，线程数从“每次 +1”变成固定的 **1 定时 + 2 工作**。

## 三、新增测试

`tasks` 自带 2 条单测：

- `spawn_later` 确实在延迟之后执行；
- `spawn` 能正常跑完任务。

Rust 测试 476 → **478**，全部通过。

## 四、验证

| 项目 | 结果 |
|---|---|
| `cargo test` | **478 passed** / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | **887 / 887** |
| `tools/test-pages.js` | **56 / 56** |
| `tools/check-all.js` | 五项全过 |
| `node tools/e2e/run.js` | **4/4 场景**（磨砂 11 / 恢复默认 6 / 搜索 9） |

---

**SHA256: 04B7C8D6D6868D87AC2E33EF7CB08AF7B62996C8D3C98C2FE92CDD0EF504E234**