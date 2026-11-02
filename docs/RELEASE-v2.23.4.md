# Quick Achieve v2.23.4

> v2.23.3 之后的复核收尾。三路只读审计（哨兵登记 / 框选状态机 / 配置改名）加自查，
> 修掉 17 处，其中 **3 处会坏功能**。

## 会坏功能

| 问题 | 症状 | 修法 |
|---|---|---|
| `toggle_bar_engine_impl` 的准入是一串手写 `&& engine != BAR_XXX`，`@any` 只进了配置层判据 | **「＋ 加进横条」点了没反应**：列表不变、横条上也不出现 | 准入链补 `BAR_ANY`；两处判据各钉一条测试 |
| `export.rs` 的 `appSettings` 组缺 `regionAdjust`（也缺 `barTriggerMode` / `barIdleSec` / `barAwayPx` / `barNeedText`） | 勾「应用设置」导出 `.qsconf`、换机器导入 —— 这几项**静默**回默认值（导入侧按白名单硬裁） | 把 5 个键补进组里 |
| 恢复 **Config 格式**的备份是整份赋值 | 老备份没有 `region_adjust` 键 → 解析成 `false` → 用户刚打开的「框选后可调整」被静默关回去（只提示了令牌） | 键缺失 = 不动，与 `apply_view` 对齐 |

顺带：新建自定义引擎不能再叫 `@any` / `@img` / `@trans` / `@qr` / `@qrmake` / `@wrap`
（以前只挡 `@ocr`）。不挡的话保存是成功的，但渲染时哨兵先命中，用户自己那条永远点不出来。

## 框选遮罩里 9 处

| 问题 | 症状 |
|---|---|
| `#dim` 自带 `cursor: crosshair` 且铺满全屏 | 调整阶段的手柄光标 / 移动光标**从来没出现过**（JS 写在 body 上的全被盖住） |
| `#dim` 34% + `#sel` 阴影 34% 叠了两层 | 选区外 56%、**选区内仍有 34%** —— 选中的内容看着跟没选一样暗，"掏洞"等于没生效 |
| 动作条无条件画 5 颗 | 翻译 / 识图 / OCR 关掉后仍能点，点了白框一次（后端才拒绝） |
| `keydown` 抢回车 | Tab 到「翻译」再按 Enter，执行的是**高亮那颗**（常常是识别） |
| `commit()` 原样发出负坐标 | 把框拖出屏幕再执行，截图带黑边，OCR / 识图跟着吃到黑边 |
| 没有 `pointercancel`，`pointermove` 不看 `e.buttons` | 在窗口外松手后，鼠标一动框就跟着跑（"橡皮筋"） |
| 小框（<24px）点内部判成 `new` | 框个图标想挪一下，一点就重来，选区被毁 |
| `buildBar()` 无条件重建 DOM | 配置晚到几十毫秒触发重建，正按着的按钮被摘掉 → 点了没反应 |
| `holdMode` 在配置回来前按 `false` 算 | 开了「框选后可调整」但手快，仍被立刻执行 |

还修了：`@any` 图标写死 22px（调大图标后永远比邻居小一号）、`placeBar` 在窗口比动作条窄时
算出负数（最左边那颗点不到）、尺寸标签右边被裁、拖框起点不吸附、`jlog` 没 `.catch`
（`log_msg` reject 会自激成一串 IPC）、英文帮助里 ⑤ 还写着旧流程、字典里 3 条死词条。

## 已知未修（记在这里，不藏）

1. **二维码结果窗按 Esc 会丢**：全局键盘钩子抢在 WebView 前头直接关窗，绕过了
   `ocr_cancel` 里把结果窗还回来那段。要把取消路径抽成 `cancel_region_flow()` 共用，
   动的是全局低级钩子，单独一轮做更安全。
2. **心跳兜底在提交后失效**：判据是 `REGION_BEAT == 0`，而它只被写成非零时间戳，
   提交过一次后"5 秒没心跳就关窗"不再成立（仍有 30 秒总闸）。
3. `MODE === 'app'` 整套分支是死代码（点选程序早走 `picker.html`）。
4. `#size` 标签竖直方向没夹（水平方向已夹）。

## 安装

1. 下载 `QuickAchieve.exe`；
2. 覆盖你现有的那份（**数据目录 `Quick Achieve data\` 不受影响**）；
3. 双击运行。首次运行没有窗口是正常的 —— 在任务栏右下角托盘区找它。

> ⚠ 如果软件正在运行，Windows 会拒绝覆盖。先退出（托盘右键 → 退出），再替换。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | **426 passed / 0 failed / 2 ignored** |
| `node tools/test-ui.js` | **827 / 827** |
| `node tools/test-pages.js` | **56 / 56** |
| `node tools/i18n-coverage.js` | 12 个页面，残留 2 条（均为应有行为） |
| 真机启动 | 无 PANIC，托盘 / 钩子正常就绪 |

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 6ED9FEB9E3EDD8C57078A52EB6F6EC7AD34E938703ECF224E61AE0C02733B33F**

（`QuickAchieve.exe`，7,923,712 字节 / 7.56 MiB）
