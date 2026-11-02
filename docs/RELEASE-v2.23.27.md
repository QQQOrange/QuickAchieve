# Quick Achieve v2.23.27

> 两条更新探针一跑就 panic（一直是死的）；顺带核实了整条更新链的关键逻辑。

## 一、`--update-probe` / `--update-install` 一跑就崩

它们会先调 `update::check`，而它要读配置里的 GitHub 令牌（`cfg_with`）——
**那时全局状态还没初始化**，`cfg_with` 直接 `expect("state not ready")` 崩掉。

所以这两条诊断路径一直是死的：发布流程里谁也跑不起来，只是没人注意（它们只给脚本用）。
现在探针开头补 `init_state()`，并打上 `PROBE` 标记（不碰你的备份）。

修完实测：

```
NEWER=false
TAG=v2.23.26
CURRENT=2.23.26
```

## 二、顺带核实了更新链的几处关键逻辑（逐行读 + 实跑）

| 环节 | 结论 |
|---|---|
| 版本比较 | 按数字分段比 `(u64,u64,u64)`，**不会**把 `2.23.9` 判成比 `2.23.23` 新 |
| 下载失败 | 区分"可续传"与"丢弃断点"；内容校验不过会删断点，不会拼出一个永远错的文件 |
| 替换 exe | 先给自己改名 → 再搬新的进来；第二步失败**会回滚**，回滚再失败会明确告诉用户手工改名自救，不会留下"没有 exe"的状态 |
| 匿名 vs 令牌 | 没填令牌只提示"去网页下载"，不假装能自动更新；填了才有 5000 次/小时额度 |
| 与自动接管的关系 | 更新进行中 `binwatch` 会让路（v2.23.16 加的），不会两边各起一个进程 |

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
| `--update-probe` | 正常输出 `NEWER/TAG/CURRENT`（修前是 panic） |
| 真机启动 | 无 PANIC，托盘 / 钩子正常就绪 |

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: EBF2808434AFF1946A14A1B0DA318A930BAAEC5C1C42A919B88B5414FF2F8C41**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）
