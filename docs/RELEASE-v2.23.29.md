# Quick Achieve v2.23.29

> 这一版主要是**回归散步**：把这一路改过的地方用真实数据跑一遍，确认没有互相踩。

## 一、`QUIET_PANIC` 改成 RAII 复位

上一版加的"探测 ONNX 时不写 panic 日志"用的是裸的 `store(true) … store(false)`。
万一探测期间发生意外导致漏掉复位，这个标志会卡在 `true` ——
**之后所有 panic 都不再写日志**，等于把最要紧的诊断能力悄悄关掉。
现在用 RAII：不管怎么退出，Drop 一定复原。

## 二、回归验证（真实数据）

| 检查 | 结果 |
|---|---|
| 用**真实配置**（27 份备份 + 自定义引擎 + 收藏夹）启动 | 14 行日志、**0 错误 0 警告**；`engines=770`、`hotkey: 独占 1 个组合`、`uia 预热 25ms` |
| 全新数据目录启动 | `PANIC` 行数 **0**，首启引导正常 |
| `--stress 200`（反复 show/hide） | 端到端 25~47ms、内存/句柄无增长趋势、0 JS 报错 |
| `--restore-probe`（真实备份 + 临时数据目录） | `OK=`，配置正确还原，且**不再改动真实备份目录**（27→27） |
| `--update-probe` | 正常输出 `NEWER/TAG/CURRENT` |
| 滑块拖动会不会造成广播风暴 | 滑块是松手（`change`）才保存，不是每像素一条 IPC |
| `close()` 派发事件 + 调用点显式清理 | `clear_ocr_session` 幂等，不会重复扣数据 |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 覆盖你现有的那份（**数据目录 `Quick Achieve data\` 不受影响**）；
3. 双击运行。首次运行没有窗口是正常的 —— 会弹托盘气泡并打开设置页。

> 从 v2.23.10 起，换上新 exe 后程序会**自己接管**（每 20 秒看一眼磁盘上的版本号，空闲时自动重启）。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 429 passed / 0 failed / 2 ignored |
| `node tools/test-ui.js` / `test-pages.js` | 827 / 56 全过 |
| 真机启动（真实配置 / 全新目录） | 均无 PANIC、无错误 |
| 发布流水线 | SHA256、push、tag、Release、资产回读校验全过 |

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）
