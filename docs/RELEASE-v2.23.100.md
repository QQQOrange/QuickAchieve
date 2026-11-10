# Quick Achieve v2.23.100

> 前端拆分第二刀：`settings.js` 的「恢复本节默认」按钮安装逻辑搬到
> `ui/settings-card-reset.js`。依赖用对象显式传入，不靠跨脚本全局。

## 一、拆了什么

`ui/settings-card-reset.js`（88 行）现在负责：

- 给每张带 `range` / `checkbox` 控件的卡片装一颗「恢复本节默认」；
- 从控件 id 反推配置 key（`s-<key>`），只处理数字/开关，不碰路径、凭据、数组；
- 点击时懒取 `defaults_view`，只把与默认值不同的字段通过 `set_settings` 写回；
- 滑块默认值优先用 `SLIDERS` 的样式默认（磨砂强度 12），否则用 Rust 默认。

`settings.js` 里只留一个 10 行的包装器，把状态以依赖对象传给新文件：

```js
window.QSCardReset.install({
    defaults, current, sliders: SLIDERS,
    getConf: () => CONF,
    getDefaults: () => DEFAULTS,
    setDefaults: (v) => { DEFAULTS = v; },
    invoke, toast,
});
```

这样 `settings.js` 仍是 `CONF` / `DEFAULTS` / `SLIDERS` 的唯一持有者，
新文件不保存任何配置；`tools/test-ui.js` 逐脚本 eval 也能正常跑。

## 二、行数

| 文件 | 之前 | 现在 |
|---|---|---|
| `ui/settings.js` | 12,969 | **12,884** |
| `ui/settings-card-reset.js` | — | 88 |

## 三、验证

| 项目 | 结果 |
|---|---|
| `cargo test` | 474 passed / 0 failed / 2 ignored |
| `cargo clippy --all-targets -- -D warnings` | **通过** |
| `tools/test-ui.js` | **887 / 887** |
| `tools/test-pages.js` | **56 / 56** |
| `tools/check-all.js` | 五项全过 |
| `node tools/e2e/run.js` | **3/3 场景**；「恢复本节默认」6 条断言全过（按钮装得上、点得动、值回默认） |

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**
