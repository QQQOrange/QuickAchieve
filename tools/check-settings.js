#!/usr/bin/env node
/* ============================================================
 *  设置项一致性检查（v2.23.89）
 * ============================================================
 *
 *  设置页有四处“同一件事”的登记：
 *    ui/settings.html      —— 控件（id = `s-<key>`）
 *    ui/settings.js        —— `SLIDERS` 表、各种开关映射表
 *    src/settings_view.rs  —— `config_view_of()` 真正下发的 camelCase key
 *    tools/settings-schema.json —— 控件 key 与配置 key 不一致时的显式映射
 *
 *  这个脚本做静态对账（不启动应用）：
 *    · SLIDERS 的控件/标签/key 是否存在；
 *    · 每个 `s-*` 控件要么直接是 config_view_of 的 key，要么在 aliases，
 *      要么在 external 里写明“为什么不在配置视图里”（本地状态/专用命令）；
 *    · aliases 的目标 key 必须真的存在；external 登记项必须有对应控件。
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
const view = fs.readFileSync(path.join(root, 'src/settings_view.rs'), 'utf8');
const schema = JSON.parse(fs.readFileSync(path.join(root, 'tools/settings-schema.json'), 'utf8'));

const issues = [];
const add = (msg) => issues.push(msg);

/* ---- 1. HTML 控件 ---- */
const htmlAllIds = new Set([...html.matchAll(/\bid="([A-Za-z0-9_-]+)"/g)].map((m) => m[1]));
const controls = new Map(); // id → type
for (const m of html.matchAll(/<(input|select|textarea)\b([^>]*)>/g)) {
    const attrs = m[2];
    const id = (/id="(s-[A-Za-z0-9_-]+)"/.exec(attrs) || [])[1];
    if (!id) continue;
    const type = (/type="([a-z]+)"/.exec(attrs) || [])[1] || m[1];
    controls.set(id, type);
}
for (const m of html.matchAll(/<div\b[^>]*class="[^"]*\bseg\b[^"]*"[^>]*id="(s-[A-Za-z0-9_-]+)"/g)) {
    controls.set(m[1], 'seg');
}
const rangeIds = new Set([...controls].filter(([, t]) => t === 'range').map(([id]) => id));

/* ---- 2. SLIDERS 表 ---- */
const sStart = js.indexOf('const SLIDERS = [');
const sEnd = sStart < 0 ? -1 : js.indexOf('\n];', sStart);
if (sStart < 0 || sEnd < 0) add('settings.js 里找不到 `const SLIDERS = [` 块');
const sBlock = sStart >= 0 && sEnd > sStart ? js.slice(sStart, sEnd) : '';
const sliders = [];
for (const m of sBlock.matchAll(/\[\s*'(s-[A-Za-z0-9_-]+)'\s*,\s*'([A-Za-z0-9_-]+)'\s*,\s*'([A-Za-z0-9_]+)'\s*,\s*(-?\d+(?:\.\d+)?)\s*\]/g)) {
    sliders.push({ id: m[1], label: m[2], key: m[3], def: Number(m[4]) });
}
if (!sliders.length) add('SLIDERS 表解析出 0 条（正则失配？）');

/* ---- 3. config_view_of 下发的 key ---- */
const cvStart = view.indexOf('fn config_view_of(');
const cvEnd = cvStart < 0 ? -1 : view.indexOf('\n}', cvStart) + 2;
if (cvStart < 0 || cvEnd < 0) add('settings_view.rs 里找不到 `fn config_view_of(` 的函数体');
const configKeys = new Set();
if (cvStart >= 0 && cvEnd > cvStart) {
    for (const m of view.slice(cvStart, cvEnd).matchAll(/"([A-Za-z][A-Za-z0-9_]*)"\s*:/g)) configKeys.add(m[1]);
}
if (!configKeys.size) add('config_view_of 解析出 0 个 key');

/* ---- 4. SLIDERS 对账 ---- */
const seenId = new Set();
const seenKey = new Set();
for (const s of sliders) {
    if (seenId.has(s.id)) add(`SLIDERS 里控件 id 重复：${s.id}`);
    if (seenKey.has(s.key)) add(`SLIDERS 里配置 key 重复：${s.key}`);
    seenId.add(s.id);
    seenKey.add(s.key);
    if (!controls.has(s.id)) add(`SLIDERS 引用了 settings.html 里不存在的控件：${s.id}`);
    else if (!rangeIds.has(s.id)) add(`SLIDERS 的控件不是 range 输入：${s.id}`);
    if (!htmlAllIds.has(s.label)) add(`SLIDERS 的标签 id 不在 settings.html 里：${s.label}`);
    if (!configKeys.has(s.key)) add(`SLIDERS 的 key 不在 config_view_of 的下发列表里：${s.key}`);
}

/* ---- 5. 每个控件都必须有“归宿” ---- */
const aliases = schema.aliases || {};
const external = schema.external || {};
for (const [id] of controls) {
    const key = id.slice(2);
    if (configKeys.has(key)) continue;
    if (aliases[key] && configKeys.has(aliases[key])) continue;
    if (external[key]) continue;
    add(`控件 ${id} 未登记：不在 config_view_of，也不在 settings-schema.json 的 aliases/external 里`);
}

/* ---- 6. 映射表自身不能腐坏 ---- */
for (const [from, to] of Object.entries(aliases)) {
    if (!configKeys.has(to)) add(`aliases.${from} → ${to}：目标不在 config_view_of 里`);
    if (!controls.has(`s-${from}`)) add(`aliases.${from}：settings.html 里没有控件 s-${from}`);
}
for (const [key, why] of Object.entries(external)) {
    if (!controls.has(`s-${key}`)) add(`external.${key}：settings.html 里没有控件 s-${key}`);
    if (!why || typeof why !== 'string') add(`external.${key}：必须写一句“为什么不在配置视图里”`);
}

if (!issues.length) {
    console.log(`[OK] 设置项一致性：${controls.size} 个控件 / ${sliders.length} 个滑块 / ${configKeys.size} 个配置 key 全部对上`);
} else {
    for (const it of issues) console.log(`[问题] ${it}`);
}
console.log(`\n合计问题：${issues.length}`);
process.exit(issues.length ? 1 : 0);
