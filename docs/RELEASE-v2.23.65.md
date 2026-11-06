# Quick Achieve v2.23.65

> 修：**双击 / 三击选词不弹条**（抖音网页评论区就是这种场景）。

## 根因

不是"没检测到"，而是双击被当成点击扔掉了：

- `Ev::Up` 里已经有双击判定（`is_double`），也会去 `schedule_capture`；
- 但 `start_capture` 里还有一道 `if !dragged { return; }` —— 双击**没有拖动过程**（起点终点同一个点），`dragged == false`，于是**连复制都不试**就返回。

抖音评论区是短句，大家习惯**双击选一个词、三击选一行**，正好全踩在这条上。

## 修法

把双击标记一路带进取词链路：`schedule_capture(..., dbl)` → `start_capture(..., dbl)`，
闸门改成 `if !dragged && !dbl { return; }`。双击/三击照样去试"复制取词"，复制不到就什么都不弹（不会误弹）。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test -- --test-threads=1` | **455** passed |
| `cargo clippy --all-targets -D warnings` | **0 告警** |
| `node tools/check-all.js` | 页面 56 / 命令表 / 英文覆盖 —— 全过 |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 托盘图标右键 → 退出；
3. 覆盖 → 双击运行。

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: DA56C02DF15D8ADE03A3C352DD2DF1982635CD7A293AF5CFA77762743C4028B3**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）