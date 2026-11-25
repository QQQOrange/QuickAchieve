# QuickAchieve 横条磨砂 → liquidDX11 质感的改造探讨

> 只探讨，不动代码。所有结论都对着两边的源码核过，行号可查。

---

## 0. 一句话结论

你现在缺的**不是"模糊强度"**，而是 **"两档模糊叠加插值"** 这个结构。

liquidDX11 的磨砂 = `lerp(softBlur, heavyBlur, mix)`，两档同时存在、按系数混。
你现在的磨砂 = `textureLod(tex, uv, log2(frost))`，只有一档，调的是半径。

前者磨砂里**还留着背景的形**，所以"能看出下面在动"；后者半径一大就是一块死糊，半径一小就是塑料片。这就是观感差的真正来源，跟参数调多少无关。

---

## 1. 两边现在到底在干什么

### 1.1 QuickAchieve 的链路

```
横条窗口显示
  └─ SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)   glass.rs:489
       └─ 横条对 BitBlt 隐身，抓到的永远是干净桌面
            └─ BitBlt + GetDIBits → RGBA 字节              glass.rs:163 capture_rect
                 └─ t::ipc 原始字节直传（不走 base64）      glass.rs:333 capture_encoded
                      └─ WebGL2 texImage2D 上传成纹理       glass.js
                           └─ textureLod 取 mip 低层级      glass.js:212-238
```

抓屏矩形来自 `main.rs:1016`：

```rust
glass::store_rect_only(wx, wy, pw, ph);   // ← 恰好是窗口矩形，零外扩
```

### 1.2 liquidDX11 的链路

```
DXGI Desktop Duplication（全程 GPU，零 CPU 拷贝）   backdrop.cpp:266 Capture
  └─ desktop_ 整屏纹理 1920×1080
       ├─ 6 级 down（4tap + 中心×4）/8              backdrop.cpp:250-252
       ├─ 5 级 up（8-tap tent）/12                  backdrop.cpp:253-257
       │    └─ heavy_  = 大半径模糊（全屏尺寸）
       └─ soft_   = 原图过一遍 spread=1             backdrop.cpp:259
            └─ 着色器里 lerp(soft, heavy, blur_mix) shaders.h:167-172
```

---

## 2. 三条差距，从致命到次要

### 差距 ①：抓屏矩形没有外扩 —— 磨砂的"厚度"被物理卡死（最致命）

横条是扁的，比如 `1200 × 40`。mipmap 按**两维同时**降采样：

| mip level | 尺寸 | 状态 |
|---|---|---|
| 0 | 1200 × 40 | 原始 |
| 3 | 150 × 5 | 还能看 |
| 5 | 37 × **1.25** | 矮边已死 |
| 6 | 18 × **0.6** | 无效 |

`glass.js:217` 的 `clamp(log2(max(1.0, u_frost)), 0.0, 6.0)` 就算钳到 6，**矮边那一维已经降到 1px 以下**，取出来的不是"半径 24px 的模糊"，而是"在竖直方向被压扁的拉伸糊"。所以你把磨砂滑块拉满，摸到的天花板是硬的。

对比 liquidDX11：它模糊的是**整屏 1920×1080**，`1/64` = `30 × 16`，半径绰绰有余。

**这一条是"为什么调不出质感"的第一根因。**

### 差距 ②：单档半径 vs 双档插值 —— 结构上的缺失

| | QuickAchieve | liquidDX11 |
|---|---|---|
| 模糊档数 | 1 档（mip lod） | 2 档（soft / heavy） |
| 强度控制 | 改 lod → 改半径 | 改 mix → 改**两档占比** |
| 半径大时 | 一块死糊 | heavy 占比高，但 soft 的形还在 |
| 半径小时 | 塑料片 | soft 占比高，等于没磨砂 |
| 中间态 | 介于两者之间的糊 | 磨砂里**透出背景轮廓** |

你想要的"本身是磨砂的，但能实时反馈下面的背景变化"——这个描述**精确对应 `lerp(soft, heavy, mix)` 的中间态**，而不是任何一种单档模糊。

顺带一个细节（值得抄）：`shaders.h:174-179`

```hlsl
float3 sampleBackdropFrost(float2 uv, float frost) {
    return lerp(s, h, saturate(blur_mix + frost));
}
```

`frost` 传的是 `erUse`（边缘 SDF 的平方，`shaders.h:279`）——**边缘更磨砂、中心更清透**。这是液态玻璃的标志性观感，单档方案里没法表达。

### 差距 ③：着色器里少了一整套"色调调理"

`shaders.h:297-353` 这一段你一行都没有：

