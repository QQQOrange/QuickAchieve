# Quick Achieve v2.23.53

> 修：托盘「退出」后**进程不退出**——图标从托盘消失，任务管理器里还留着。

## 根因（查了 tao 0.37 源码，不是猜的）

Windows 事件循环里，如果在**事件回调内部**把控制流设成 `ExitWithCode`，tao **不会当场 break**，要等下一轮再检查：

```rust
// tao/src/platform_impl/windows/event_loop.rs
if let ControlFlow::ExitWithCode(code) = runner.control_flow() {
    if !runner.handling_events() { break 'main code; }
}
```

而下一轮循环开头就是 `GetMessageW`。托盘图标已经被摘掉了，没人再发消息 → 永远阻塞 → 进程永远不退出。

## 修法（两层）

1. **唤醒主循环**：`AppHandle::exit()` 投递 `Exit` 之后，再 `PostThreadMessageW(WM_NULL)` 给主线程，把那次检查叫醒（`src/t.rs` 的 `wake_main_loop`）。
2. **硬兜底**：`quit_with_sync()` 的"不同步"分支请求退出后起一个 1.2 秒线程，没退就直接 `process::exit`；WebDAV 同步那条的 8 秒看门狗也改成硬退。

## 验证

新增 `--quit-probe`：启动后 2 秒走**和托盘退出完全同一条** `quit_with_sync(0)`。

```
隔离实例实测：2.0s 触发 → 2.3s 进程消失
日志：quit-probe: 触发 quit_with_sync(0)
```

⚠ 没能用"带完整界面的实例"验证：本机有一个**旧版残留进程**（权限更高、杀不掉）占着 WebView2 用户数据目录，新实例建窗失败。那个残留进程本身就是这个 bug 的现场——从托盘退一次旧版或重启就会清掉。

## 其它验证

| 项目 | 结果 |
|---|---|
| `cargo test` | **453** passed |
| `cargo clippy -- -D warnings` | **0 警告** |
| `node tools/check-all.js` | UI 880 / 页面 56 / 命令表一致 / 英文残留 3 —— 全过 |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 托盘图标右键 → 退出（**如果旧版退不掉，用任务管理器结束一次**）；
3. 覆盖 → 双击运行。

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）