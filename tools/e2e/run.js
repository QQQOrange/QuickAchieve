'use strict';
/* 真机 E2E 入口。
 *
 *   node tools/e2e/run.js            # 全部场景
 *   node tools/e2e/run.js away frost # 只跑指定场景
 *   node tools/e2e/run.js --list     # 列场景
 *
 * 退出码 0 = 全过。需要 Windows 桌面会话 + 已构建 target/release/QuickAchieve.exe。
 */

const path = require('node:path');

const SCENARIOS = {
    away: { file: 'away.js', desc: '鼠标移远就收起：不动不收、移开 0.35s 内收' },
    frost: { file: 'frost-static.js', desc: '静态磨砂：不抓屏、无 backdrop-filter、底板 alpha' },
    reset: { file: 'settings-reset.js', desc: '设置页每节恢复默认，磨砂强度回 12' },
};

async function main() {
    const args = process.argv.slice(2);
    if (args.includes('--list')) {
        for (const [k, v] of Object.entries(SCENARIOS)) console.log(`${k.padEnd(8)} ${v.desc}`);
        return;
    }
    const wanted = args.length ? args : Object.keys(SCENARIOS);
    for (const k of wanted) {
        if (!SCENARIOS[k]) {
            console.error(`未知场景：${k}（可选：${Object.keys(SCENARIOS).join(' / ')}）`);
            process.exitCode = 2;
            return;
        }
    }
    let failed = 0;
    for (const k of wanted) {
        const mod = require(path.join(__dirname, SCENARIOS[k].file));
        try {
            const ok = await mod();
            if (!ok) failed += 1;
        } catch (e) {
            failed += 1;
            console.error(`\n场景 ${k} 抛异常：`, e && (e.stack || e));
        }
    }
    console.log(`\nE2E 合计：${wanted.length - failed}/${wanted.length} 个场景通过`);
    process.exitCode = failed ? 1 : 0;
}

main().catch((e) => {
    console.error(e);
    process.exitCode = 1;
});
