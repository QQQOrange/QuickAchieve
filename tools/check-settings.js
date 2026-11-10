#!/usr/bin/env node
/* ============================================================
 *  设置项一致性检查（v2.23.89）
 * ============================================================
 *
 *  设置页有三份“同一件事”的登记：
 *    ui/settings.html  —— 滑块/开关的控件（id = `s-<key>`）
 *    ui/settings.js    —— `SLIDERS` 表（控件 id、标签 id、配置 key、默认值）
 *    src/main.rs       —— `config_view_of()` 真正下发的 camelCase key
 *
 *  三者任缺一处，用户看到的就是「滑块拖了没反应 / 值不保存 / 默认值对不上」。
 *  这个脚本只做**静态对账**，不启动应用，几毫秒跑完，适合放进 check-all。
 *
 *  用法：node tools/check-settings.js
 *  退出码：0 = 无问题；1 = 有问题（发布挡住）
 * ============================================================ */
'use strict';

const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '..');
const html = fs.readFileSync(path.join(root, 'ui/settings.html'), 'utf8');
const js = fs.readFileSync(path.join(root, 'ui/settings.js'), 'utf8');
const rs = fs.readFileSync(path.join(root, 'src/main.rs'), 'utf8');

const issues = [];
const add = (msg) => issues.push(msg);

/* ---- 1. HTML 里真实存在的控件 id ---- */
const htmlIds = new Set();
for (const m of html.matchAll(/<(?:input|select|textarea)\b[^>]*\bid="(s-[A-Za-z0-9_]+)"/g)) {
    htmlIds.add(m[1]);
}
const segIds = new Set([...html.matchAll(/<div\b[^>]*\bclass="[^"]*\bseg\b[^"]*"[^>]*\bid="(s-[A-Za-z0-9_]+)"/g)]
    .map((m) => m[1]));
for (const id of segIds) htmlIds.add(id);
const htmlAllIds = new Set([...html.matchAll(/\bid="([A-Za-z0-9_-]+)"/g)].map((m) => m[1]));
const rangeIds = new Set([...html.matchAll(/<input\b[^>]*\btype="range"[^>]*\bid="(s-[A-Za-z0-9_]+)"/g)]
    .map((m) => m[1]));

/* ---- 2. SLIDERS 表 ---- */
const sBlockStart = js.indexOf('const SLIDERS = [');
const sBlockEnd = sBlockStart < 0 ? -1 : js.indexOf('\n];', sBlockStart);
if (sBlockStart < 0 || sBlockEnd < 0) {
    add('settings.js 里找不到 `const SLIDERS = [` 块');
}
const sBlock = sBlockStart >= 0 && sBlockEnd > sBlockStart ? js.slice(sBlockStart, sBlockEnd) : '';
const sliders = [];
for (const m of sBlock.matchAll(/\[\s*'(s-[A-Za-z0-9_]+)'\s*,\s*'([A-Za-z0-9_-]+)'\s*,\s*'([A-Za-z0-9_]+)'\s*,\s*(-?\d+(?:\.\d+)?)\s*\]/g)) {
    sliders.push({ id: m[1], label: m[2], key: m[3], def: Number(m[4]) });
}
if (!sliders.length) add('SLIDERS 表解析出 0 条（正则失配？）');

/* ---- 3. config_view_of 下发的 key ---- */
const cvStart = rs.indexOf('fn config_view_of(');
const cvEnd = cvStart < 0 ? -1 : rs.indexOf('fn config_view()', cvStart);
if (cvStart < 0 || cvEnd < 0) {
    add('main.rs 里找不到 `fn config_view_of(` / `fn config_view()`');
}
const configKeys = new Set();
if (cvStart >= 0 && cvEnd > cvStart) {
    for (const m of rs.slice(cvStart, cvEnd).matchAll(/"([A-Za-z][A-Za-z0-9_]*)"\s*:/g)) {
        configKeys.add(m[1]);
    }
}
if (!configKeys.size) add('config_view_of 解析出 0 个 key');

/* ---- 4. 对账 ---- */
const seenId = new Set();
const seenKey = new Set();
for (const s of sliders) {
    if (seenId.has(s.id)) add(`SLIDERS 里控件 id 重复：${s.id}`);
    if (seenKey.has(s.key)) add(`SLIDERS 里配置 key 重复：${s.key}`);
    seenId.add(s.id);
    seenKey.add(s.key);

    if (!htmlIds.has(s.id)) add(`SLIDERS 引用了 settings.html 里不存在的控件：${s.id}`);
    else if (!rangeIds.has(s.id)) add(`SLIDERS 的控件不是 range 输入：${s.id}`);
    if (!htmlAllIds.has(s.label)) add(`SLIDERS 的标签 id 不在 settings.html 里：${s.label}`);
    if (!configKeys.has(s.key)) add(`SLIDERS 的 key 不在 config_view_of 的下发列表里：${s.key}`);
}

if (!issues.length) console.log('[OK] 设置项一致性：SLIDERS ↔ HTML ↔ config_view_of 全部对上');
else {
    for (const it of issues) console.log(`[问题] ${it}`);
}
console.log(`\n合计问题：${issues.length}`);
process.exit(issues.length ? 1 : 0);
