# Quick Achieve v2.23.92

> 拆 `main.rs` 的第一刀：把「智能直达」的业务从 2.6 万行的 `main.rs` 里搬到
> `src/clip_open.rs`。命令壳只留 4 行，`main.rs` 少约 130 行。

## 一、拆了什么

- `open_clipboard_target`（约 110 行）+ `is_open_failure` + 对应的单元测试，
  整体搬到 [src/clip_open.rs](D:\Local AI\git\Quick Achieve 源代码\src\clip_open.rs)；
- `main.rs` 只保留命令壳：

  ```rust
  #[crate::t::command]
  fn open_clipboard_target() -> Result<String, String> {
      crate::clip_open::open_clipboard_target()
  }
  ```

- 模块只依赖 `clip` / `paths` / `engines` / `open` 与日志，不碰窗口、不碰
  命令注册 —— 这是选它做第一块的原因：**依赖面小，行为可完全由单测和现有
  E2E 覆盖**，不引入任何生命周期耦合；
- 判据顺序、错误文案、日志前缀（`clip-direct:`）原样保留，行为零变化。

## 二、为什么先拆它

- `main.rs` 2.6 万行、230 条命令；直接拆核心（横条/抓词/网格）会牵动大量
  全局状态与窗口生命周期，一步到位风险大；
- 这一块是"命令壳 → 业务模块"模式的样板：以后 `capture/`、`bar/`、`grid/`
  按同一形状搬（命令壳留 main，业务进模块），每次都能独立编译 + 跑测试。

## 三、验证

| 项目 | 结果 |
|---|---|
| `cargo test -- --test-threads=1` | 474 passed / 0 failed / 2 ignored（测试随模块搬家，没丢） |
| `tools/test-ui.js` | 887 / 887 |
| `tools/test-pages.js` | 56 / 56 |
| `tools/check-all.js` | 五项全过 |
| `node tools/e2e/run.js` | **3/3 场景通过** |

---

**SHA256: 8B61F27CCFF943C5141AD1D3359CED0B4147958997F74038629194B37E17C479**