| 效果 | 代码 | 作用 |
|---|---|---|
| 饱和度 + 边缘pop | `lerp(lum, B, saturation + fresR*ecSatPop)` | 磨砂不全灰，边缘更艳 |
| 亮度/对比 | `(B-0.5)*contrast+0.5+brightness` | 压暗背景不糊成白 |
| 亮度自适应 tint | `adapt = smoothstep(...bgLum...)` | 背景亮→tint 浓，背景暗→更透 |
| 静态噪点 | `hash21(client + time)` × grain | 胶片感；不是"颗粒"是"细节" |
| 双光源 + 光标高光 | `DirLight` + `exp(-cdist²/size²)` | 条随鼠标起一层柔光 |
| 环向渐变 | `sin(ang - gph) * gmul` | 微弱的"液体流转" |
| **blob SDF 融合** | `sdScene` + `blob_k` smooth-min | **横条和展开面板连成一片** ← 你这个形态特别用得上 |

---

## 3. 三个方案

| | 做法 | 改动量 | 磨砂质量 | 判定 |
|---|---|---|---|---|
| **A** | 纯前端：抓屏外扩 margin + WebGL2 自建 dual-filter FBO 链 | Rust 约 1 行，glass.js 约 150 行 | ≈ liquidDX11 | **推荐** |
| B | Rust 上 DXGI Duplication + D3D11 blur，结果**读回字节**给 WebGL | 大 | 模糊 1:1，但多一次 readback | 不划算，比现在更慢 |
| C | 横条整体改原生 D3D11 渲染 | 极大 | 1:1 | 38 万行 JS 白写，不现实 |

### 方案 B / C 为什么否掉（有一条硬墙）

WebGL 拿不到 D3D11 纹理句柄。`WGL_NV_DX_interop2` 是桌面 GL 的扩展，WebGL 跑在 ANGLE/D3D11 之上但**不暴露底层资源**。所以"原生模糊好、直接给 WebGL 用"这条路不通，只能读回字节——那 B 就退化成"用更慢的方式做 A 能做的事"。

C 唯一的现实子集是：横条保留 WebView2，底下垫一层原生 D3D11 玻璃层窗口。但那要处理分层窗口命中测试和 WebView2 透明合成，工程量跟收益不成比例——**而且它解决不了你的问题**：你的问题在模糊算法，不在渲染后端。

---

## 4. 推荐路线：A 拆三步，每步独立可回滚

### A1 — 抓屏外扩（1 行 Rust，收益最大）

**改哪里**：`main.rs:1016` 附近

```rust
// 现在
glass::store_rect_only(wx, wy, pw, ph);

// 改成（M = 96 物理px，可按 scale 调）
let m = ((96.0 * scale) as i32).max(48);
glass::store_rect_only(wx - m, wy - m, pw + 2*m, ph + 2*m);
```

**前端配套**：`glass.js` 的 `u_bounds`（"画布在纹理里的子矩形"）本来就是干这个的，把 margin 算进去即可——**这个机制你已经有了**，天生支持外扩。

**立刻拿到的收益**：
- mip 有效高度从 40 → 40+192 = 232，`1/64` = 3.6px，**矮边不再死**
- 边缘有真实桌面可采，折射和磨砂在边界处连续，不再"钳制"（`glass.js:207` 那段钳制逻辑会自然很少触发）
- 磨砂滑块从"调不动"变成"真的有反应"

**代价**：纹理面积约 2.6 倍（1200×40 → 1392×232）。BitBlt 和 IPC 字节都涨，但抓屏是异步且已有 `captureBusy` 去重，实测主进程压力极小（你注释里写过"抓屏只占 0.2%"）。

### A2 — dual-filter FBO 链（约 120 行 GLSL/JS）

在 `glass.js` 里，**只在 `dirty` 为真时**（你已经有这套节流了）跑一遍：

```
tex ──down×6──> D0..D5   (4tap + 中心×4, /8)
                D5 ──up×5──> U0..U4  (8-tap tent, /12)
                            └─> heavyTex
tex ──up×1(spread=1)──────> softTex
```

然后 `draw()` 里把 `glass.js:212-238` 那段换成：

```glsl
vec3 s = texture(softTex, uv).rgb;
vec3 h = texture(heavyTex, uv).rgb;
refrCol = vec4(lerp(s, h, u_blurMix), 1.0);
```

**参数重标定**：现在 `u_frost` 直接吃 `qsBlurPx(barBlur)` 映射到 0~24px（`glass.js:491-497`）。换成 mix 之后，磨砂滑块变成 0~1 的**占比系数**，`qsBlurPx` 那个映射表要么删掉、要么改成控制 heavy 的累加级数（像 liquidDX11 那样 `extra = (amt-3)/2`，`backdrop.cpp:155`）。

