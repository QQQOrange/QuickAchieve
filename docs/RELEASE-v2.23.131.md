# Quick Achieve v2.23.131

> 回退到 v2.23.129，并把「液态玻璃」和「磨砂」两套横条外观**整体摘除**。
> 横条只剩一种外观：主题底色 + 横条不透明度。

## 原因

用户原话：「回退到昨天的版本，同时删除液态玻璃和磨砂，只保持默认的外观」。

两套外观各自都带来过麻烦：液态玻璃要抓屏 + WebGL2 + 对抓屏隐身（截图录屏里横条
会消失），磨砂要么是 `backdrop-filter`（透明悬浮窗里取样的是空的，纯烧 GPU）要么是
一层静态乳白雾面（看着像塑料）。用户最终要的是一个不折腾的横条。

先 `git checkout` 把上一轮未提交的磨砂改造（v2.23.130 的 dual-filter 方案）全部丢弃，
工作树回到 `f2ccab5`（v2.23.129），再在这个干净版本上删代码。

## 删除

不是关开关，是把代码删掉。

| 文件 | 改动 |
|---|---|
| `src/glass.rs` | **整个删除**（600 行）。抓屏原语（`Snap` / `capture_rect` / `virtual_screen`）挪进新的 `src/screencap.rs` |
| `src/screencap.rs` | **新增**。自动明暗判定（`autotheme`）只借用抓屏，跟外观无关 —— 那是功能，留着 |
| `ui/glass.js` | **整个删除**（1039 行 WebGL2 渲染器：liquidGL 透镜着色器 + dual-filter 模糊链） |
| `src/config.rs` | 删 11 个字段：`bar_glass`、`bar_glass_refraction`、`bar_glass_bevel`、`bar_glass_frost`、`bar_glass_aberration`、`bar_glass_spec`、`bar_glass_tint`、`bar_glass_live`、`bar_glass_fps`、`bar_blur`、`bar_bg_alpha`（连同 `default_bar_bg_alpha()` 和 sanitize 分支） |
| `src/main.rs` | 删三个 IPC 命令 `glass_take` / `glass_capture` / `glass_exclude` 及注册；删 `layout_bar` 里首屏抓屏块和「窗口矩形变了就记账」那段；删 `qs://glass` 事件 |
| `src/settings_view.rs` | 删 11 个下发键 |
| `src/settings_apply.rs` | 删 11 个白名单分支 + 备份恢复时的 11 行灌回 |
| `src/bar_lifecycle.rs` | 删 `dismiss_bar` 里解除抓屏隐身的兜底调用 |
| `ui/index.html` | 删 `<canvas id="qsGlass">` 和 `<script src="glass.js">` |
| `ui/bar.js` | 删 9 处 `QSGlass.*` 调用、整个 `glassRefit()`、`syncBarBg` 里的磨砂分支、`applyLook` 里的 `data-blur` 写入、`lastBlurKey` |
| `ui/bar.css` | 删 `--qs-blur` 一族 + `[data-blur]` 的 backdrop-filter 规则、静态雾面 `::before` 段、整段 `.qs-glass` 覆盖规则；横条底板回到 `var(--qs-bg-fill, var(--qs-bg))` |
| `ui/settings.html` | 删「横条样式」三分段、「玻璃预设」、6 颗玻璃滑块、磨砂强度、底板透明度、实时折射、刷新帧率；帮助与 FAQ 里讲这三样的段落改写 |
| `ui/settings.js` | 删 `qsBarStyle` / `syncStyleUI` / `GLASS_PRESETS` / `bindGlassPreset` / `bindBarStyle` / `GLASS_FPS_CHOICES` / `bindGlassFps` / `syncGlassFpsUI`、SLIDERS 里 9 条、`PREVIEW_KEYS` / `SLIDER_LABEL` 的对应条目、设置页预览的两套画法、监听分支、boot 里的三个绑定 |
| `ui/settings.css` | 删 `.qs-preview[data-blur]` 和 `.qs-preview[data-lg]` 两段预览样式（含深色变体） |
| `ui/shared.js` | 删 `qsBlurPx` / `qsBlurTone`（滑块值 → 模糊半径的唯一换算口径） |
| `ui/settings-card-reset.js` | 删 `styleDefault = { barBlur: 12 }` 及其分支 |

**合计 18 个文件，-2761 / +177 行，删掉两个源码文件。**

### 有意没删的两处

- `ui/i18n.en.js` 里那些英文译文键（"磨砂强度"、"玻璃预设"…）保留。查不到就是死数据，
  不影响运行；一次删三十多条有引号匹配风险，不值得。
- `docs/*.md` 里历史提及这些功能的段落不动 —— 那是当时的记录。

## 行为边界

- **老 `config.json`**：那 11 个字段变成未知字段，serde 默认忽略（没有
  `deny_unknown_fields`）→ 启动即默认外观。不需要手动清配置。
- **老备份恢复**：`settings_apply` 不再读这些键，灌进去也会被忽略，结果同样是默认外观。
- **截图 / 录屏**：横条不再调 `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`，
  现在会被正常拍到。这是删掉实时抓屏的必然结果，也是好事。
- **自动明暗**：照常工作。它抓的是「横条将要出现的那个位置」，抓完就扔，不参与渲染。
- **设置页**：「横条外观」只剩图标大小、内边距、不透明度、待机淡度、圆角、投影等，
  没有任何样式开关。

## 验证

```
cargo test --release --bin QuickAchieve   → 511 passed; 0 failed; 2 ignored
cargo build --release                     → Finished，前端内嵌 429 个文件（原 430）
node --check bar.js settings.js shared.js settings-card-reset.js
             bar-panel-lazy.js i18n.js i18n.en.js   → 全部 OK
CSS 括号平衡                                → bar.css 364/364，settings.css 744/744
DOM id 引用完整性                           → bar.js 26 引用 / 26 命中，缺失 0
                                              settings.js 缺失 3 个（goName/goNote/goTarget），
                                              那是 goField() 运行时动态创建的，与本次无关
```

`cargo test` 从 513 变 511：删掉 2 个 glass 单测（`recapture_last_survives_take` /
`recapture_follows_new_rect`）和 1 个 `bar_bg_alpha` 测试，新增 3 个 `screencap`
测试，另有一处备份往返测试的字段替换，净 -2。

## 想找回来

```bash
git checkout f2ccab5 -- src/glass.rs ui/glass.js
```

再按本文件的反向补回 config 字段与 UI 即可。上一轮那套 dual-filter 磨砂方案
（v2.23.130）**没有保留提交** —— 它当时只存在于工作区，随这次回退一起丢了。
要重做的话，思路见 `docs/archive/磨砂-dual-filter-方案.md`。
