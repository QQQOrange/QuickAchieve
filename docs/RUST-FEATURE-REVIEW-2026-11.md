# Rust 语言 / 生态特性评估（2026-11）

> 范围：Quick Achieve 当前源码（v2.23.121）。用户消息里没有实际附上代码片段，
> 因此以仓库当前实现为基准，重点看 `src/autostart.rs`、`src/config.rs`、`src/main.rs`。
>
> **方法说明**：本机无外网（`raw.githubusercontent.com` 30 s 超时），无法查阅
> Rust release notes 或 crate changelog。下表中“可用”只表示**在本机 rustc 1.99.0
> 上实测能编译**；具体 stabilization 版本一律标为“不确定”，不推测版本号。

## 1. 现状快照

| 项 | 值 |
| --- | --- |
| 工具链 | rustc / cargo 1.99.0 (b940084d7 2026-09-28)，stable-x86_64-pc-windows-msvc |
| edition | 2021 |
| 代码规模 | `src/main.rs` 25,033 行；`src/config.rs` 8,048 行 |
| 测试 | 492 passed / 2 ignored（`RUST_TEST_THREADS=1`） |
| 依赖 | wry 0.57.1 · tao 0.37.2 · windows 0.62.2 · webview2-com 0.39.1 · ureq 2.12.1(vendor)+3.5.0 · image 0.25.10 · ort 2.0.0-rc.10 · indexmap 1.9.3+2.14.2 |
| 已有现代化 | `OnceLock` 53 处、`is_some_and` 2 处、`div_ceil` 4 处、`#[allow(dead_code)]` 17 处、`#[expect]` 0 处、`LazyLock` 0 处 |

## 2. 候选方案对比

排序按用户给定优先级：**性能 > 可维护性 > 依赖数量**。

| 方案 | 具体内容 | 性能 | 可维护性 | 依赖 | 风险 / 不确定性 | 结论 |
| --- | --- | --- | --- | --- | --- | --- |
| A. std 渐进现代化 | `#[expect]` 替代 `#[allow(dead_code)]`；新代码用 let-else / `div_ceil` / `is_none_or` / `LazyLock` | 无运行时变化 | 中（能发现失效的 allow） | 0 | 低 | **推荐（P1）** |
| B. 日志惰性格式化宏 | 包一层 `if log_on() { format!() }` | 本构建 `QS_LITE_PORTABLE=true` → 日志恒开，**无收益** | 略增间接层 | 0 | 低 | 不推荐（当前构建无意义） |
| C. edition 2024 | let-chains、unsafe 语义收紧 | 无 | 需要全仓 unsafe 审计 | 0 | 高（25k 行 main.rs） | 暂缓，等模块拆分后 |
| D. wry / tao 升级 | 换更新宿主版本 | 未知 | 未知 | 不变 | 高：无法核对 changelog，WebView2 行为回归成本大 | 不推荐（离线不可验证） |
| E. 依赖收敛 | ureq 2→3 单版本；indexmap 1.9→2 | 少量二进制体积 | 中 | 可能 -1 | 中：vendor patch 可能带自定义 TLS/代理逻辑 | 暂缓 |
| F. 备份轮转简化 | v2.23.121：固定留 10 份，不再按“完整度”解密比较 | **明显**：203 份 → 10 份，save 只 readdir + 文件名排序 | 明显（少 100 行） | 0 | 已上线、有单测 | **已完成** |
| G. main.rs 模块化 | 先抽 `selection`，再抽 `bar` 生命周期 | 无 | **大**（当前单文件 2.5 万行） | 0 | 中：机械搬移 + 回归；可分批 | **推荐（P2，分阶段）** |
| H. 分配器（mimalloc 等） | 替换全局 allocator | 未知，GUI 场景通常 <5% | 0 | +1 | 中：需要基准证明 | 不推荐（无数据） |
| I. `LazyLock` 全面替换 `OnceLock` | 静态表 | 无 | 小 | 0 | 低 | 只在新代码用；53 处多为运行期缓存，不适用 |

