# 真机 E2E

`tools/` 下那一堆 `*-probe.js` 是历史探针，用完即弃；这个目录只放**会被反复跑**的场景。

## 前置

- Windows 桌面会话（WebView2 + 真实指针坐标；远程无桌面会话跑不了）；
- 先构建：`cargo build --release`（产物 `target/release/QuickAchieve.exe`）；
- Node 22+（用到全局 `WebSocket` / `fetch`）。

## 跑

```powershell
node tools/e2e/run.js            # 全部
node tools/e2e/run.js away       # 只跑移远收起
node tools/e2e/run.js frost reset
node tools/e2e/run.js --list
```

退出码 0 = 全过。

## 隔离与清理

- 每个场景使用 `%TEMP%\qa-e2e-<tag>-<pid>` 作为 `QS_DATA_DIR`，独立 CDP 端口；
- 进程带 `QS_NO_ELEVATE=1` + `--no-elevate`，不会弹 UAC；
- 清理只杀路径等于 `target/release/QuickAchieve.exe` 的进程，
  **不会碰** `D:\software\myself\QuickAchieve.exe` 正式版；
- 测试会移动系统指针（移远场景需要），别在演示/录制时跑。

## 现有场景

| 名字 | 钉住什么 |
|---|---|
| `away` | 指针不动不收起；移开 500px 后 ~0.35s 收起；日志有「指针移动 ≥6px」 |
| `frost` | 静态磨砂不抓屏（captures=0）、无 backdrop-filter、底板 alpha=0.45、雾面在 `::before` |
| `reset` | 设置页每张卡片有「恢复本节默认」；磨砂卡片恢复 `barBlur=12`、`barBgAlpha=55` |

## 加场景

1. 复制一个场景文件，只改业务断言；
2. 用 `harness.js` 的 `App` / `Suite`：
   ```js
   const { App, Suite, sleep } = require('./harness');
   module.exports = async function run() {
       const s = new Suite('我的场景');
       const app = new App({ tag: 'mine', port: 9898 });
       let bar = null;
       try {
           await app.launch();
           bar = await app.connectBar();
           // …操作与断言
       } finally {
           if (bar) bar.close();
           await app.stop();
       }
       return s.done();
   };
   ```
3. 在 `run.js` 的 `SCENARIOS` 里登记。

## 为什么不放进 GitHub Actions

默认 `windows-latest` runner 没有交互式桌面（全局钩子、指针坐标、置顶窗口都不可靠）。
`ci.yml` 跑的是 clippy + 474 单测 + 887 UI 金丝雀，已经覆盖纯逻辑；
这个目录给「本机 / 自托管 runner」用。
