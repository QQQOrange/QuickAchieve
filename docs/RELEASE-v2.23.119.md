# Quick Achieve v2.23.119

> 用户要求「我就是要一个文件，不想要单独搞一个文件夹」。v2.23.118 已把持久数据收进
> `%APPDATA%\QuickAchieve`，但里面还剩 **75 MB 的 WebView2 缓存**。这一版把缓存也搬走：
> **放 `%TEMP%`，退出即删**，持久数据只剩配置 / 图标 / 日志（几 MB）+ 备份目录（你自己的数据）。

## 一、缓存位置

| 模式 | WebView2 缓存位置 | 退出后 |
|---|---|---|
| **默认** | `%TEMP%\QuickAchieve-WebView2-<pid>\` | 删除 |
| `QS_WEBVIEW_CACHE=persist` | `<数据目录>\WebView2` | 保留（启动更快） |
| `QS_WEBVIEW_CACHE_DIR=<目录>` | 指定目录 | 由用户管理 |

目录名带 PID：新实例永远不会撞上旧实例的缓存，退出清理助手可以放心删；
崩溃留下的残骸由**下次启动**统一清扫（日志：`wv2: 清掉 N 个上次遗留的临时缓存目录`）。

## 二、退出时怎么删掉的

WebView2 的 `msedgewebview2.exe` 子进程在主进程退出瞬间可能还占着文件，所以分两步：

1. 先直接 `remove_dir_all` 试一次（多数情况已退干净）；
2. 删不掉就把**本 exe 自己再拉起来一份**：
   `QuickAchieve.exe --clean-webview <目录> --parent-pid <pid>`

这个助手在 `main()` **最开头**处理（早于单实例检查、早于日志/配置初始化）：
不建窗口、不读配置、不写日志，只做 `win::wait_process_exit(pid, 20s)` → 删目录 → 静默退出。
它成功了，磁盘上就不留任何缓存。

## 三、代价（如实说明）

- 每次启动缓存重建，**第一个窗口慢 1~2 秒**；
- `localStorage` 不保留 —— 只有设置页搜索框的「最近用过」用它，其它设置都在 `config.json`；
- 运行期间 `%TEMP%` 下会有一个临时目录（退出即删）。
  **"运行中一个目录都不写"在 WebView2 下做不到** —— 这是 Chromium 的机制，不是本程序的选择。

## 四、验证

隔离实例（`--quit-probe`，走和托盘「退出」完全同一条 `quit_with_sync`）：

```
[15:00:00.051] === start, args=[…, --paused, --quit-probe], boot=no ===
[15:00:00.178] wv2: 清掉 1 个上次遗留的临时缓存目录
[15:00:02.051] quit-probe: 触发 quit_with_sync(0)
→ 退出后 %TEMP% 下已无 QuickAchieve-WebView2-* 目录
```

`QS_WEBVIEW_CACHE=persist` 也测过：不建临时目录、改用数据目录下的 `WebView2`。
