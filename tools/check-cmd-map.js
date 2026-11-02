const fs = require('fs'), path = require('path');
const root = process.cwd();
// 1) 后端命令名
const rs = fs.readFileSync(path.join(root,'src/main.rs'),'utf8').split(/\r?\n/);
const cmds = new Set();
for (let i=0;i<rs.length;i++) {
  if (/^\s*#\[crate::t::command\]/.test(rs[i])) {
    // 属性和注释都可能夹在中间（layout_bar 就是这样），往后找几行
    for (let j=i+1;j<Math.min(i+9,rs.length);j++) {
      const m = rs[j].match(/^\s*(?:pub\s+)?(?:async\s+)?fn\s+([a-z0-9_]+)/);
      if (m) { cmds.add(m[1]); break; }
    }
  }
}
// 2) 前端调用的命令名
const uiDir = path.join(root,'ui');
const files = fs.readdirSync(uiDir).filter(f=>/\.(js|html)$/.test(f));
const used = new Map();
for (const f of files) {
  const s = fs.readFileSync(path.join(uiDir,f),'utf8');
  for (const m of s.matchAll(/invoke\(\s*['"]([a-zA-Z0-9_]+)['"]/g)) {
    if (!used.has(m[1])) used.set(m[1], new Set());
    used.get(m[1]).add(f);
  }
}
const missing = [...used.keys()].filter(k=>!cmds.has(k)).sort();
console.log('后端命令 ' + cmds.size + ' 个；前端 invoke 了 ' + used.size + ' 个不同命令');
console.log('');
console.log('=== 前端调用但后端没有（点了会失败）===');
if (!missing.length) console.log('  无');
for (const k of missing) console.log('  ' + k + '  ← ' + [...used.get(k)].join(', '));

const unused = [...cmds].filter(c=>!used.has(c)).sort();
console.log('');
console.log('=== 后端有、前端没直接 invoke（可能由别处调用，仅供参考）===');
console.log('  ' + unused.length + ' 个: ' + unused.slice(0,40).join(', '));

// 缺一个 = 界面上有个按钮点了不会动，发布前就该拦下来
if (missing.length) {
  console.log('');
  console.log('[FAIL] 有 ' + missing.length + ' 个前端调用没有对应后端命令');
  process.exit(1);
}
console.log('');
console.log('[OK] 前端 invoke 与后端命令表一致');
