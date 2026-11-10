# Quick Achieve v2.23.96

> 把「智能换行」这一簇也搬进 `text_ops.rs`，文本处理模块到此完整；
> `main.rs` 再少约 100 行。

## 一、拆了什么

从 `main.rs` 移出：

| 函数 | 作用 |
|---|---|
| `unwrap_lines` | 智能换行：把被空格/换行断开的句子重新接起来 |
| `ends_sentence` | 判断一行是不是句子结尾（句号/引号/省略号…） |
| `ends_punct` | 标点辅助 |
| `is_structural_line` | 列表、表格、标题等"不能接"的结构行 |
| `needs_space_between` | 拼接时英文单词之间要不要补空格 |

现在 `src/text_ops.rs` 包含：

- 包裹符号表 + 包/拆/换对；
- 智能去空格、标点全半角、标题大小写、句首大写；
- 智能换行（本轮补齐）。

留在 main 的只剩命令外壳、剪贴板备份/还原、回写目标软件。

## 二、行数

| 文件 | 之前 | 现在 |
|---|---|---|
| `src/main.rs` | 25,591 | **25,490** |
| `src/text_ops.rs` | 243 | 342 |

`main.rs` 从 26,107 行起，四刀累计 **-617 行**。

## 三、验证

| 项目 | 结果 |
|---|---|
| `cargo test -- --test-threads=1` | 474 passed / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | 887 / 887 |
| `tools/test-pages.js` | 56 / 56 |
| `tools/check-all.js` | 五项全过 |
| `node tools/e2e/run.js` | **3/3 场景通过**（移远单独复跑 5/5） |

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**
