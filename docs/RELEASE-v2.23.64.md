# Quick Achieve v2.23.64

> 关掉所有"打扰型"提示。

## 1. 「不支持划词弹条」—— 彻底不弹

`hint_no_uia_once()` 现在只往 `log.txt` 写一行结论，**不发小提示条、不发托盘气泡**。排查时翻日志能看到"这个 pid/exe 读不到选区"，但不打扰你。

## 2. 「已使用：xxx」小提示条 —— 默认关，老配置也关

- `notify_feature` 默认改成 **false**；
- 光改默认值只影响新装用户，所以加了一次性迁移 `notify_quiet_migrated`：老配置读进来时若还开着，自动关掉并写回（日志记一行 `迁移 —— 关闭「已使用」提示`）。

设置页那两个开关都还在（通用 → 提示），想开回来随时可以。

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

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）