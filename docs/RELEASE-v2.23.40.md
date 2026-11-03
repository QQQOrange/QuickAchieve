# Quick Achieve v2.23.40

> 继续按"逐条走真实流程"查，这轮补的是**两条完全静默的链路**——出了问题连"到底点没点"都分不清。

## 一、收藏夹：点了之后没有任何记录

整份日志里 **0 条 `fav_` 记录**，没法判断是"这轮没用过收藏夹"还是"用了但没生效"。而 `fav_send` 原来只在"注入失败"时写日志，正常走"复制到剪贴板"那条路一个字都没有。

现在补齐：

```
fav_send: id=xxxx                                ← 入口（点了）
fav_send: 已注入 N 字到 hwnd=…「标题」              ← 注入成功（原有）
fav_send: 前台仍是我们自己（前台锁定）→ 退回复制      ← 失败分支（原有）
fav_copy: id=xxxx kind=text html=是 结果=已复制    ← 新增：复制结果
fav_copy: id=xxxx kind=image 12345 字节 结果=已复制 ← 新增：图片分支
```

一条链路"点了 → 走了哪条路 → 成没成"现在全能从日志读出来。

## 二、翻译：整条链路一个字都不记

`trans_run` 原来完全静默。现在对齐 OCR 那套口径，**只记字数与耗时，不记原文**：

```
trans_run: siliconflow → 简体中文 成功，输入 25 字 / 输出 31 字，耗时 840ms
trans_run: siliconflow → 简体中文 失败（320ms）：401 未授权……
trans_run: 不认识的引擎 xxx
```

## 三、顺带查清"云端备份 361KB 是怎么回事"

日志里有一条 `已备份并上传：config(…).QuickAchieve（361.3 KB）`，而本地 `config.json` 只有 17KB。查下来是**设计如此、不是膨胀**：

- 备份载荷里有 `overrideIcons` / `dirIcons` / `favorites`，收藏夹条目里的图片是**内嵌 dataURL**（源码注释写明"收藏夹跟着 config.json 一起备份，所以图片必须内嵌"）；
- 本地那份 17KB 是**加密后**的大小，两者不可直接比较；
- 同步前有"实质内容没变就跳过上传"的去重，不会白传。

结论：**不动**。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 431 passed / 0 failed / 2 ignored |
| `node tools/test-ui.js` / `test-pages.js` | 823 / 56 全过 |
| `node tools/check-cmd-map.js` | 一致 |
| 真机构建 | 通过 |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 托盘图标右键 → 退出；
3. 覆盖 → 双击运行。

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）
