# Quick Achieve v2.23.116

> 两个回归修复：**托盘打不开设置**、**磨砂的强度/透明度拖动时看起来没生效**。

## 一、托盘打不开设置

真机日志（2026-11-11 12:20:43）里抓到的是：

```
tray: 左键抬起（时限 500ms）→ 判为双击，开设置
settings: building window
tray: 左键抬起（时限 500ms）→ 判为双击，开设置
settings: building window
settings: build failed: WebView2 error: WindowsError(Error {
  code: HRESULT(0x80010108), message: "被调用的对象已与其客户端断开连接。" })
```

**两次 `open_settings_win` 并发建窗**（双击判定 + 菜单/前一次请求）时，
WebView2 以 `RPC_E_DISCONNECTED` 失败，结果一个窗口都没建出来。

修复：

- 新增 `SETTINGS_BUILDING: AtomicBool`：同一时刻只允许一次建窗，重复请求
  记日志 `settings: 已有一次建窗在进行，忽略重复请求` 后返回；
- 建窗成功/失败都复位；失败后 **250ms 自动重试，最多 2 次**，并记
  `settings: 250ms 后自动重试（第 N 次）`；
- 右键菜单的「设置…」统一走 `tray_open_settings()`（和双击同一条 500ms 去重）；
- 托盘入口新增日志 `tray: 打开设置（双击 / 菜单）`，下次再出问题能直接看出
  是没进回调、还是建窗失败。

## 二、磨砂强度 / 透明度拖动不生效

设置页滑块原来只在 `change`（松手）时 `set_setting`；拖动过程中横条完全没变化，
看起来就像“调了没生效”。

现在 `barBlur`（磨砂程度）和 `barBgAlpha`（底板透明度）在 `input` 时也**防抖
120ms 落库**，`change` 清掉计时器再发一次保证最终值落盘；其它滑块保持原行为。

真机验证（隔离实例 + 真实配置）：

| 操作 | 配置 | 横条 CSS |
|---|---|---|
| 初始 | blur=0 / alpha=65 | 实色白 |
| input blur=19（未松手） | blur=19 | `rgba(255,255,255,.35)` + 噪点 |
| input alpha=25（未松手） | alpha=25 | `rgba(255,255,255,.75)` |

## 三、验证

| 项目 | 结果 |
|---|---|
| `cargo test` | **478 passed** / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | **887 / 887** |
| `tools/test-pages.js` | **56 / 56** |
| `tools/check-all.js` | 五项全过 |
| `node tools/e2e/run.js` | **4/4 场景** |
| 真机滑块探针 | 拖动中配置 + CSS 都变 |

---

**SHA256: 6048D18576AC4ED4841C194EDE3E06990A2BF06055E59574833EBD72779AA018**