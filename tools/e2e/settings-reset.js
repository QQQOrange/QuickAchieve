'use strict';
/* 场景③：设置页「恢复本节默认」（v2.23.88）。
 *
 * 钉住：
 *   · 每张卡片都装得上按钮（不依赖启动时序）；
 *   · 点磨砂卡片 → barBlur 回到磨砂样式默认 12（不是全局默认 0）；
 *   · 底板透明度回到 55；
 *   · 只提交改动过的字段（没改的 alpha 不动）。
 */

const { App, Suite, sleep } = require('./harness');

module.exports = async function run() {
    const s = new Suite('恢复本节默认');
    const app = new App({ tag: 'reset', port: 9897 });
    let bar = null;
    let set = null;
    try {
        await app.launch();
        bar = await app.connectBar();
        await bar.invoke('set_settings', {
            pairs: [['barBlur', 21], ['barBgAlpha', 0]],
        });
        await bar.invoke('open_settings');
        set = await app.connectSettings();
        await sleep(1800);

        const buttons = await set.eval("document.querySelectorAll('.card-reset').length");
        s.ok('每张卡片装了「恢复本节默认」', buttons >= 10, `实际 ${buttons} 颗`);

        const defaults = await set.invoke('defaults_view');
        s.ok('defaults_view 返回完整视图', defaults && Object.keys(defaults).length > 100,
            defaults ? `${Object.keys(defaults).length} 个字段` : 'null');

        const before = await set.invoke('get_config');
        s.ok('前置：磨砂强度 21 / 底板 0',
            before.barBlur === 21 && before.barBgAlpha === 0,
            `barBlur=${before.barBlur} barBgAlpha=${before.barBgAlpha}`);

        const clicked = await set.eval(`(function(){
            const card = document.querySelector('#s-barBlur')
                && document.querySelector('#s-barBlur').closest('.card');
            const btn = card && card.querySelector('.card-reset');
            if (!btn) return 'no-button';
            btn.click();
            return 'ok';
        })()`);
        s.ok('点到磨砂卡片上的按钮', clicked === 'ok', clicked);

        let after = null;
        for (let i = 0; i < 20; i++) {
            sleep(150);
            after = await set.invoke('get_config');
            if (after.barBlur === 12 && after.barBgAlpha === 55) break;
        }
        s.ok('磨砂强度回到样式默认 12', after && after.barBlur === 12, String(after && after.barBlur));
        s.ok('底板透明度回到 55', after && after.barBgAlpha === 55, String(after && after.barBgAlpha));
    } finally {
        if (set) set.close();
        if (bar) bar.close();
        await app.stop();
    }
    return s.done();
};
