# Quick Achieve v2.23.129

> 内置「欧路词典」查词引擎。上一版只是「支持」本机程序 + 启动参数，这一版把它
> **做进软件里**：装完就有一条「欧路词典」，程序不在默认位置会自己找，找不到会
> 问你，顺手把程序自带的图标抠过来。

## 原因

v2.23.128 让软件引擎能启动 `exe + 参数`，但欧路词典仍要用户自己：找到 exe、
填路径、填 `-w {query}`、选类型、加进横条、再自己找一张图标。装在不同盘上的
机器还得先翻目录。用户的原话是「这个欧路查词要内置」。

## 新增

- **内置引擎**：首次启动写入「欧路词典」（id 固定 `eudic`，哨兵 `@app:eudic`，
  类型「搜索」，参数 `-w {query}`），并把它补进横条末尾。
  只写一次：配置里 `apps_builtin_seeded` 记着，用户删掉之后不会再被偷偷补回来。
- **路径探测三层**（`config::find_eudic_exe`）：
  1. `%ProgramFiles%`、`%ProgramFiles(x86)%`、`%LOCALAPPDATA%`、`%APPDATA%`
     下的 `eudic\eudic.exe`；
  2. 两个默认路径 `C:\Program Files\eudic\eudic.exe` /
     `C:\Program Files (x86)\eudic\eudic.exe`；
  3. 注册表 `HKCU|HKLM\Software\Microsoft\Windows\CurrentVersion\App Paths\eudic.exe`
     的默认值（新增 `win::app_paths_exe`，读到的路径还会再 `is_file` 验一次）。
  4. 开始菜单 / 公共桌面 / 当前用户桌面上的快捷方式：解开 `.lnk` 看它指向哪个 exe
     （新增 `win::lnk_target` + `win::shortcut_exe_path`）。绿色安装版往往什么都不留，
     只剩一个「欧路词典 eudic.lnk」—— 本机就是这样找到 `D:\software\eudic\eudic.exe` 的。
- **不重复加**：判重三条 —— 同 id、同名字、**指向同一个 exe**。用户在这一版之前
  就自己加过欧路词典（名字可能写的是「欧路」），名字对不上时靠 exe 路径认出来，
  避免横条上并排两个一模一样的格子。
- **找不到就问**：`open_app` 发现路径不在盘上时，弹一句「找不到「欧路词典」的程序，
  要现在指定它的位置吗」，选完写回**同一个 id**（横条格子与图标键都不变），
  然后继续查词。整段在新线程里跑（`open_app` 是同步命令、在主线程，
  在这里等文件对话框会把软件冻住）。
- **自动取图标**：启动 1.2 秒后从 exe 抠一次 256px 图标，落成
  `icons/<键>.png` 并写 `overrides["@app:eudic"]`；`overrides` 里已有键、
  或 icons 目录里已有同名 PNG 时**不覆盖**（那是用户自己的选择）。
- **设置页**：软件表单新增「从程序取图标」按钮 —— 换了 exe、或自动那次没取到
  （程序里没有图标资源）时的手动口子。

## 行为边界

- 内置那条和普通软件引擎完全同构：可以改名、改路径、换图标、移出横条、
  放进网格页，也可以删掉。删掉不会复活。
- `admin` 默认 `false`：查词是高频动作，勾了每次点都要弹 UAC；需要时自己勾。
- 图标提取失败只写日志，不打扰用户（此时显示首字徽标）。

## 部署时踩到的坑（已修，留档）

首次把 v2.23.129 部署到本机时，软件起来就 panic：

```
PANIC: panicked at tao-0.37.2/src/platform_impl/windows/window.rs:1145:41:
OleInitialize failed! Result was: `RPC_E_CHANGED_MODE`.
```

原因：`win::lnk_target` 里顺手 `CoInitializeEx(None, COINIT_MULTITHREADED)`，而它是
**在主线程**上被调用的（启动时播种内置引擎 → 猜路径 → 解 .lnk）。COM 的公寓模式
按线程定、且不可改：主线程先起了 MTA，tao 建第一个 WebView2 窗口时那句
`OleInitialize`（要 STA）就必然失败。

修法：`lnk_target` / `shortcut_exe_path` 的 COM 段落**整段扔进临时线程**，主线程一点
COM 都不碰。加回归测试 `lnk_target_keeps_caller_apartment`：在 STA 线程里调
`lnk_target`，调完该线程必须**仍是 STA**。

## 验证

- `cargo test --bin QuickAchieve`：513 项 / 511 passed / 2 ignored / 0 failed。
- 新增单测：内置清单契约（搜索类必须带占位符、默认不提权）、只播种一次、
  用户已有同名条目时不重复添加、探测到的路径必须真实存在且以 `eudic.exe` 结尾、
  `.lnk` 解析对非快捷方式安静返回 None、`lnk_target` 不许改调用方线程的公寓模式。
- 手动：在装好欧路词典的机器上启动 → 横条末尾出现「欧路词典」且图标是程序图标；
  选中文字点它 → 欧路词典打开并查该词；把配置里的路径改成不存在的路径再点 →
  弹「要现在指定它的位置吗」，选 exe 后正常查词。
