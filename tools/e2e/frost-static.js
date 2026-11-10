'use strict';
/* 场景②：静态磨砂（v2.23.85 起）。
 *
 * 钉住：
 *   · 磨砂样式**不抓屏、不跑 WebGL、不吃 backdrop-filter**（captures=0）；
 *   · 横条背景只剩 background-color，雾面/颗粒在 ::before 上；
 *   · 底板透明度 55% → 背景 alpha 0.45；强度 12 → 有一层雾面；
 *   · 强度归 0 后 ::before 消失（不再叠雾面）。
 */

const { App, Suite, sleep } = require('./harness');

function alphaOf(color) {
    const m = /rgba?\(([^)]+)\)/.exec(String(color || ''));
    if (!m) return null;
    const parts = m[1].split(',').map((x) => Number(x.trim()));
    return parts.length >= 4 ? parts[3] : 1;
}

module.exports = async function run() {
    const s = new Suite('静态磨砂');
    const app = new App({ tag: 'frost', port: 9896 });
    let bar = null;
    try {
        await app.launch();
        bar = await app.connectBar();
        await bar.invoke('set_settings', {
            pairs: [['barGlass', false], ['barBlur', 12], ['barBgAlpha', 55]],
        });
        await sleep(400);
        await bar.invoke('show_bar', { text: '静态磨砂 E2E', atMouse: true, viaCopy: false });
        await sleep(800);

        const info = await bar.eval(`(function(){
            const b = document.querySelector('.bar');
            const cs = getComputedStyle(b);
            const ps = getComputedStyle(b, '::before');
            const g = window.QSGlass ? window.QSGlass.debugInfo() : null;
            return {
                glassOn: g ? g.on : null,
                captures: g ? g.stats.captures : null,
                bg: cs.backgroundColor,
                barImage: cs.backgroundImage,
                backdrop: cs.backdropFilter,
                pseudo: ps.backgroundImage,
            };
        })()`);

        s.ok('磨砂不再走玻璃画布', info.glassOn === false, JSON.stringify(info));
        s.ok('抓屏次数为 0', info.captures === 0, String(info.captures));
        s.ok('横条自身没有 background-image', info.barImage === 'none', info.barImage);
        s.ok('横条没有 backdrop-filter', info.backdrop === 'none', info.backdrop);
        s.ok('底板 alpha ≈ 0.45（100 - 55）', Math.abs((alphaOf(info.bg) || 0) - 0.45) < 0.02, info.bg);
        s.ok('雾面在 ::before 上（内联 SVG 噪点）', /data:image\/svg/.test(info.pseudo || ''), info.pseudo);

        await bar.invoke('set_setting', { key: 'barBlur', value: 0 });
        await sleep(500);
        const off = await bar.eval(`(function(){
            const b = document.querySelector('.bar');
            return {
                pseudo: getComputedStyle(b, '::before').backgroundImage,
                bg: getComputedStyle(b).backgroundColor,
            };
        })()`);
        s.ok('强度归 0 后不再叠雾面', off.pseudo === 'none', off.pseudo);
        s.ok('强度归 0 后底板回到实色', Math.abs((alphaOf(off.bg) || 0) - 1) < 0.02, off.bg);

        /* v2.23.90：引擎图标已转 WebP —— 打开「更多引擎」面板，确认图片真能解码。
         * 只改文件名不改内容的迁移最容易在这里翻车（404/白格）。 */
        await bar.eval("document.getElementById('btnMore').click(); 1");
        await sleep(1200);
        const icons = await bar.eval(`(function(){
            const imgs = [...document.querySelectorAll('#panelBody img')];
            return {
                imgs: imgs.length,
                loaded: imgs.filter((i) => i.complete && i.naturalWidth > 0).length,
                broken: imgs.filter((i) => i.complete && i.naturalWidth === 0).length,
            };
        })()`);
        s.ok('更多引擎面板有图标', icons.imgs > 0, JSON.stringify(icons));
        s.ok('WebP 图标全部解码成功（没有坏图）', icons.broken === 0, JSON.stringify(icons));
        s.ok('至少有一张图标真的加载完成', icons.loaded > 0, JSON.stringify(icons));
    } finally {
        if (bar) bar.close();
        await app.stop();
    }
    return s.done();
};
