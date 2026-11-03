# Quick Achieve v2.23.38

> 给**极端数据**上的一组回归测试（这版不改产品行为）：用户能造出来的怪输入会不会把设置页画崩、会不会把文字当 HTML 解释。

## 新增 4 条用例

| 输入 | 断言 |
|---|---|
| 超长引擎名（160 字）+ 特殊字符 `<b>粗</b> & "引号" '单引号' 😀` | 名字**原样**出现在 DOM 里；特殊字符**没有被当成 HTML**（不许冒出 `<b>` 标签） |
| 全空配置（横条空 / 无自定义引擎 / 无外部软件 / 无收藏） | 没有**幽灵引擎条目** —— 只剩 OCR / 翻译 / 万能框选三条"快捷添加"行 |
| 横条上放不存在的引擎 + 悬空哨兵（`@fav:missing-id` / `@app:nope` / `@bogus`） | 列表照常渲染，不因为解析不到就整页炸 |
| 一百二十个收藏目录 | **没测**，并且在代码注释里写清原因：收藏树在 `bindSwitches()` 里重建，而 `favTree` / `favRender` 都在模块作用域内，这套 jsdom harness 拿不到 —— 硬凑断言只会变成假通过 |

## 顺带核实（未改代码）

- **WebDAV 上传是单次 PUT**：断流理论上可能留半截文件，但恢复时会先解析、读不出来就跳过该候选（`readable` 过滤），所以**不会拿一份坏备份覆盖本机配置** —— 属于"已经能自愈"，不动它。
- **`innerHTML` 注入面**：`bar.js` 里 46 处 `innerHTML` **没有一处拼用户数据**；`settings.js` 里唯一拼变量的那处拼的是 `missing.length`（数字）。

## 验证

| 项目 | 结果 |
|---|---|
| `node tools/test-ui.js` | **823 / 823**（新增 3 条） |
| `cargo test` | 431 passed / 0 failed / 2 ignored |
| `node tools/test-pages.js` | 56 / 56 |
| `node tools/check-cmd-map.js` | 一致 |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 托盘图标右键 → 退出；
3. 覆盖 → 双击运行。

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）
