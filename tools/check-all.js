#!/usr/bin/env node
/* ============================================================
 *  一键跑完所有前端自检
 * ============================================================
 *
 *  为什么要有它：发布前要跑 5 个脚本（UI 金丝雀 / 页面冒烟 / 命令表一致性 /
 *  英文覆盖 / 设置项一致性），手动一条条敲容易漏，而且漏了不会有任何提示 ——
 *  那个"漏跑一次"恰好就是把 i18n 盲点带进版本的方式（见 v2.23.36）。
 *
 *  用法：node tools/check-all.js [--quiet]
 *  退出码：0 = 全过；1 = 有失败（发布脚本据此拦住）
 *
 *  ⚠ 它只跑**前端**那几套。Rust 侧仍然要 `cargo test`（发布脚本自己做）。
 */
'use strict';
const { execFileSync } = require('child_process');
const path = require('path');

const root = path.resolve(__dirname, '..');
const quiet = process.argv.includes('--quiet');

const steps = [
    // [名字, 脚本, 允许的最大"残留"数, 输出里用来判断成败的关键字]
    ['UI 金丝雀（jsdom）', 'tools/test-ui.js', null, /(\d+)\s*\/\s*(\d+)\s*通过/],
    ['页面冒烟（每页能不能起来）', 'tools/test-pages.js', null, /(\d+)\s*\/\s*(\d+)\s*通过/],
    ['命令表一致性（invoke ↔ Rust）', 'tools/check-cmd-map.js', null, /\[OK\]/],
    /* 允许的残留只有三类，都是**应当保留中文**的：
     *   · 语言自称「简体中文」（下拉里就该显示它自己的语言名）；
     *   · 引擎名（`title=百度` 这类，专有名词）；
     *   · 同上，出现在第二处（同名引擎在两条轨上各有一个扁标）。
     * 之前是 2，加了设置项搜索之后引擎名的那条计数变成 2 个，合计 3。 */
    /* v2.23.88：3 → 5。残留是**用户数据类**固定项：横条/设置页里的引擎名
       「百度」（index 3 处 + settings 2 处）和语言自称「简体中文」。
       新增的设置项文案已经全部进 i18n；再涨就说明真有漏翻。 */
    ['英文覆盖（残留条数）', 'tools/i18n-coverage.js', 5, /合计残留：(\d+)/],
    /* v2.23.89：设置项三处登记（HTML 控件 / SLIDERS / config_view_of）静态对账。
       滑块拖了没反应、值不保存、默认值对不上，先看这一行。 */
    ['设置项一致性（滑块 ↔ HTML ↔ 配置）', 'tools/check-settings.js', 0, /合计问题：(\d+)/, '问题'],
];

let failed = 0;
const rows = [];

for (const [name, script, maxResidue, pattern, unit] of steps) {
    let out = '';
    let ok = true;
    let detail = '';
    try {
        out = execFileSync(process.execPath, [path.join(root, script)], {
            cwd: root,
            encoding: 'utf8',
            stdio: ['ignore', 'pipe', 'pipe'],
            maxBuffer: 32 * 1024 * 1024,
        });
    } catch (e) {
        out = String((e && (e.stdout || e.message)) || '');
        ok = false;
        detail = '脚本本身非零退出';
    }
    const m = out.match(pattern);
    if (!m) {
        ok = false;
        if (!detail) detail = '输出里没有预期的结果行';
    } else if (maxResidue !== null) {
        const residue = Number(m[1]);
        if (Number.isFinite(residue) && residue > maxResidue) {
            ok = false;
            detail = `${unit || '残留'} ${residue} 条，超过允许的 ${maxResidue}`;
        } else {
            detail = `${unit || '残留'} ${m[1]} 条（允许 ${maxResidue}）`;
        }
    } else {
        detail = m[0].trim();
    }
    if (!ok) failed++;
    rows.push({ name, ok, detail });
    if (!quiet) {
        console.log(`${ok ? '[OK]  ' : '[FAIL]'} ${name} —— ${detail}`);
        if (!ok && out) console.log(out.trim().split('\n').slice(-12).join('\n'));
    }
}

if (quiet) {
    for (const r of rows) console.log(`${r.ok ? 'OK' : 'FAIL'}\t${r.name}\t${r.detail}`);
}
console.log('');
console.log(failed === 0 ? '前端自检全部通过' : `前端自检有 ${failed} 项失败`);
process.exit(failed === 0 ? 0 : 1);
