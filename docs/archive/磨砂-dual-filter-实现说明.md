# 横条磨砂改造实现说明（v2.23.130）

> 目标：把 QuickAchieve 的横条磨砂做成 liquidDX11 那种质感 —— **本身是磨砂的，
> 但能实时反馈条下面的背景变化**。本次按 P0→P3 全部落地，P4 明确不做（理由见 §6）。

---

## 1. 一句话总结

**从"一个可调的模糊半径"改成"两档模糊按系数混"。**

| | 改之前 | 改之后 |
|---|---|---|
| 磨砂执行者 | CSS 静态雾面（不抓屏、不跑 WebGL） | WebGL2 + Rust 抓屏，实时 |
| 模糊算法 | 无（只有一层乳白 + 噪点贴图） | dual-filter：4 级 down（盒式）+ up（tent） |
| 强度控制 | 雾面浓度 | `lerp(soft, heavy, mix)` + 降采样级数 |
| 能不能看见背景在动 | 不能（静态） | 能（soft 档保住了"形"） |

---

## 2. 三条根因，逐条对应到改动

### ① 抓屏矩形 = 窗口矩形 → 模糊的"厚度"被物理卡死

`src/glass.rs` 新增 `MARGIN = 96`（物理像素），`tex_rect_for()` 把窗口矩形
向外扩一圈再钳进虚拟屏幕：

```rust
let tx = (x - m).max(vx);
let ty = (y - m).max(vy);
let tx2 = (x + w + m).min(vx + vw);
let ty2 = (y + h + m).min(vy + vh);
```

横条 1200×40 → 纹理 1392×232。**矮边从 40 变 232**，第 6 级还剩 3.6px，
模糊才真的做得出来。

越界时**收缩单边**而不是整体放弃 —— 横条贴屏幕顶边是常态
（`pick_bar_side` 就爱把它摆在光标上方），整体放弃等于磨砂在屏幕边缘失效。

代价：面积约 2.7 倍，IPC 字节同比。用 `MAX_TEX_PIXELS = 8_000_000`
兜住 4K 超宽屏的极端值（超了 margin 减半）。

### ② 单档半径 vs 双档插值 → 结构上做不到"磨砂但有形"

`ui/glass.js` 新增两条 GLSL pass（逐行抄 liquidDX11 的 `kBlurHLSL`）：

| pass | 采样 | 权重 | 出处 |
|---|---|---|---|
| `downSample` | 4 角 + 中心×4 | /8 | `shaders.h:27-36` |
| `upSample` | 8-tap tent（边×1、角×2） | /12 | `shaders.h:38-50` |

链的拓扑：

```
tex ─down→ D0 ─down→ D1 ─down→ … ─down→ D(n-1)
            │                              │
            │  soft（轻磨砂，形还在）        up ×(n-1)
            │                              ↓
            └────────────────────────→ heavy（厚磨砂）
```

**soft 不额外做一遍**：`D0` 本身就是"原图 1/2 降采样 + 4tap 加权"，
liquidDX11 的 `soft_` 也是这么来的 → 零额外 pass。

着色器里：

```glsl
float mixv = clamp(u_blurMix + edge*0.18, 0.0, 1.0);
refrCol = vec4(mix(texture(u_soft, uv).rgb,
                   texture(u_heavy, uv).rgb, mixv), 1.0);
```

`+ edge*0.18` 照抄 `sampleBackdropFrost` 的思路（`shaders.h:174-179`）：
边缘更磨、中心更透。

### ③ 级数按纹理尺寸算，不是死抄 6 级

```js
function levelCountFor(w, h) {
  var minSide = Math.max(MIN_COARSE_PX, Math.min(w, h));
  var n = Math.floor(Math.log(minSide / MIN_COARSE_PX) / Math.LN2);
  return Math.max(2, Math.min(MAX_LEVELS, n + blurExtra));
}
```

`MIN_COARSE_PX = 24`：最粗一级的矮边低于 24px，竖直方向的信息已经被压没，
上采样回来只是一块"没有层次的横条纹"，不是磨砂。

1392×232 → `floor(log2(232/24)) = 3` 级，最粗 174×29 ✓

---

## 3. 磨砂强度 → 两个旋钮

滑块「磨砂强度」量程 0..32（`settings.html:596`），映射：

```
blurT   = barBlur / 32
blurMix = min(1, blurT / 0.72)          ← 前段：形 → 糊
blurExtra = blurT > 0.82 ? 2 : (blurT > 0.55 ? 1 : 0)   ← 后段：糊 → 厚
```

为什么必须两个：光调占比，上限就是 `D0` 那点模糊，拉满也不够厚；
光调级数，低强度时一跳一档，滑块没有连续感。

**实测梯度**（4px 棋盘 / 低频色块，见 §7 截图）：
- `barBlur=2`（mix 0.09）→ 色块边界清晰，只轻微柔化
- `barBlur=16`（mix 0.69）→ 边界柔化成渐变，**但左红→绿→黄→蓝的走向还在**
- `barBlur=32`（mix 1.0）→ 糊成一片，色带方向仍保留

