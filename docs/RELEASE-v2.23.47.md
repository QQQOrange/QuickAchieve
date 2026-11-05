# Quick Achieve v2.23.47

> 本轮新代码逐个审了一遍，修掉 **3 个真 bug**；`main.rs` 拆分继续推进（备份那块尝试失败，已回退，不留半成品）。

## 修掉的三个 bug

### 1. 同一毫秒连点两次收藏 → 两条收藏同 id

收藏 id 用的是毫秒时间戳。同一毫秒内连续收藏（连点、脚本调用）会撞 id，
之后按 id 找条目会「点第二条动了第一条」。

现在 id 带自增序号：`q<毫秒>_<序号>`，并加单测钉住。

### 2. 关掉剪贴板历史后，磁盘上的历史文件不删

原来写的是 `if c.persist { remove_file }` —— 用户把「启用」和「写入磁盘」一起关掉时，
条件不成立，`cliphist-data.json` 永远留在磁盘上。

**用户以为关了就没有，其实还在** —— 这是隐私问题。现在只要「功能关着」或
「选了不落盘」就删文件，并加单测。

### 3. 本地答案的日期差在凌晨会差一天

「今天」原来用 `UNIX 秒 / 86400`（UTC）。在 UTC+8 的 00:00–08:00 之间算出来的还是昨天 ——
`2026-11-05 → 还有 1 天` 会在凌晨错报成 2 天。改用 `GetLocalTime()` 取本地日期。

## 拆分进度

| 模块 | 状态 |
|---|---|
| `src/diag.rs`（诊断） | ✅ 已拆（v2.23.46） |
| `src/favquick.rs`（收藏逻辑） | ✅ 已拆（v2.23.46） |
| `src/cliphist.rs` / `src/localanswer.rs` | ✅ 本来就是独立模块 |
| 备份工具 | ⏳ 今天尝试搬 `backup_diff`，`generate_handler!` 注册被打乱，**已回退** |
| 窗口 / 钩子 | ⏳ 耦合最深，未动 |

**失败原因记下来了**：命令宏会在**定义所在的模块**生成 `__qs_reg_<name>` 符号，
而 `generate_handler!` 在 main.rs 里按名字引 —— 搬到别的模块后必须把这两个符号
一起 `pub use` 回来，否则报 `cannot find function __qs_reg_backup_diff`。
下次按这个思路重试，不再瞎试。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | **453** passed（+2：同毫秒 id 唯一、关掉即删盘） |
| `cargo clippy -- -D warnings` | **0 警告** |
| `node tools/check-all.js` | UI 858 / 页面 56 / 命令表一致 / 英文残留 3 —— 全过 |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 托盘图标右键 → 退出；
3. 覆盖 → 双击运行。

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）
