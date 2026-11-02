const fs = require('fs'), path = require('path');
const uiDir = path.join(process.cwd(), 'ui');
let total = 0, bare = [];
for (const f of fs.readdirSync(uiDir).filter(x => /\.(js|html)$/.test(x))) {
  const lines = fs.readFileSync(path.join(uiDir, f), 'utf8').split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    if (!/invoke\s*\(\s*['"]/.test(lines[i])) continue;
    total++;
    // 往后找 4 行，看有没有 .catch / await / try
    const win = lines.slice(i, i + 5).join('\n');
    if (/\.catch\s*\(/.test(win)) continue;
    if (/await\s+invoke/.test(win)) continue;
    if (/=\s*invoke/.test(win) && /\bthen\b/.test(win)) continue;
    bare.push(`${f}:${i + 1}  ${lines[i].trim().slice(0, 90)}`);
  }
}
console.log(`前端 invoke 调用 ${total} 处，其中 ${bare.length} 处没有 .catch / await / then`);
for (const b of bare.slice(0, 40)) console.log('  ' + b);
