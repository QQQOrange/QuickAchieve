# Quick Achieve v2.23.121

> 两处按你的要求做的行为调整：**备份只留最近 10 份，其余全删**；
> **选区取词不再用屏幕 OCR 兜底**。

## 一、备份：固定保留 10 份

原来的轮转按“配置完整度”保护：比本次保存更有价值的旧备份不删，读不出来的旧备份也跳过。
好处是不会误删“内容更全”的备份，坏处是每保存一次都要解密目录里的每一份来算完整度，
而且只要旧备份一直“更有价值”，目录就会一直涨 —— 你的机器上已经堆到 **203 份 / 63 MB**，
其中 176 份是 WebDAV 启动同步自动生成的 `config(时间戳)`。

现在改成固定保留：

- `src/config.rs` 新增 `pub const MAX_BACKUPS: usize = 10;` 与
  `pub fn prune_backups(dir: &std::path::Path, keep: usize) -> usize`。
- 只处理备份族文件：`config.auto(...)`、`config(...)`、`config.before-*`；
  你自己放进备份目录的其它文件不碰。
- 按文件名括号里的时间戳倒序，保留最新 `keep` 份，其余 `remove_file`，
  日志：`config: 备份超过 10 份，清掉最旧的 <name>`。
- 排序键是 `backup_stamp_of(name)`（第一个 `(` 与 `)` 之间的时间戳），
  **不是整个文件名** —— 否则 `config(...)` 和 `config.auto(...)` 前缀不同，
  会把最新的手动备份排到最旧的滚动备份后面，删错文件。
- `save_to()` 写完备份后裁剪；`backup_once()`（WebDAV / 手动备份）落盘后也裁剪。
- 诊断页「备份份数」只统计备份族（含旧版 `config.broken(...)` / `config.wiped(...)` 存档），
  与「只留 10 份」的口径一致；`QuickSearch back` 这类子目录不算、也不删。
- 删除了不再被引用的 `backup_quality_cached`（含进程内缓存）。
  `backup_quality` 仍用于“恢复备份”列表的排序。

> 注意：升级后第一次保存 / 同步备份时，会把现有 203 份裁到 10 份，**不可恢复**。
> 如果你还想留某几份旧的，先拷出来再升级。

## 二、选区：不再用 OCR 兜底

按你的要求，划词取词只走“复制 / UIA 无障碍”两条通道，拿不到就提示，不再截屏识别：

- 删除 `post_ocr_fill()` 与 `ocr_region_text()` 及全部调用点
  （原 `post_show_copy_fill` 的终端分支、复制失败分支，以及 `confirm_selection_text` 的终端分支）。
- 起点在终端（控制台）时：不发 Ctrl+C（那是 SIGINT，会打断正在跑的命令），也不截屏，直接不弹。
  日志：`confirm: 起点在终端里 —— 不模拟 Ctrl+C（会打断正在跑的命令），也不做屏幕识别，不弹`。
- 复制 / UIA 都失败时：提示一次“该软件不支持划词取词”，不再补字。
  日志：`post-copy: 复制没拿到文字 —— 不做屏幕识别（按用户要求），放弃补字`。
- `bar_need_text` 的语义没变：它只管“Ctrl+C 空条是否延迟弹 / 是否先弹空条”，与 OCR 无关。
- 显式 OCR 功能（`ocr_region`、截图识图、在线 OCR）不受影响。

## 三、验证

```
cargo test                     492 passed / 0 failed / 2 ignored
cargo clippy --all-targets     clean（仅 vendored ureq 一条 warning）
cargo build --release          58.17 s
前端内嵌                        430 个文件，3.47 MB → 压缩后 1.55 MB
```

新增单测 `config::prune_tests::keeps_newest_ten_and_leaves_foreign_files_alone`：
13 份混合 rolling/manual + 1 份 before 快照 + 1 个用户文件 → 删 4 份、留 10 份 + 用户文件，
最旧的两份被删、最新的保留。

## 四、升级说明

- 版本号：`Cargo.toml` 2.23.121；`app.rc` 文件/产品版本 2,23,121,0。
- 覆盖安装后第一次启动即生效；备份裁剪在下一次保存或 WebDAV 同步时执行。
- 想保留更多历史备份，可把 `MAX_BACKUPS` 调大后自行编译，或升级前手动拷走 `back\` 目录。
