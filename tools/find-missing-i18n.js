// 找出 settings.html 里「含中文但英文词典没有」的文本片段（和 i18n-coverage 同口径）。
// 用法：node tools/find-missing-i18n.js
const fs = require('fs');
const path = require('path');
const root = path.resolve(__dirname, '..');
const html = fs.readFileSync(path.join(root, 'ui/settings.html'), 'utf8');
const en = fs.readFileSync(path.join(root, 'ui/i18n.en.js'), 'utf8');
const CJK = /[\u4e00-\u9fa5]/;

const items = new Map(); // key -> 展示形式
const textRe = />([^<>]*[\u4e00-\u9fa5][^<>]*)</g;
let m;
while ((m = textRe.exec(html))) {
    const t = m[1].replace(/\s+/g, ' ').trim();
    if (t) items.set(t, t);
}
const attrRe = /(title|placeholder|aria-label)="([^"]*[\u4e00-\u9fa5][^"]*)"/g;
while ((m = attrRe.exec(html))) {
    const v = m[2].replace(/\s+/g, ' ').trim();
    items.set(v, m[1] + '=' + v);
}

const missing = [];
for (const [key, label] of items) {
    // 词典是 JSON 对象字面量：键被 JSON.stringify 过，所以查转义后的形式
    const esc = JSON.stringify(key).slice(1, -1);
    if (!en.includes('"' + esc + '"')) missing.push(label);
}

console.log('含中文片段 ' + items.size + ' 条，其中缺翻译 ' + missing.length + ' 条：');
for (const s of missing) console.log('  · ' + s);