---

## 4. 质感调理（P2）

着色器里新增五段，**玻璃模式全部是恒等变换**，老观感分毫不动：

| 段 | 作用 | 磨砂取值 |
|---|---|---|
| 饱和度 + 边缘 pop | 磨砂不全灰 | `sat=1.16`, `pop=0.16` |
| 对比 / 亮度 | 压暗背景不糊成白 | `contrast=1.06`, `bright=0.012` |
| 乳白膜 `u_film` | 磨砂的"实底" | `(1 - barBgAlpha/100) * 0.86` |
| 静态颗粒 | 磨砂的肌理 | `grain=0.5`（幅度 ≈0.11） |
| 边缘菲涅尔 | 厚度感 | `u_lightDir=(-0.55, 0.83)` |

**乳白膜接替了 v2.23.84 的 `u_bgMix`**，浓度由「底板透明度」反推：

- 滑块 100（全透）→ 膜 0 → 纯玻璃
- 滑块 0（实色）→ 膜 0.86

滑块语义和从前**完全一致**，只是执行者从 CSS 底板换成着色器 ——
好处是它跟着刚抓到的背景亮度自适应（`adapt`），白底桌面不糊成一片白、
深色桌面不发灰。

**颗粒用 `gl_FragCoord` 而不是 `u_time`**：不随时间变的才叫"磨砂的表面"，
随时间的叫"电视雪花"。

**折射没归零**（v2.23.84 曾全归零）：磨砂保留 `0.0015` 的贴边位移。
liquidDX11 的磨砂也有 `refr_band`，那一圈极轻的位移正是"玻璃有厚度"的来源；
归零之后边缘是一刀切的硬边，看着像贴纸。

---

## 5. 双光源（P3）

用户原有的是"两团面上光斑"（`u_specular`）。新增的是**边缘菲涅尔**：

```glsl
vec2 Ld = normalize(u_lightDir);
float ndl = dot(refractDir, Ld);
float frf = pow(max(0.0, ndl), 8.0);    // 朝光边：锐利高光
float bkf = pow(max(0.0, -ndl), 5.0);   // 背光边：宽而暗 = "厚"的那侧
finalCol.rgb += (frf*0.55 + bkf*0.18) * edge * u_rimStrength * 6.0;
```

`refractDir` 直接复用 SDF 边缘法线（shader 里 190 行算好的），不重复求导。
指数 8 / 5 不对称是照 `DirLight`（`shaders.h:193-214`）。

---

## 6. P4（blob SDF 融合）明确不做

liquidDX11 的 `sdScene` 用 smooth-min 把两个圆角矩形焊成一片，
**前提是有两个 blob**。而本项目：

> ⚠ 2026-10-16 r47（用户原话）：「通过更多引擎打开的页面就还是普通的页面，
> 默认的页面不要液态玻璃了，就不管任何主题都不会更改」

面板**明确不做玻璃**（`bar.css:773-781` 那一整段注释都在讲这件事），
所以永远只有横条一个 blob —— smooth-min 退化成一个圆角矩形，写它没有意义。

**这不是漏做**。哪天真要让面板也吃玻璃，扩展点在：
`draw()` 里已经按"每块玻璃一次 drawBox"组织，加一块只需要再算一份
`u_bounds` + 传第二个 blob 的中心/半尺寸。

顺带一个真有用的候选：`sdRoundedRect` 的超椭圆参数（`edge_p8.x = n`）。
iOS 那种 squircle 比正圆角好看，且不需要第二个 blob。要做得单独开一轮。

---

## 7. 验证

### 7.1 Rust 单测（4 个，全过）

```
test glass::tests::recapture_last_survives_take ... ok
test glass::tests::recapture_follows_new_rect ... ok
test glass::tests::tex_rect_grows_by_margin ... ok
test glass::tests::tex_rect_clamps_at_screen_edge ... ok
```

新增的两条是纯算术，不依赖屏幕：
- 屏幕正中 → 四条边都外扩满 `MARGIN`
- 贴左上角 → 只有右下两边扩，`off = (0,0)`

### 7.2 无头 Edge 探针（shader + FBO 链 + 状态串味）

用 `--headless=old --use-angle=swiftshader` 加载一个临时页，
mock 掉 Tauri IPC 直接喂一张 792×232 的假背景（`mx=my=96`），
走 `QSGlass.onShow()` → `onFresh()` → `upload` → `kick` → `draw` → `runBlur` 全链。

结果：

```
OK || after-frost || ready=true mode=frost tex=792x232 off=96,96
blurReady=true chain=792x232#4 levels=4 mix=0.868
runs=5 takes=1 caps=4 draws=5 passes=5 vis=visible
```

