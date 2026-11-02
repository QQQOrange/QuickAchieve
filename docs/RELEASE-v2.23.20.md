# Quick Achieve v2.23.20

> 一条并发保护：**整份恢复 / 明文导入，要和云同步互斥**。

## 问题

`SyncGuard` 一直只保护云操作（上传 / 下载 / 删除 / 从云端恢复）。
而「恢复备份目录里的某一份」和「导入明文配置」同样是**整份写 `config.json`**，
却不受它管 —— 如果正在自动上传时点了恢复，两边会各写一遍：

- 轻则恢复到一半被那边的 `save` 冲掉；
- 重则刚恢复的配置被正在上传的那一份覆盖回去。

而且这两种操作都是**破坏性**的（整份覆盖），撞上时的损失比普通设置更重。

## 修法

这两条路现在也抢同一把锁，抢不到就明确回一句：

> 正在和云端同步，稍等一下再恢复 / 导入

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
| 真机启动 | 无 PANIC，托盘 / 钩子正常就绪 |

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: F94EE471CC306C57A4E5E5111B0FE84BE96C31CC2DE48ACC59C189452D5AEBAD**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）
