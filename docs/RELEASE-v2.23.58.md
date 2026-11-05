# Quick Achieve v2.23.58

> 真凶找到了：`barList()` 把**空横条**当成"没配置"，直接退回整套引擎目录。

## 为什么「点掉了还在」

```js
// 以前
const raw = (barOverride && barOverride.length) ? barOverride
    : (CONF.barList && CONF.barList.length ? CONF.barList : all);   // ← 空数组 → 全部引擎
```

只要 `bar_list` 变成空数组（把 OCR 这类最后一项移除、或清空横条），前端就当成"没配"，
把**整个引擎目录**画回横条——OCR 自然也在里面。上一版我只是把「固定按钮」藏了，
但这条兜底会让 OCR 作为**引擎格子**重新出现。

## 现在

```js
const raw = (barOverride && barOverride.length) ? barOverride
    : (CONF && Array.isArray(CONF.barList) ? CONF.barList : all);
```

后端 `Config::default()` 的 `bar_list` 有默认值（`DEFAULT_BAR`），老配置缺字段走 serde
容器默认也不会是空数组——所以"空"只可能是用户真的清空了，前端必须尊重它。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test` | **453** passed |
| `node tools/check-all.js` | UI **883** / 页面 56 / 命令表 / 英文覆盖 —— 全过 |
| 新增用例 | 清空横条后引擎区一个都不剩 + 固定 OCR 按钮仍然藏着 |

## 安装

1. 下载 `QuickAchieve.exe`；
2. 托盘图标右键 → 退出；
3. 覆盖 → 双击运行。

## 许可

见仓库根目录 [LICENSE](https://github.com/QQQOrange/QuickAchieve/blob/main/LICENSE)（MIT）。

---

**SHA256: 1F71B4C7AF19404C6505249936F749C4772BDA3A63C0A43A42AF6358BA5198F1**

（`QuickAchieve.exe`，构建后由 `tools/release.ps1` 自动回填）