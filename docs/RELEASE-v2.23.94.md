# Quick Achieve v2.23.94

> 拆 `main.rs` 第二刀：OCR 的「这一轮跑哪几家 / 凭据齐不齐」搬到
> `src/ocr_plan.rs`。纯判断逻辑，不碰窗口和剪贴板。

## 一、拆了什么

从 `main.rs` 移出四个函数（含注释约 200 行）：

| 原函数 | 新位置 |
|---|---|
| `ocr_engine_plan` | `ocr_plan::engine_plan` |
| `ocr_plan_with_local_fallback` | `ocr_plan::plan_with_local_fallback` |
| `creds_ready` | `ocr_plan::creds_ready` |
| `online_engine` | `ocr_plan::online_engine` |

`main.rs` 用一行别名导入，所有调用点（`ocr_start_flow`、`trans_region`、
单测）一个字都不用改：

```rust
use ocr_plan::{
    creds_ready, engine_plan as ocr_engine_plan, online_engine,
    plan_with_local_fallback as ocr_plan_with_local_fallback,
};
```

留在 main 的：

- `OCR_FORCE_ENGINES` 静态（格子绑定 `@ocr:<id>` 的轮次状态）→
  `ocr_forced_engines()` 改成 `pub(crate)`，模块通过 `crate::` 读取；
- `ocr_creds()`（凭据字段映射，老字段回退逻辑）→ `pub(crate)`，模块调用；
- `ocr_start_flow` / 窗口编排 / 结果页，仍在 main。

## 二、为什么拆它

- 只依赖 `config` + `ocr_engines` 注册表，是纯函数；
- 它的三个入口（横条 OCR、Ctrl+Alt+O、托盘）和「翻译里的识别」共用同一份
  计划逻辑，集中到一个模块后，以后调兜底策略只有一处；
- 沿用 v2.23.92 的样板：**命令/编排留 main，纯业务进模块**。

## 三、行数变化

| 文件 | 之前 | 现在 |
|---|---|---|
| `src/main.rs` | 25,985 | **25,824** |
| `src/ocr_plan.rs` | — | 128 |

（`main.rs` 本轮开始前是 26,107 行；两刀之后累计 -283。）

## 四、验证

| 项目 | 结果 |
|---|---|
| `cargo test -- --test-threads=1` | 474 passed / 0 failed / 2 ignored |
| `tools/test-ui.js` | 887 / 887 |
| `tools/test-pages.js` | 56 / 56 |
| `tools/check-all.js` | 五项全过 |
| `node tools/e2e/run.js` | **3/3 场景通过** |

---

**SHA256: F639C84F38CB2D092CA150C147476C0B885188A1154958E6C00658579847B4A9**
