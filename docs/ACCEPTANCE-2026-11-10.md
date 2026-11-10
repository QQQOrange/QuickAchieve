# Quick Achieve 优化验收（v2.23.104）

> 日期：2026-11-10 ｜ 分支：`main` ｜ 部署：`D:\software\myself\QuickAchieve.exe`
> 对应计划：设置搜索/恢复默认、schema 对账、面板按需渲染、图标 WebP、
> 线程收敛、CDP E2E + CI、逐步拆分 main.rs / settings.js / bar.js。

## 一、总览

| 指标 | 优化前 | 现在 | 变化 |
|---|---|---|---|
| exe 体积 | 8,197,120 B | **7,611,904 B** | −585 KB（−7.1%） |
| UI 资源 | 5.17 MB | **3.44 MB** | −1.73 MB |
| 图标目录 | 2.6 MB / 399 个 | **0.82 MB / 399 个**（220 WebP + 179 SVG） | −1.78 MB |
| `main.rs` | 26,107 行 | **25,338 行** | −769 行 |
| `bar.js` | 7,229 行 | **7,071 行** + lazy 161 | 拆出 2 块 |
| `settings.js` | 12,969 行 | **12,822 行** + reset 88 + search 76 | 拆出 2 块 |
| Rust 测试 | 474 | **474** | 保持 |
| UI 金丝雀 | 887 | **887** | 保持 |
| 页面冒烟 | 56 | **56** | 保持 |
| 前端五项自检 | 通过 | **通过** | 保持 |
| clippy `-D warnings` | 红 | **绿** | 修掉 6 处历史 lint |
| 真机 E2E | 0 场景 | **4 场景 / 31 断言** | 新增 |

## 二、逐项证据

### 1. 设置页搜索 + 分区恢复默认

- 搜索：输入即筛、↑↓ 选择、Esc 关闭、结果点选跳分页；
- 每张带数字/开关的卡片有「恢复本节默认」；滑块用样式默认（磨砂 12），
  开关用 Rust `Config::default()`；只提交改动字段，不碰路径/凭据/数组；
- E2E：
  - `search`：9 条断言（搜索框、结果列表、磨砂强度、跳分页、动作、Esc）；
  - `reset`：6 条断言（每卡有按钮、`defaults_view`、磨砂 21→12、alpha 0→55）。

### 2. 设置 schema 单一事实来源 + 一致性测试

- 后端 `config_view_of(&Config)` + `defaults_view()` 是默认值唯一来源；
- `tools/settings-schema.json` 记录 77 个控件、18 个滑块、134 个配置 key、
  别名与外部控件；
- `tools/check-settings.js` 静态对账；已作为 `tools/check-all.js` 第 5 项，
  阈值 0。

### 3. 引擎面板按需渲染

- 面板本来就按小类懒建：先填到 `scrollHeight ≥ 344px`，其余交给
  `IntersectionObserver`（`rootMargin: 400px`）滚动补；
- `renderPanel` 有内容指纹缓存（`sig === panelRenderSig` 直接复用 DOM）；
- 实测首次打开 **38ms**（260 格），重开走缓存；
- 结论：不重复造虚拟列表；把懒建/分片填充拆到 `ui/bar-panel-lazy.js`，
  以后调这块不再动 7k 行的 bar.js。

### 4. UI 图标 WebP / 体积优化

- 220 个被引用 PNG/ICO 转**无损 WebP**，SVG 保持原样；
- 修掉 3 个历史坏文件（2 个假 PNG 移除引用、1 个错扩展名改名）；
- `iconFile` 引用 656 条，missing = 0；
- E2E 面板图标断言：**242 张图，0 张坏图**。

### 5. 后台线程收敛

| 版本 | 合并 |
|---|---|
| v2.23.89 | 置顶守护 `qs-bartop`（800ms）→ `qs-baraway` |
| v2.23.91 | 右键兜底 `qs-rbtn`（20ms）→ `qs-baraway`（70ms） |
| v2.23.93 | `qs-binwatch`（20s）+ `qs-settings-reaper`（5s）→ `qs-house` |

- 横条显示时的定时唤醒从 50+14 次/秒降到 14 次/秒；
- 设置窗回收关闭时 `qs-house` 只按 20s 醒，不空转；
- 事件钩子（`qs-detect`）是消息循环，不适合塞进线程池，保持独立；
- 没有为“线程池”而池化一次性任务，避免调度开销大于收益。

### 6. CDP 探针固化为 E2E + CI

- `tools/e2e/`：`harness.js` + 4 场景（away / frost / reset / search），
  合计 **31 条断言**；隔离 `QS_DATA_DIR`、独立端口、不动正式版；
- CI（`.github/workflows/ci.yml`）：
  - clippy `-D warnings`；
  - `cargo test -- --test-threads=1`；
  - `node tools/check-all.js`（UI/页面/命令表/英文/设置项）；
  - **E2E 场景语法 + `run.js --list`**（不依赖桌面会话，本轮新增）；
- `tools/release.ps1` 同步加入 E2E 静态检查，发版前也会拦。

### 7. 逐步拆分

`main.rs` → 6 个模块（命令壳留 main，业务进模块）：

| 模块 | 行数 | 内容 |
|---|---|---|
| `clip_open.rs` | 156 | 智能直达 |
| `ocr_plan.rs` | 138 | OCR 计划/凭据 |
| `text_ops.rs` | 342 | 包裹/文本处理/智能换行 |
| `capture_env.rs` | 162 | 剪贴板 owner/微信/前台/终端 |
| 其余既有模块 | — | config/clip/win/engines… |

前端：

| 文件 | 拆出 |
|---|---|
| `bar.js` | `bar-panel-lazy.js`（懒建 + 最近使用；`QSBar` 桥） |
| `settings.js` | `settings-card-reset.js`（依赖对象传入）、`settings-search.js` |

## 三、验证命令（2026-11-10 跑过）

```powershell
cargo test -- --test-threads=1          # 474 passed / 0 failed / 2 ignored
cargo clippy --all-targets -- -D warnings   # 通过
node tools/test-ui.js                   # 887 / 887
node tools/test-pages.js                # 56 / 56
node tools/check-all.js                 # 五项全过
node tools/e2e/run.js                   # 4/4 场景、31 断言
```

## 四、后续（非阻塞）

- `bar.js` 还可继续拆 `renderPanel` / `renderFlatBody`；
- `settings.js` 可拆搜索绑定（`bindSettingsSearch`）与剪贴板历史；
- `main.rs` 可拆 grid 命令组 / settings API；
- 这些是持续维护项，不影响本次七个目标的验收。

---

**部署校验**：`D:\software\myself\QuickAchieve.exe` 版本 = Release 版本，
日志出现 `update: 检查完成 —— 远端 vX，当前 vX，有新版=false`。
