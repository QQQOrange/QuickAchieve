# Quick Achieve v2.23.54

> 删掉「Ctrl+C 唤起横条」这条逻辑——正常复制不该被打扰。

## 之前为什么会弹

`main.rs` 的键盘钩子分支里有一段：

```rust
if !ours && ctrl && !alt && vk == VK_C && cfg_with(|c| c.trigger_copy) {
    copy_trigger(seq0);   // → 取词 → show_bar
}
```

而 `trigger_copy` 的默认值是 **true**，所以装完就默认「复制一下就弹横条」。这条已**整段删除**。

## 配套改动

| 项 | 改动 |
|---|---|
| `trigger_copy` 配置项 | 默认改 **false**，字段保留只为兼容老配置（备份能来回倒）；没有任何代码再读它来唤起 |
| 设置页 | 删掉「复制时弹横条（Ctrl+C）」那一行（开关已无意义，留着误导） |
| 帮助文案 | 「按 Ctrl+C 也会弹」→「Ctrl+C 不再唤起，用自定义快捷键 / 托盘」 |
| 诊断 | 不再输出「Ctrl+C 触发」 |

## 取词还剩哪两条路

1. **鼠标拖选**（全局钩子，选中即弹 / 按住 Ctrl 才弹，按你的设置）；
2. **自定义快捷键 / 托盘菜单 / 收藏夹**。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | **453** passed |
| `cargo clippy -- -D warnings` | **0 警告** |
| `node tools/check-all.js` | UI 880 / 页面 56 / 命令表一致 / 英文残留 3 —— 全过 |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 托盘图标右键 → 退出；
3. 覆盖 → 双击运行。

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 4AE3EE1FFD6FB3536FC8E6ACD40DC2D447C550192CA8CAE9F3B365820F63CA70**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）