'use strict';
/* 场景④：设置页搜索（v2.23.102）。
 *
 * 钉住用户最常用的两条路：
 *   · 输入中文关键词 → 结果列表出现，且能找到对应设置行；
 *   · 点结果 → 切到对应分页、列表收起；
 *   · 搜「导出」→ 动作条目也在结果里（命令面板轻量版）；
 *   · Esc → 结果列表收起。
 *
 * 不点「导出配置备份」本身：那会弹系统文件框，E2E 不该碰真实文件。
 */

const { App, Suite, sleep } = require('./harness');

module.exports = async function run() {
    const s = new Suite('设置页搜索');
    const app = new App({ tag: 'search', port: 9898 });
    let bar = null;
    let set = null;
    try {
        await app.launch();
        bar = await app.connectBar();
        await bar.invoke('open_settings');
        set = await app.connectSettings();
        await sleep(1800);

        const hasInput = await set.eval("!!document.getElementById('setSearch')");
        s.ok('设置页有搜索框', hasInput === true);

        // ① 中文关键词 → 结果列表出现，且包含「磨砂强度」
        const typed = await set.eval(`(function(){
            const i = document.getElementById('setSearch');
            i.focus();
            i.value = '磨砂';
            i.dispatchEvent(new Event('input', { bubbles: true }));
            return true;
        })()`);
        s.ok('输入「磨砂」', typed === true);
        await sleep(250);

        const listText = await set.eval(`(function(){
            const l = document.getElementById('setSearchList');
            return { hidden: l.hidden, text: l.textContent.slice(0, 400) };
        })()`);
        s.ok('结果列表出现', listText && listText.hidden === false,
            JSON.stringify(listText));
        s.ok('结果里有「磨砂强度」', /磨砂强度/.test((listText && listText.text) || ''),
            (listText && listText.text || '').slice(0, 80));

        // ② 点第一条结果 → 跳到对应分页、列表收起
        const clicked = await set.eval(`(function(){
            const first = document.querySelector('#setSearchList .find-item');
            if (!first) return 'no-item';
            first.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
            return 'ok';
        })()`);
        s.ok('点搜索结果', clicked === 'ok', clicked);
        await sleep(350);

        const after = await set.eval(`(function(){
            const open = [...document.querySelectorAll('.pane[data-pane]')]
                .filter((p) => !p.hidden)
                .map((p) => p.dataset.pane);
            /* 正确判据：当前可见分页里确实有 #s-barBlur（磨砂强度），
             * 而不是猜分页 key —— 设置项分组以后可能改名。 */
            const hasTarget = !!document.querySelector(
                '.pane[data-pane]:not([hidden]) #s-barBlur');
            return { open, hasTarget,
                listHidden: document.getElementById('setSearchList').hidden };
        })()`);
        s.ok('跳到了磨砂强度所在分页', after && after.hasTarget === true,
            JSON.stringify(after));
        s.ok('结果列表收起', after && after.listHidden === true,
            JSON.stringify(after));

        // ③ 搜「导出」→ 动作条目出现
        await set.eval(`(function(){
            const i = document.getElementById('setSearch');
            i.focus();
            i.value = '导出';
            i.dispatchEvent(new Event('input', { bubbles: true }));
        })()`);
        await sleep(250);
        const actText = await set.eval(
            "document.getElementById('setSearchList').textContent.slice(0, 400)");
        s.ok('动作「导出配置备份」可被搜到', /导出配置备份/.test(actText || ''),
            (actText || '').slice(0, 100));

        // ④ Esc 收起
        const esc = await set.eval(`(function(){
            const i = document.getElementById('setSearch');
            i.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
            return document.getElementById('setSearchList').hidden;
        })()`);
        s.ok('Esc 收起结果列表', esc === true, String(esc));
    } finally {
        if (set) set.close();
        if (bar) bar.close();
        await app.stop();
    }
    return s.done();
};
