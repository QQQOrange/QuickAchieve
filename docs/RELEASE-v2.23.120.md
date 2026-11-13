# Quick Achieve v2.23.120

> 功能核查时发现的一个自启状态误判：从相对路径启动（`cd /d D:\software\myself` 然后
> `./QuickAchieve.exe`）时，设置页和诊断会把**已经生效**的自启报成"未生效"。
> 这一版把路径归一化补上，并顺手堵住"把相对命令写进自启项"的隐患。

## 一、问题

`current_exe()` 在相对路径启动时会原样带回 `D:\software\myself\.\QuickAchieve.exe`，
而计划任务里存的是 `D:\software\myself\QuickAchieve.exe`：

```
开机自启: 配置 开 / 实际 未生效（未启用）     ← 误报
```

`src/autostart.rs` 的 `normalize_path()` 只做了斜杠 / 大小写归一，`.`
分量原样保留，两个字符串不相等 → `task_points_here=false`。

## 二、修复

- `normalize_path()`：先 `std::fs::canonicalize`（折叠 `.` / `..`，顺带解析短名），
  失败再退回原来的字符串归一。`D:\dir\.\a.exe` 与 `D:\dir\a.exe` 现在判等。
- `exe_path()`：写注册表 / 计划任务前也走 `abs_path()`。否则会把
  `".\QuickAchieve.exe" --autostart` 这种**相对命令**写进自启项 ——
  登录时工作目录不是 exe 目录，必然起不来。
- 新增单测 `normalize_path_collapses_dot_components`。

## 三、验证

```
cargo test                     491 passed / 2 ignored
cargo clippy --all-targets     clean
node tools/check-all.js        UI 887/887 · 页面 56/56 · 命令表 OK · 设置项 0 问题
```

真机（`cd /d/software/myself && ./QuickAchieve.exe --diag-probe`）：

```
Quick Achieve: 2.23.120
exe 文件版本: 2.23.120
exe 路径: D:\software\myself\.\QuickAchieve.exe
开机自启: 配置 开 / 实际 生效（已生效：计划任务（登录时以最高权限运行，无 UAC））
```

升级后第一次启动的计划任务 `RegistrationInfo Date` 不再变化（没有多余的 repair 重写）。
