# Quick Achieve v2.23.63

> 通知太频繁 —— 两处都收紧了。

## 1. 「不支持划词」提示每划一次冒一条

v2.23.60 的小提示条是"每 4 秒一条"，去重键还是 **pid**。微信 4.x 是"主进程 + 一堆 WeChatAppEx 子进程"，每次划词落在哪个 pid 不固定——按 pid 去重等于**每个子进程各提示一次**。

改法：

- 去重键换成**可执行文件名**（`weixin.exe` 只算一个软件）；
- 小提示条 + 托盘气泡都改成**每个软件每会话只发一次**。

## 2. 「已使用：xxx」换功能就弹

原来节流是"同一功能 60 秒一次，换功能立刻发"。连着用几项功能（或换搜索引擎）时，右下角一条接一条。

改法：新增**全局间隔 20 秒**（`NOTIFY_GAP_GLOBAL`）——不管换不换功能，任意两条提示之间至少隔 20 秒；同一功能仍 60 秒。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test -- --test-threads=1` | **455** passed |
| `cargo clippy --all-targets -D warnings` | **0 告警** |
| `node tools/check-all.js` | UI 883 / 页面 56 / 命令表 / 英文覆盖 —— 全过 |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 托盘图标右键 → 退出；
3. 覆盖 → 双击运行。

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）