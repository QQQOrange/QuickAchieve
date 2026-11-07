# Quick Achieve v2.23.71

> 「收藏这段文字」正式变成横条的**一等成员**（和 @ocr / @img 同一套），"移出横条"真的能移出了。

## 之前为什么移不掉

设置页那个按钮调的是 `set_setting('favQuickShow', …)`，而横条是另一套判断（`fav_quick_show`）—— 两者和真正的横条成员（`bar_list`）脱节，所以点了没反应、也不跟 `@ocr`/`@img` 一致。

## 现在的逻辑

- 新增哨兵常量 `BAR_FAVQUICK = "@favquick"`，并加进 `toggle_bar_engine_impl` 的放行名单 —— 它就是 `bar_list` 里的一项；
- 设置页那一行的「移出横条 / 加入横条」改调 **`toggle_bar_engine(engine='@favquick')`**，和引擎完全同一条路；
- 横条 `applyFavQuickVisible()` 改成**只看 `bar_list` 里有没有 `@favquick`**；
- 一次性迁移：老配置里 `fav_quick_show=true` 且列表里没有它的，自动补到最前面；之前关掉的（false）不补。

## 验证

| 项目 | 结果 |
|---|---|
| `cargo test -- --test-threads=1` | **459** passed |
| `node tools/check-all.js` | UI 883 / 页面 56 / 命令表 / 英文覆盖 —— 全过 |

---

**SHA256: 0000000000000000000000000000000000000000000000000000000000000000**