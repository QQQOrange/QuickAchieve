# Quick Achieve v2.23.118

> 用户要求：「只想 QuickAchieve 只保留一个单 exe 文件……不会另外生成文件夹」。
> 现在 **exe 旁边只有一个 exe**；想要绿色便携，放一个 `portable.flag` 就切回去。

## 一、为什么不能"零文件夹"

QuickAchieve 的界面全部由系统自带的 **WebView2（Chromium 内核）** 渲染，而
WebView2 **强制**需要一个可写的"用户数据目录"（缓存 / GPU 着色器 / localStorage），
没有它窗口建不出来，也没有"放内存"的选项；程序本身还要存配置、日志、备份。

所以能做到的是**搬家**：把这些都挪到系统应用数据目录，exe 旁边自然就干净了。

## 二、现在的存储方式

| 模式 | 触发条件 | 数据位置 |
|---|---|---|
| **单 EXE（默认）** | 无 | `%APPDATA%\QuickAchieve\`（配置 / 图标 / 日志 / `WebView2` 缓存），备份在 `…\back\` |
| 绿色便携 | exe 旁边有空的 `portable.flag` | `Quick Achieve data\` + `Quick Achieve back\`（exe 同级） |
| 自定义 | 设了 `QS_DATA_DIR` | 该目录（测试 / 调试用） |

## 三、老用户会自动迁移

第一次启动新版时，如果 exe 旁边有 `Quick Achieve data\config.json`：

1. **复制**：数据目录、图标、日志、各类标记文件、备份目录 → `%APPDATA%\QuickAchieve`；
2. **校验**：`config.json` 大小一致、`icons\` 逐文件大小一致、`back\` 逐文件大小一致；
3. **全过了才删旧目录**；任何一步不对就**继续用旧目录**，数据一个字节都不动，
   日志里写清原因（例如"迁移到 %APPDATA% 失败（…）—— 继续用 exe 旁边的数据目录"）。

真机验证（隔离目录 + 真实配置副本）：

```
[2026-11-13 14:19:57.925] === start, args=[…\qs-single-test\QuickAchieve.exe, --paused], boot=no ===
[2026-11-13 14:19:57.925] config: 已迁移到单 EXE 模式：C:\…\qs-single-appdata\QuickAchieve（20 个文件，备份 205 份）
[2026-11-13 14:19:58.011] wv2: 已清理旧的缓存目录 C:\…\qs-single-test\QuickAchieve.exe.WebView2（现在统一放在数据目录下）
[2026-11-13 14:19:58.485] bar: 第一个 WebView2 窗口已建成
```

跑完之后原目录里**只剩 `QuickAchieve.exe`**，配置、图标、203 份备份、WebView2 缓存
全部落在 `%APPDATA%\QuickAchieve`，应用正常启动。

## 四、WebView2 缓存也搬走了

所有窗口改用 `wry::WebContext::new(Some(数据目录\WebView2))` 建环境，不再用
默认的 `<exe>.WebView2`。启动时还会顺手清理旧版本留下的
`<exe>.WebView2` / `*.old-*.WebView2`（那时还没建任何 WebView，目录没被占用）。

> 这个缓存目录删了也没事，下次启动会自动重建，只是首次启动慢一点。

## 五、顺带修掉一个潜在死锁

老代码在"便携目录不可写 → 退回 %APPDATA%"那条分支里直接 `win::log`，而
`win::log` 的开关判定会回头调 `data_dir()` —— 同一个 `OnceLock` 二次进入就是死锁。
现在这类说明先攒进队列（`config::take_data_dir_notes`），启动日志落盘后再补写；
`portable_dir_adopted` 里同样的写法也一起改掉了。

## 六、验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 490 通过 / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | 通过 |
| `node tools/check-all.js` | UI 887 / 页面 56 / 命令表 / 英文 / 设置项 全绿 |
| 真机隔离实测 | 迁移 + 旧缓存清理 + 新 UDF 生效；根目录只剩 exe |
| 新增单测 | 迁移成功（复制/校验/删旧）、迁移失败（源数据保留） |