- `ready=true` → 主着色器 + 两个 blur 着色器**全部编译通过**
- `off=96,96` → 24 字节 IPC 头解析正确
- `chain=792x232#4` → 4 级链按 `MIN_COARSE_PX` 算对了
- 无 `console.error` / `console.warn`

玻璃模式（`barGlass=true`）同样跑通：`mode=glass draws=14 passes=14`，画面正常。

### 7.3 肉眼梯度（截图）

- 高频（4px 棋盘）：磨砂后完全糊掉，对照组棋盘清清楚楚 → 模糊真的在跑
- 低频（大色块）× 三档强度：见 §3 的梯度结论
- 圆角：左上 / 右上 / 左右边缘采样，圆角外是页面背景色 ✓

证据图（工作目录）：
- `frost-gradient.png` —— 大色块 × barBlur 2/16/32 的磨砂梯度
- `glass-probe2.png` —— 上半是 dual-filter 磨砂，下半是同一块背景的原始像素

### 7.4 收尾回归（改完上下文重建那段之后）

同一个探针再跑一次，全绿：

```
OK || ready=true mode=frost tex=792x232 off=96,96
blurReady=true chain=792x232#4 lv=4 runs=4 takes=1 draws=4 passes=4
```

全量测试：`513 passed; 0 failed; 2 ignored`。
`cargo build --release` 通过，前端 430 个文件打包进 exe（临时探针页已删，
没混进产物）。

---

## 8. 改动清单

| 文件 | 改了什么 |
|---|---|
| `src/glass.rs` | `MARGIN`/`HEAD`/`MAX_TEX_PIXELS` 常量；`Snap` 加 `mx/my`；`Rects` 结构替 `LAST_RECT`；`tex_rect_for()` / `remember()` / `rects()`；`capture_encoded` 与 `encode` 改 24 字节头；4 个单测 |
| `src/autotheme.rs` | 测试里的 `Snap` 构造补 `mx/my` |
| `src/main.rs` | 抓屏条件 `bar_glass` → `bar_glass \|\| bar_blur > 0` |
| `ui/glass.js` | blur 着色器 + FBO 链（约 180 行新代码）；shader 加 dual-filter 分支、色调调理、乳白膜、颗粒、边缘菲涅尔；`upload` 24 字节头；`u_bounds` 加纹理偏移；`isFrost`/`canvasOn`/`liveWanted` 让磨砂走 WebGL；`debugInfo` 加 `blur` 块 |
| `ui/settings.html` | 「磨砂强度」说明从"静态磨砂，不抓屏、不吃 GPU"改成实时磨砂 |
| `Cargo.toml` | 2.23.129 → 2.23.130 |

**没改**：`bar.css`（`.stage.qs-glass > .bar` 已有的透明规则直接生效，
`::before` 雾面那条本来就带 `:not(.qs-glass)`）、`bar.js`（`syncBarBg` 保留作
WebGL 不可用时的降级）、`settings.js`、`config.rs`。

---

## 9. 取舍与已知限制

**① 磨砂默认实时，抓屏限 30fps。**
`liveWanted()` 对磨砂返回 `barGlassLive !== false` —— 磨砂的定义就是
"实时反映背景"，否则退回静态雾面。抓屏给 30fps 上限：外扩后面积 2.7 倍，
"跟随显示器"（85fps）下每秒要过约 90MB IPC。用户显式设了 `barGlassFps` 就按用户的。

**② 静止时零开销。**
`blurWrong` 只在 `upload()`（有新纹理）时置上，`runBlur()` 只在它为真时跑。
静止的横条一次 blur 都不算 —— 这条和 r84/r86 的节流哲学一致。

**③ WebGL2 不可用时优雅降级。**
`initGL()` 失败 → `this.ready` 保持 false → `qs-glass` 类不挂 →
CSS 底板和 `::before` 静态雾面照常出现。磨砂退化成 v2.23.85 那套，不会变透明。

**④ 软件渲染下的 `ms` 不可信。**
无头探针用 swiftshader，`stats.blurMs` 报 0 是计时精度问题，不代表真实 GPU 耗时。
真实耗时要在有 GPU 的机器上看 `QSGlass.debugInfo().blur.ms`。

**⑤ 级数变化会重建整条 FBO 链。**
`ensureChain()` 的 key 是 `纹理尺寸#级数`。用户拖动磨砂强度跨过 0.55 / 0.82
两个阈值时会重建一次（几毫秒）。可以预建三级链避免，但那是过早优化。

---

## 10. 回滚

改动集中在 5 个文件，全部有本轮的注释标 `v2.23.130`。要单独关掉磨砂的
WebGL 管线（退回静态雾面），只改一处：

```js
// ui/glass.js
function canvasOn(c) { return glassOn(c); }   // 去掉 || isFrost(c)
```

连带 `liveWanted()` 也要回到只看 `barGlassLive`。其余（Rust 的 margin、
24 字节头）不影响静态雾面，可以留着。