## 3. 语言特性实测（rustc 1.99.0）

### 可用（edition 2021，实测编译通过）

`std::sync::LazyLock` · `std::cell::LazyCell` · let-else · `c"..."` 字面量 ·
`std::mem::offset_of!` · `usize::div_ceil` · `Option::is_none_or` ·
`std::path::absolute` · `std::fs::exists` · `Mutex::clear_poison` ·
`slice::first_chunk` / `split_first_chunk` · `Iterator::is_sorted` ·
`#[expect(...)]` · trait 内 `async fn` · `std::io::IsTerminal` ·
局部 `OnceLock::take` · `Duration::abs_diff`

### 可用（仅 edition 2024）

let-chains：`if let Some(x) = v && x > 0 { ... }`

### 不可用（1.99 仍为 unstable，E0658）

`impl Trait` in type aliases · `array::try_from_fn` · `Iterator::next_chunk`

> 以上“可用”只证明当前工具链支持；**具体从哪个版本开始稳定，未核对，标为不确定**。

## 4. 推荐方案

**A + G，不做 D/C/E/H。**

理由：
1. 当前性能瓶颈不在语言层。v2.23.121 的备份轮转已经从“遍历并解密目录里的备份”
   变成“readdir + 按文件名时间戳排序”，这是本轮唯一有量级意义的性能改动。
2. 最大的可维护性问题是 `main.rs` 25,033 行。继续堆特性不如先拆模块。
3. GUI 栈（wry/tao）和 OCR 运行时（ort rc）升级在离线环境无法验证，
   风险远大于收益；edition 2024 的 let-chains 收益很小，却要全仓 unsafe 审计。

## 5. 迁移步骤

### 阶段 0：基线（每次改动前）

```powershell
cargo test                      # 492 passed / 2 ignored
cargo clippy --all-targets -- -D warnings
node tools/check-all.js         # UI 887/887 · 页面 56/56
node tools/e2e/run.js           # 4/4
```

### 阶段 1：`#[expect]` 清理（约 1 小时，零行为变化）

1. 把 17 处 `#[allow(dead_code)]` 改成 `#[expect(dead_code, reason = "...")]`。
2. `cargo clippy` 若报 “unfulfilled expectation”，说明该 allow 已失效，直接删。
3. 跑阶段 0 全部门禁。

### 阶段 2：抽 `selection` 模块（1–2 天，分两步）

1. 先搬纯函数与常量：`capture_selection_fast` 的调用编排、`confirm_selection_text`、
   `post_show_copy_fill`、`drag_in_console`，保持函数签名不变，`main.rs` 只留 `use`。
2. 编译 + 全量测试 + 真机划词回归（记事本 / 浏览器 / 终端各一次）。
3. 再考虑 `bar` 生命周期；每次只搬一个内聚块，禁止顺手改逻辑。

### 阶段 3：性能验证（可选）

- 用日志时间戳量 `save` 前后耗时；若 prune 的 readdir 在备份目录很大时可见，
  再加“目录 mtime 缓存”，否则不做。
- 不做没有基准数据的优化（分配器、SmallVec 等）。

### 阶段 4：联网后再评估 D/E

- 拿到 wry/tao changelog 后，先在隔离数据目录跑 E2E 4 场景 + 真机 3 次启动，
  再决定是否升级；ureq 2→3 需要确认 vendor patch 的自定义逻辑是否已进上游。

## 6. 明确不确定项

- 各特性具体的 stabilization 版本（无外网，未核对 release notes）。
- wry 0.57.1 / tao 0.37.2 之后是否有更新版本、是否有 breaking change。
- ureq 3.5.0 是否覆盖 `vendor/ureq` 里的自定义逻辑。
- edition 2024 迁移对 25k 行 `main.rs` 的实际影响面（未做 dry-run）。
- mimalloc 等分配器在本应用的真实收益（未做基准）。
