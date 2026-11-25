# Quick Achieve v3.0.0

> 减法的版本。回退到 v2.23.129，把「液态玻璃」和「磨砂」两套横条外观**整体摘除**，
> 横条只剩一种外观：主题底色 + 横条不透明度。版本号跳到 3.0 表示这条线到此为止。

**SHA256: **

## 为什么是 3.0 而不是 2.23.132

2.23 这条线从 2.23.1 一路加到现在，光「横条长什么样」就攒了三套互斥的实现
（默认 / 磨砂 / 液态玻璃），每套都带着自己的配置字段、设置页分段、CSS 规则和
渲染链路。这次把它们砍掉是一次**能力回收**，不是又叠一层补丁 —— 用 3.0 划界，
免得后来人以为这还是一次普通的增量。

## 移除

不是关开关，是把代码删掉：**18 个文件，-2761 / +177 行**。

| 层 | 删了什么 |
|---|---|
| Rust | `src/glass.rs` 整删（600 行）；三个 IPC 命令 `glass_take` / `glass_capture` / `glass_exclude`；`layout_bar` 里的首屏抓屏与矩形记账；`qs://glass` 事件；11 个配置字段（`bar_glass*` 9 个 + `bar_blur` + `bar_bg_alpha`）以及它们的默认值、归一、备份灌回 |
| 渲染 | `ui/glass.js` 整删（1039 行 WebGL2 渲染器：liquidGL 透镜着色器 + 后期那套 dual-filter 模糊链）；`index.html` 的 `<canvas id="qsGlass">` 与脚本引用 |
| 样式 | `bar.css` 的 `--qs-blur` 一族 + `[data-blur]` 的 `backdrop-filter`；静态雾面 `::before`（乳白薄纱 + feTurbulence 颗粒）；整段 `.qs-glass` 覆盖规则；`--qs-bar-fill` / `--qs-frost-veil` 两条磨砂专用变量 |
| 设置页 | 「横条样式」三分段、「玻璃预设」两档、6 颗玻璃滑块、磨砂强度、底板透明度、实时折射、刷新帧率档位；`settings.js` 里对应的推导（`qsBarStyle`）、绑定、预览两套画法 |
| 文案 | 帮助与 FAQ 里讲「样式三选一 / WebGL2 抓屏隐身」的段落改写；`ui/shared.js` 的 `qsBlurPx` / `qsBlurTone` 删除 |

### 有意保留的两处

- **抓屏原语**：自动明暗判定（`autotheme`）要抓一帧桌面判深浅，那是功能不是外观。
  `Snap` / `capture_rect` / `virtual_screen` 挪进了新的 `src/screencap.rs`，其余全删。
- **`i18n.en.js` 里的旧译文键**：查不到就是死数据，不影响运行；成批删有引号匹配风险。

顺手把 `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`（让横条对抓屏隐身）也去掉了 ——
那是"持续抓自己背后的桌面"的配套手段，现在没这需求了。**副作用是好事**：截图和录屏里
横条不再凭空消失。

## 兼容

- **老 `config.json`**：那 11 个字段变成未知字段，serde 默认忽略（没有 `deny_unknown_fields`）
  → 启动即默认外观。**不用手动清配置**。
- **老备份恢复**：`settings_apply` 不再读这些键，灌进去也会被忽略。
- **自动明暗、收藏夹、OCR、翻译、网格页**：一行没动。

## 验证

```
cargo test --release                     → 511 passed / 0 failed / 2 ignored
前端自检（check-all.js，5 项全过）：
  UI 金丝雀（jsdom）        874 / 874 通过
  页面冒烟                  56 / 56 通过
  命令表一致性（invoke ↔ Rust）  OK
  英文覆盖                  残留 4 条（允许 5）
  设置项一致性（滑块↔HTML↔配置） 问题 0 条
```

### 测试也跟着改了

删功能就得改断言，否则门槛拦的是"你还删不删"而不是"删干净没有"：

- `tools/test-ui.js` 里原来钉液态玻璃实现细节的十几条断言（画布铺满窗口、liquidGL
  shader 数学、Rust 抓屏层、bar.js 五个钩子）**反过来钉「不许再出现」** ——
  文件不存在、样式里没有 `qs-glass` / `data-blur`、`bar.js` 不碰 `QSGlass`。
  防的是有人把它们偷偷加回来。
- 三处 `runScripts(... 'ui/glass.js')` 去掉（文件没了，加载就抛异常，
  而且会把后面所有断言静默跳过 —— 这个坑很隐蔽，只报一条"测试过程中不应抛异常"）。
- 「恢复本节默认」那条测试原来拿磨砂滑块当样本，改用同一张卡里的「横条内边距」。
- 判定要**先去注释再匹配**：bar.js / index.html 里那些"原本调 `QSGlass.xxx`"的
  删除说明本身就带着这些词，不去注释会自己把自己判红。

## 找回来

```bash
git checkout 2.23.129 -- src/glass.rs ui/glass.js
```

再按上文的反向补回配置字段与 UI。dual-filter 那套磨砂方案（当时的 2.23.130）
没有保留提交 —— 它只存在于工作区，随回退一起丢了；思路记在
`docs/archive/磨砂-dual-filter-方案.md`。
