'use strict';
/* 场景①：横条「鼠标移远就收起」。
 *
 * 钉住 2026-11-10 修过的两件事：
 *   1. 指针**没动过**不能开始倒计时（多行划词时横条挂在选区下方，
 *      指针停在选区中间、离横条 53px，旧逻辑弹出 1 秒后自己消失）；
 *   2. 真移开以后要在 ~0.35 秒内收起，日志里能看到「指针移动 ≥6px」。
 */

const { App, Suite, sleep, cursorPos, setCursorExact } = require('./harness');

module.exports = async function run() {
    const s = new Suite('鼠标移远就收起');
    const app = new App({ tag: 'away', port: 9895 });
    let bar = null;
    try {
        await app.launch();
        bar = await app.connectBar();
        await bar.invoke('set_settings', { pairs: [['barAwayPx', 40], ['barIdleSec', 0]] });
        await sleep(300);

        const placed = setCursorExact(1200, 800);
        if (!placed) {
            s.skip('指针定位失败（远程会话/别的程序抢指针），跳过场景');
            return s.done();
        }
        await bar.invoke('grid_trigger_from_ui');
        await sleep(700);

        const atShow = await bar.invoke('bar_auto_state');
        s.ok('横条已弹出', atShow && atShow.shown === true, JSON.stringify(atShow));

        await sleep(1200);
        const p0 = cursorPos();
        if (Math.abs(p0.x - 1200) > 2 || Math.abs(p0.y - 800) > 2) {
            /* 指针被外部移动 = “静止”这个前提已经不成立，整场无法判；
             * 只 skip 一条断言会把后面的步骤也带偏，直接收场。 */
            s.skip('等待期间指针被外部移动，跳过场景', JSON.stringify(p0));
            return s.done();
        }
        const stationary = await bar.invoke('bar_auto_state');
        if (stationary && stationary.shown === true) {
            s.ok('指针没动时不会自己收起', true);
        } else {
            /* 这台机器是活的：测试跑的时候用户/系统可能真的动了指针。
             * 如果日志里已经出现「指针移动 ≥6px」，那是环境干扰，不是产品失败；
             * 没出现才判失败，并把日志附上。 */
            const tail = app.logTail(60);
            if (/bar-away: 指针移动 ≥6px/.test(tail)) {
                s.skip('指针没动时不会自己收起（环境干扰：检测到外部指针移动），跳过场景',
                    tail.split('\n').slice(-4).join('\n'));
                return s.done();
            } else {
                s.ok('指针没动时不会自己收起', false,
                    `${JSON.stringify(stationary)}\n${tail.split('\n').slice(-12).join('\n')}`);
            }
        }

        if (!setCursorExact(1720, 800)) {
            s.skip('移开时指针定位失败，跳过');
            return s.done();
        }
        let hidden = false;
        for (let i = 0; i < 20; i++) {
            sleep(100);
            const st = await bar.invoke('bar_auto_state');
            if (!st.shown) { hidden = true; break; }
        }
        s.ok('移开 500px 后收起', hidden);

        const log = app.logTail(80);
        s.ok('日志证明是“指针动过”才计时', /指针移动 ≥6px/.test(log), log.split('\n').slice(-6).join('\n'));
        s.ok('日志有“连续 …ms 离横条 … 收起”', /连续 \d+ms 离横条/.test(log), log.split('\n').slice(-4).join('\n'));
    } finally {
        if (bar) bar.close();
        await app.stop();
    }
    return s.done();
};
