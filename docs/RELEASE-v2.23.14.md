# Quick Achieve v2.23.14

> v2.23.13 的兜底：命令内部出错时，界面不会再卡在"进行中"。

## 问题

v2.23.13 把 async 命令挪到了工作线程 —— 翻译、「重新识别」期间界面不再冻结。
但工作线程里如果 panic 了，前端那个 Promise 会**永远挂着**：按钮一直是"进行中"，
用户除了重启程序没有别的办法。

## 修法

用 `catch_unwind` 接住工作线程里的 panic，回一条明确的错误让界面自己收场：
**命令失败归失败，界面不能卡死**。

## 顺带

`main.rs` 里那条自检 `sync_commands_must_not_block_the_main_thread` 以前只 grep
`thread::sleep` / `open::that(`，还把 async 命令整个跳过（理由写的是"async 命令不占主线程"——
在自建运行时里这句本来是错的）。现在 async 命令真的不在主线程跑了，那条理由才成立。

## 安装

1. 下载 `QuickAchieve.exe`；
2. 覆盖你现有的那份（**数据目录 `Quick Achieve data\` 不受影响**）；
3. 双击运行。首次运行没有窗口是正常的 —— 在任务栏右下角托盘区找它。

> 从 v2.23.10 起，换上新 exe 后程序会**自己接管**（每 20 秒看一眼磁盘上的版本号，空闲时自动重启）。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 429 passed / 0 failed / 2 ignored |
| 真机启动 | 无 PANIC，托盘 / 钩子正常就绪 |

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）