**成本**：11 个 pass，但每个都是半分辨率起跳、条尺寸级别。而且——**这是重点**——只在背景真变时才跑。静止的横条一次都不跑，这跟你现在 `dirty` 的设计哲学完全一致。

### A3 — 色调调理移植（纯加法，可挑着做）

按性价比排序，建议先做前三个：

1. **亮度自适应 tint**（`adapt`）：背景亮时自动加深底色。解决"白底桌面下磨砂糊成一片白"
2. **grain 静态噪点**：一个 `hash21`，成本几乎为零，质感提升明显（把"均匀糊"变成"有肌理"）
3. **饱和度 + 边缘 pop**：一行 lerp
4. 双光源 + 光标高光：中等改造成本，是"液态感"的主要来源
5. blob SDF 融合：横条 ↔ 面板展开时连成一片。**你这个"胶囊 + 面板"的形态恰好是它最适用的场景**，但属于锦上添花

---

## 5. 坑

| 坑 | 说明 |
|---|---|
| **margin 会让横条自己进画面** | 现在靠 `WDA_EXCLUDEFROMCAPTURE` 隐身（`glass.rs:489`），已经解决了。但外扩后抓到的区域更大，如果隐身失败，画面里会出现自己的影子——**要在外扩后回归测一次隐身是否还生效**（`glass.rs:548` 的 `can_capture` 断言可以扩到这里） |
| **多屏 / 负坐标** | `capture_rect` 已有"矩形必须完整落在虚拟屏幕内"的检查（`glass.rs:167`）。外扩后更容易越界，需要把 margin 在越界时**向内收缩**，而不是直接失败 |
| **`u_bounds` 的坐标换算** | 外扩后画布在纹理里的位置不再是 `(0,0,1,1)`。这是 `u_bounds` 本来就设计好的用途，但 `glass.js` 里所有基于 `uv` 的假设（尤其 `glass.js:199-207` 的透镜映射）都要复核一遍 |
| **纹理重传 vs 重建** | 尺寸固定后应该用 `texSubImage2D` 而不是每次都 `texImage2D`——否则尺寸一抖就重建整条链。现在尺寸只在窗口矩形变化时变（`main.rs:1016` 有判断），所以基本稳 |
| **不要给 `.bar` 加回 `backdrop-filter`** | `bar.css:173-179` 那条注释已经踩过了：透明悬浮窗里它取样的是页面内容（空的），只会烧 GPU。磨砂只能走 canvas |
| **frost 语义变更会打架** | `glass.js:399-404` 的 `isFrost(c)` 用 `barGlass === false && barBlur > 0` 判定。改 mix 之后这条判定和 `barGlassFrost` 的关系要重新理，否则设置页的滑块和实际渲染对不上 |

---

## 6. 如果要一张"抄作业清单"

按投入产出排序：

| 优先级 | 动作 | 位置 | 工作量 |
|---|---|---|---|
| P0 | 抓屏外扩 +96px | `main.rs:1016` | 半天（含回归） |
| P1 | dual-filter FBO 链 + `lerp(soft,heavy,mix)` | `glass.js:212-238` | 1~2 天 |
| P2 | 亮度自适应 tint + grain | 着色器尾部 | 半天 |
| P3 | 边缘 frost（`+ erUse`） | 同 P1 的采样处 | 2 小时 |
| P4 | 双光源 + 光标高光 | 着色器 | 1 天 |
| P5 | blob SDF 融合（横条+面板连体） | 需要 Rust 侧传 second blob 参数 | 2~3 天 |

---

## 7. 结论

**你要的效果，结构上就是 "两档模糊 + 按系数混"，不是"一个可调的模糊半径"。**

- 你现在的 `textureLod` 单档方案，因为横条矮边的物理限制，**连"一档大半径"都给不了**——这是先要修的（P0）
- 修完 P0，双档才有可能谈（P1）
- P1 做完，"磨砂但能看见背景在动"这个描述就自然成立了
- P2 之后才有 liquidDX11 那种"高级感"，但那是调味，不是主菜

顺便说一句：`glass.js` 开头那段注释里你写的"C̶S̶S̶ backdrop-filter 取样的是页面内元素，透明悬浮窗里那后面是空的——只剩底板色，就是塑料磨砂的根因"——这个判断完全正确，而且思路（Rust 抓屏 → 纹理 → WebGL 折射）也跟 liquidDX11 是同一个架构。**卡住的地方只有"D3D11 的 mip 链是 6 级全屏"和"WebGL 的 mip 是一条扁平纹理"这个尺度差。**
