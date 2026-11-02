# Quick Achieve v2.23.25

> 诊断探针原来是"会动用户数据"的；这版把它变成真正的只读，并让它失败时说得清楚。

## 一、诊断探针不是只读的：它会动你真实的备份

`--restore-probe` 的说明一直写着"配合 `QS_DATA_DIR` 指向数据副本，就能在**不碰真实配置**
的前提下验完整条路"。实测发现不是：

- 它会在**真实备份目录**里写一份 `config.before-restore(...)` 快照
  （`backup_dir` 来自配置，不是数据目录，所以 `QS_DATA_DIR` 管不到它）；
- 并触发轮转，**删掉最旧的一份真备份**。

一个诊断工具不该改变你磁盘上的任何东西。现在探针模式下恢复前快照、滚动备份、
备份轮转全部跳过 —— 实测「探针前 27 份 → 探针后 27 份」，一份不多一份不少。

## 二、探针失败时什么都不说

它只 `println!` 一行结果，而日志走异步队列 —— 紧接着的 `process::exit(0)`
让队列根本没机会落盘。于是探针失败时日志目录里一个字都没有。

现在会先同步写一行上下文：

```
restore-probe: name="config.auto(…).QuickAchieve" data_dir=… backup_dir=… exists=true
```

这一行当场就解释了本轮排查的困惑：在 `target\release\` 下跑探针时，
便携默认备份目录会跟着变成 `target\release\Quick Achieve back`（不是你以为的那个）。

## 三、顺带验证：恢复链路是通的

用部署位置的真实 exe + 临时数据目录跑完整条恢复：

```
OK=                        ← 恢复成功
临时目录 config.json 17,033 B（已被还原的配置覆盖）
```

说明 v2.23.20 加的那把云同步互斥锁**没有**挡住正常的恢复路径。

## 说明一处副作用

本轮排查中，第一次跑探针（还没加只读保护时）确实删掉了你备份目录里**最旧的一份**
`config.auto(2026-11-02 12-38-02).QuickAchieve`（其余 27 份都在）。你自己的配置没受影响，
那份备份内容也与相邻几份一致（都是同一天的 170 分完整度）。

## 安装

1. 下载 `QuickAchieve.exe`；
2. 覆盖你现有的那份（**数据目录 `Quick Achieve data\` 不受影响**）；
3. 双击运行。首次运行没有窗口是正常的 —— 在任务栏右下角托盘区找它。

> 从 v2.23.10 起，换上新 exe 后程序会**自己接管**（每 20 秒看一眼磁盘上的版本号，空闲时自动重启）。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 429 passed / 0 failed / 2 ignored |
| `node tools/test-ui.js` / `test-pages.js` | 827 / 56 全过 |
| `node tools/check-cmd-map.js` | 前端调用与后端命令表一致 |
| 探针只读性 | 备份目录 27 份 → 27 份（不变） |
| 恢复链路 | `--restore-probe` 返回 `OK=`，临时目录配置被正确还原 |

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: EFA89B651C45DEF06B166208B95293C4B8A0B4464FA42473B5D3C0301F5BEF66**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）
