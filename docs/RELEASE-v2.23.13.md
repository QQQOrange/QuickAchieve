# Quick Achieve v2.23.13

> 翻译和「重新识别」期间，界面不再冻结。

## 问题

注册命令的时候，`async` 这个信息**在注册那一刻就丢了** —— 运行时只知道"有这么个命令"。
于是 13 个 async 命令照样在主线程上 `block_on(未来)` 跑完：

`trans_run`（翻译）、`trans_test`（测试连接）、`ocr_rerun` / `ocr_rescan`（重新识别）、
`icon_remove_bg`（抠图）、`update_install`（更新）以及各个 `open_*`（打开帮助页）。

后果：

- 点「重新识别」或翻一段长文，**整个界面冻结 25~90 秒**，连"正在识别…"都刷不出来，
  看着像程序死了；
- 翻译的流式增量（`qs://trans-chunk`）全堵在事件队列里，**翻完才一起涌出** ——
  "第一个字几百毫秒就出来"的设计等于没有。

## 修法

宏在注册命令时多带一个 `is_async` 字段，`run_ipc` 看到 async 就把命令丢到工作线程，
主线程立刻返回；结果通过 `eval_in` 投递回来（那条路本来就能跨线程调用）。

顺带的好处：主线程空出来之后，界面照常重绘、按钮照常响应，翻译的流式输出也恢复成
"一段一段地冒"。

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

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）
