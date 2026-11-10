# Quick Achieve v2.23.95

> 拆 `main.rs` 第三刀：包裹符号与文本处理的**纯函数**搬到
> `src/text_ops.rs`；顺手把 CI 的 clippy 闸门修回全绿。

## 一、拆了什么

从 `main.rs` 移出（含注释约 240 行）：

| 内容 | 说明 |
|---|---|
| `WRAP_PAIRS` | 符号表（双引号 / 书名号 / 方头括号…） |
| `wrap_strip` / `wrap_apply` | 拆一层 / 包、拆、换对的核心纯逻辑 |
| `is_cjk` / `is_cjk_punct` | 中文字符、中文标点判断 |
| `text_op_trim` / `text_op_punct` / `text_op_title` / `text_op_sentence` | 智能去空格、标点全半角、标题大小写、句首大写 |
| `apply_text_op` | `@txt:<op>` 的分发 |

留在 main 的：

- `wrap_text` / `text_op` 两个命令（要读选区、备份剪贴板、回写目标软件）；
- `wrap_paste_into_target` / 剪贴板还原；
- `unwrap_lines`（智能换行，依赖 `ends_sentence` / `is_structural_line`，下刀再拆）。

`main.rs` 用别名导入，命令与单测调用点不变。

## 二、行数

| 文件 | 之前 | 现在 |
|---|---|---|
| `src/main.rs` | 25,824 | **25,591** |
| `src/text_ops.rs` | — | 243 |

`main.rs` 从 26,107 行起，三刀累计 **-516 行**。

## 三、CI 闸门修复（原本就是红的）

跑 `cargo clippy --all-targets -- -D warnings` 时发现几处历史问题，一并修掉：

- `src/clip.rs`：一个已经搬走的函数留下的孤立 doc 注释（空行后接 doc）；
- `src/config.rs`：`#[test]` 与文档块错位，导致
  `sanitize_clamps_bar_auto_hide_thresholds` 没有 `#[test]`、bar_bg 测试
  被两个属性夹住；
- `src/main.rs`：`ocr_creds_view(&cfg)` / `trans_creds_view(&cfg)` 的多余借用；
- 拆分后不再需要的 import，以及测试里单独需要的 `text_op_punct`。

现在 `cargo clippy --all-targets -- -D warnings` **全绿**（只剩 vendor `ureq`
自身的 warning，不属于本仓库代码）。

## 四、验证

| 项目 | 结果 |
|---|---|
| `cargo test -- --test-threads=1` | 474 passed / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | 887 / 887 |
| `tools/test-pages.js` | 56 / 56 |
| `tools/check-all.js` | 五项全过 |
| `node tools/e2e/run.js` | **3/3 场景通过** |

---

**SHA256: B3CE88201E25BE6B119EB6C4C2B3CE0AB4A9ABD02D8AB8136E3C9B6D26155F73**
