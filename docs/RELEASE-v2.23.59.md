# Quick Achieve v2.23.59

> 核查结论：**确实有一处在碰用户的复制**（Ctrl+C），已撤销。

## 查到了什么

`src/hook.rs` 的键盘低级钩子里：

```rust
if info.vkCode == 0x43 && win::ctrl_down() && info.dwExtraInfo != win::SELF_INJECT_TAG {
    clip::capture_precopy();   // ← 在钩子回调里 OpenClipboard
}
```

它**不吞按键**，但会在按键进入目标程序之前**打开剪贴板**抢一份旧内容。`OpenClipboard` 是跨进程同步操作：微信 / Office / 剪贴板工具正握着剪贴板时它要等；低级钩子跑在系统输入关键路径上，等久了轻则这次 Ctrl+C 变卡，重则撞 300ms 的 `LowLevelHooksTimeout`——钩子被系统摘掉，目标程序那次复制也可能没写成功。表现就是「按了 Ctrl+C 复制不上」。

**已整段删除。** 钩子回调现在只剩纯读的 `GetAsyncKeyState` / `GetClipboardSequenceNumber`（后者只读序号、不开剪贴板）。

## 复制类快捷键一律不抢

| 组合 | 处理 |
|---|---|
| `Ctrl+C` | 永不抢 |
| `Ctrl+Shift+C`（终端复制） | 永不抢 |
| `Ctrl+Insert`（老式复制） | 永不抢 |

以前只挡 `Ctrl+C` 且要求"没按 Shift"，所以把 `ctrl+shift+c` 配成热键就会吞掉终端复制。

## 护栏

新增单测 `钩子回调里不许碰剪贴板`：读 `hook.rs` 源码、去掉注释后断言不出现 `capture_precopy(` / `OpenClipboard` / `clip::set_text`。以后谁再往钩子里加，测试当场红。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | **454** passed |
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