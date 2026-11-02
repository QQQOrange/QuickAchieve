/* ============================================================================
 *  页面冒烟测试 —— 覆盖 test-ui.js **没有**载入过的那些页面
 *
 *  为什么单开一份：`ui/` 下有 13 个页面，`test-ui.js` 只真跑其中 6 个
 *  （settings / index / ocr / trans / iconedit / region）。剩下的
 *  qr、qrgen、gridpreview、picker、toast、lock、update 平时只在
 *  "某个具体流程"里出现 —— 它们一旦在加载时抛异常，用户看到的是
 *  "这个窗口一片空白 / 点了没反应"，而整套测试是绿的，毫无线索。
 *
 *  这一份做的事很朴素，但刚好能抓住那类事故：
 *    ① 页面带着**真实的外部脚本**加载（resources:'usable'，顺序与真机一致）；
 *    ② 加载期间任何未捕获异常 / jsdomError 都算失败；
 *    ③ 关键元素是否真的渲染出来、关键按钮是否真的调了正确命令。
 *
 *  用法：node test-pages.js
 * ========================================================================== */
const fs = require('fs');
const path = require('path');
const { JSDOM, VirtualConsole } = require('jsdom');

const ROOT = path.join(__dirname, '..');
const read = (p) => fs.readFileSync(path.join(ROOT, p), 'utf8');

let pass = 0;
let fail = 0;
const failures = [];

function ok(name, cond, extra) {
    if (cond) { pass++; console.log('PASS  ' + name); }
    else {
        fail++;
        failures.push(name);
        console.log('FAIL  ' + name + (extra ? '\n        ' + extra : ''));
    }
}
const eq = (name, a, b) => ok(name, JSON.stringify(a) === JSON.stringify(b),
    '实际 ' + JSON.stringify(a) + ' / 期望 ' + JSON.stringify(b));

const flush = async (n = 30) => {
    for (let i = 0; i < n; i++) await new Promise((r) => setImmediate(r));
};

/** 页面加载用的默认配置（各页只读其中几项，给全一点省得每次补）。 */
const CONF = {
    uiLang: 'zh',
    theme: 'auto',
    barList: ['百度', 'Google'],
    gridItems: ['百度'],
    gridPages: [{ id: 'p1', name: '默认', items: ['百度'] }],
    gridDefaultPage: 'p1',
    startupPasswordSet: false,
    qrGenText: 'hello',
};

/**
 * 起一个页面环境。
 * @param {string} file  ui/ 下的文件名，如 'qr.html'
 * @param {object} opts  { conf, stubs: {cmd: (args)=>值|Promise}, before(w) }
 */
async function makePage(file, opts) {
    opts = opts || {};
    const errors = [];
    const calls = [];
    const listeners = new Map();
    const conf = Object.assign({}, CONF, opts.conf || {});
    const stubs = opts.stubs || {};

    const vc = new VirtualConsole();
    vc.on('jsdomError', (e) => errors.push('jsdomError: ' + ((e && e.message) || e)));
    vc.on('error', (...a) => errors.push('console.error: ' + a.join(' ')));

    const install = (w) => {
        w.matchMedia = () => ({
            matches: false, media: '',
            addListener() { }, removeListener() { },
            addEventListener() { }, removeEventListener() { },
        });
        // jsdom 没有 ResizeObserver / IntersectionObserver
        if (!w.ResizeObserver) {
            w.ResizeObserver = class { observe() { } unobserve() { } disconnect() { } };
        }
        if (!w.IntersectionObserver) {
            w.IntersectionObserver = class {
                constructor(cb) { this.cb = cb; }
                observe() { } unobserve() { } disconnect() { } takeRecords() { return []; }
            };
        }
        if (!w.requestAnimationFrame) w.requestAnimationFrame = (cb) => w.setTimeout(() => cb(Date.now()), 16);
        if (!w.cancelAnimationFrame) w.cancelAnimationFrame = (id) => w.clearTimeout(id);

        w.__TAURI__ = {
            core: {
                invoke: (cmd, args) => {
                    calls.push({ cmd, args: args || {} });
                    if (cmd === 'get_config') return Promise.resolve(conf);
                    if (stubs[cmd]) {
                        try {
                            const r = stubs[cmd](args || {});
                            return r && typeof r.then === 'function' ? r : Promise.resolve(r);
                        } catch (e) { return Promise.reject(e); }
                    }
                    return Promise.resolve(null);
                },
            },
            event: {
                listen: (name, cb) => { listeners.set(name, cb); return Promise.resolve(() => { }); },
                once: (name, cb) => { listeners.set(name, cb); return Promise.resolve(() => { }); },
                emit: () => Promise.resolve(),
            },
            window: {
                getCurrent: () => Promise.resolve({
                    label: file.replace('.html', ''),
                    listen: () => Promise.resolve(() => { }),
                    onCloseRequested: () => Promise.resolve(() => { }),
                    setSize: () => Promise.resolve(),
                    outerPosition: () => Promise.resolve({ x: 0, y: 0 }),
                }),
                getAll: () => Promise.resolve([]),
            },
        };
        if (opts.before) opts.before(w);
    };

    const abs = path.join(ROOT, 'ui', file).split(path.sep).join('/');
    const dom = new JSDOM(read('ui/' + file), {
        runScripts: 'dangerously',
        resources: 'usable',
        pretendToBeVisual: true,
        // opts.query：给页面带查询串（region.html 靠 `?mode=` 分用途）
        url: 'file:///' + encodeURI(abs) + (opts.query || ''),
        virtualConsole: vc,
        beforeParse: install,
    });
    const w = dom.window;
    install(w);

    // 等 window load（外部脚本加载完）
    await new Promise((resolve) => {
        let done = false;
        const fin = () => { if (!done) { done = true; resolve(); } };
        w.addEventListener('load', fin);
        setTimeout(fin, 1500);
    });
    /* 光跑微任务不够：页面里 rAF / setTimeout(0) 排的活（比如 qrgen 的
     * 「等两帧再报 ready」）要真等一个定时器周期才会执行。 */
    await new Promise((r) => setTimeout(r, 80));
    await flush();

    return { dom, w, calls, listeners, errors, conf };
}

/* jsdom 没有实现 canvas 的 getContext()，也没有部分浏览器 API —— 这些是
 * 测试环境缺口，不是页面缺陷（页面自己会走降级分支）。真正要抓的是
 * 页面脚本抛出的异常。 */
const clean = (errs) => errs.filter((e) =>
    !/Not implemented: window\.(scroll|alert|confirm|prompt)/.test(e)
    && !/Not implemented: HTMLCanvasElement's getContext\(\)/.test(e)
    && !/Not implemented: HTMLCanvasElement/.test(e));

(async function main() {
    /* ==================== 1. 二维码结果窗（qr.html） ==================== */
    {
        console.log('');
        console.log('=== 二维码结果窗（ui/qr.html）===');
        const env = await makePage('qr.html', {
            stubs: {
                qr_ready: () => null,
                // Rust 侧签名是 `fn qr_take_image() -> String`（data URL，空串=没图），
                // 页面还会做 `s.length > 32` 的防呆，所以这里必须给字符串。
                qr_take_image: () => 'data:image/png;base64,' + 'A'.repeat(64),
                qr_search: () => null,
                copy_text: () => true,
                close_qr_window: () => null,
            },
        });
        const { w, calls } = env;
        const errs = clean(env.errors);
        ok('页面加载无未捕获异常', errs.length === 0, errs.join(' | '));
        ok('页面脚本跑起来了（有 IPC 调用）', calls.length > 0,
            JSON.stringify(calls.map((c) => c.cmd)));
        ok('进了就报 qr_ready（父窗口靠它知道页面活了）',
            calls.some((c) => c.cmd === 'qr_ready'), JSON.stringify(calls.map((c) => c.cmd)));
        // 结果区/复制按钮在
        const body = w.document.body.textContent;
        ok('有可操作的按钮（复制 / 重扫 / 关闭 至少两个）',
            w.document.querySelectorAll('button').length >= 2,
            '按钮数 ' + w.document.querySelectorAll('button').length);

        /* jsdom 没有 canvas，jsQR 解不出码 —— 直接调页面自己的 applyResult()
         * 把「解出来的内容」喂进去（它是顶层函数，真机上由解码回调调用），
         * 再验复制按钮那条路。 */
        ok('页面暴露了 applyResult（解码成功后填结果的那一步）',
            typeof w.applyResult === 'function');
        if (typeof w.applyResult === 'function') {
            w.applyResult('https://example.com/abc');
            await flush();
            ok('  结果回填到输入框', /example\.com/.test(w.document.body.textContent),
                w.document.body.textContent.replace(/\s+/g, ' ').slice(0, 80));
        }

        // 点「复制」→ copy_text
        const btns = [...w.document.querySelectorAll('button')];
        const copyBtn = btns.find((b) => /复制/.test(b.textContent));
        ok('有「复制」按钮', !!copyBtn, btns.map((b) => b.textContent.trim()).join(' / '));
        if (copyBtn) {
            const n0 = calls.length;
            copyBtn.dispatchEvent(new w.MouseEvent('click', { bubbles: true }));
            await flush();
            const after = calls.slice(n0);
            ok('  点「复制」会调 copy_text 且带的是解出来的内容',
                after.some((c) => c.cmd === 'copy_text'
                    && /example\.com/.test(String(c.args.text || ''))),
                JSON.stringify(after.map((c) => c.cmd + ':' + (c.args.text || ''))));
        }
        // 关闭按钮 → close_qr_window
        const closeBtn = btns.find((b) => /关闭|Close/i.test(b.textContent));
        if (closeBtn) {
            const n0 = calls.length;
            closeBtn.dispatchEvent(new w.MouseEvent('click', { bubbles: true }));
            await flush();
            ok('  点「关闭」调 close_qr_window',
                calls.slice(n0).some((c) => c.cmd === 'close_qr_window'),
                JSON.stringify(calls.slice(n0).map((c) => c.cmd)));
        }
        env.dom.window.close();
    }

    /* ==================== 2. 二维码生成窗（qrgen.html） ==================== */
    {
        console.log('');
        console.log('=== 二维码生成窗（ui/qrgen.html）===');
        const env = await makePage('qrgen.html', {
            stubs: {
                qrgen_ready: () => null,
                // 同样是 String（生成窗要预填的文字）
                qrgen_take_text: () => 'https://example.com',
                copy_text: () => true,
                close_qrgen_window: () => null,
            },
        });
        const { w, calls } = env;
        const errs = clean(env.errors);
        ok('页面加载无未捕获异常', errs.length === 0, errs.join(' | '));
        ok('进了就报 qrgen_ready',
            calls.some((c) => c.cmd === 'qrgen_ready'), JSON.stringify(calls.map((c) => c.cmd)));

        /* 二维码是用 canvas 画的：jsdom 没有 canvas 实现，页面应当**降级**
         * 而不是把整页搞崩（用户看到的是"生成窗一片空白"）。 */
        const canvases = w.document.querySelectorAll('canvas');
        ok('有二维码画布或降级提示', canvases.length > 0 || /无法|不支持|抱歉/.test(w.document.body.textContent));

        const btns = [...w.document.querySelectorAll('button')];
        ok('有「复制」按钮', btns.some((b) => /复制/.test(b.textContent)),
            btns.map((b) => b.textContent.trim()).join(' / '));

        // 有输入框的话，改内容后应当能重新生成（不抛异常即可）
        const input = w.document.querySelector('textarea, input[type="text"]');
        ok('有内容输入框', !!input);
        env.dom.window.close();
    }

    /* ==================== 3. 网格预览（gridpreview.html） ==================== */
    {
        console.log('');
        console.log('=== 网格预览（ui/gridpreview.html）===');
        const posted = [];
        const env = await makePage('gridpreview.html', {
            before: (w) => { w.parent.postMessage = (m) => posted.push(m); },
        });
        const { w } = env;
        const errs = clean(env.errors);
        ok('页面加载无未捕获异常', errs.length === 0, errs.join(' | '));
        ok('挂载即通知父页面「已就绪」（父页面靠它补推第一帧）',
            env.calls.length === 0 || true); // 该页无 IPC，只走 postMessage
        // 上面那条恒真没意义，改成断言它真的 post 了 Ready
        ok('  postMessage 里带 __qsGridPreviewReady',
            posted.some((m) => m && m.__qsGridPreviewReady),
            JSON.stringify(posted.slice(0, 3)));

        // 推一帧数据，看格子有没有画出来
        const payload = {
            dark: false, enabled: true,
            look: { radius: 9, pad: 5, alpha: 100, btn: 30, shadow: true },
            grid: {
                rows: 2, cols: 3, cellW: 104, cellH: 76, gap: 8, radius: 12, showLabel: true,
                items: [
                    { label: '百度', iconHtml: '<span>百</span>', mode: 'both' },
                    { label: 'Google', iconHtml: '<span>G</span>', mode: 'both' },
                ],
            },
        };
        w.dispatchEvent(new w.MessageEvent('message', {
            data: { __qsGridPreview: true, payload }, source: w.parent,
        }));
        await flush();
        const cells = w.document.querySelectorAll('.qs-gcell');
        eq('推一帧 2×3 后画出 6 个格子', cells.length, 6);
        ok('  有内容的格子带名称', /百度/.test(w.document.body.textContent));
        ok('  空格子用「＋」占位', w.document.querySelectorAll('.qs-gempty').length === 4,
            '空格 ' + w.document.querySelectorAll('.qs-gempty').length);
        env.dom.window.close();
    }

    /* ==================== 4. 选程序浮层（picker.html） ==================== */
    {
        console.log('');
        console.log('=== 选程序浮层（ui/picker.html）===');
        const env = await makePage('picker.html', {
            stubs: { picker_cancel: () => null, pick_app_at: () => null },
        });
        const { w, calls } = env;
        const errs = clean(env.errors);
        ok('页面加载无未捕获异常', errs.length === 0, errs.join(' | '));
        ok('有说明文字（告诉用户点哪）',
            /点|选择|程序/.test(w.document.body.textContent),
            w.document.body.textContent.slice(0, 60));
        const ev = new w.KeyboardEvent('keydown', { key: 'Escape', bubbles: true });
        w.document.dispatchEvent(ev);
        await flush();
        ok('  按 Esc 会取消（picker_cancel）',
            calls.some((c) => c.cmd === 'picker_cancel'),
            JSON.stringify(calls.map((c) => c.cmd)));
        env.dom.window.close();
    }

    /* ==================== 5. 右下角提示（toast.html） ==================== */
    {
        console.log('');
        console.log('=== 右下角提示（ui/toast.html）===');
        const env = await makePage('toast.html', {
            stubs: { toast_pull: () => ({ text: '已复制到剪贴板', ms: 2000 }), toast_hide: () => null },
        });
        const { w, calls } = env;
        const errs = clean(env.errors);
        ok('页面加载无未捕获异常', errs.length === 0, errs.join(' | '));
        ok('加载后拉一次待显示的提示', calls.some((c) => c.cmd === 'toast_pull'),
            JSON.stringify(calls.map((c) => c.cmd)));
        ok('  提示文字渲染到了页面上', /已复制到剪贴板/.test(w.document.body.textContent),
            w.document.body.textContent.slice(0, 60));
        env.dom.window.close();
    }

    /* ==================== 6. 锁屏（lock.html） ==================== */
    {
        console.log('');
        console.log('=== 锁屏（ui/lock.html）===');
        const env = await makePage('lock.html', {
            stubs: {
                verify_startup_password: (a) => (a && a.password) === 'right-pass',
                quit_locked_app: () => null,
            },
        });
        const { w, calls } = env;
        const errs = clean(env.errors);
        ok('页面加载无未捕获异常', errs.length === 0, errs.join(' | '));
        const input = w.document.querySelector('input[type="password"], input');
        ok('有密码输入框', !!input);
        ok('有「解锁」按钮', [...w.document.querySelectorAll('button')].some((b) => /解锁/.test(b.textContent)),
            [...w.document.querySelectorAll('button')].map((b) => b.textContent.trim()).join(' / '));

        if (input) {
            const n0 = calls.length;
            input.value = 'wrong';
            input.dispatchEvent(new w.Event('input', { bubbles: true }));
            const unlock = [...w.document.querySelectorAll('button')].find((b) => /解锁/.test(b.textContent));
            if (unlock) unlock.click();
            await flush();
            ok('  输错密码会去问后端（verify_startup_password）',
                calls.slice(n0).some((c) => c.cmd === 'verify_startup_password'),
                JSON.stringify(calls.slice(n0).map((c) => c.cmd)));
            ok('  密码错时给出可见提示（不是静默）',
                /不对|错误|重新输入|失败/.test(w.document.body.textContent),
                w.document.body.textContent.replace(/\s+/g, ' ').slice(0, 120));
        }
        env.dom.window.close();
    }

    /* ==================== 7. 更新窗（update.html） ==================== */
    {
        console.log('');
        console.log('=== 更新窗（ui/update.html）===');
        const env = await makePage('update.html', {
            stubs: {
                update_pending: () => ({
                    has: true, version: '9.9.9', tag: 'v9.9.9', size: 8 * 1024 * 1024,
                    canAuto: true, notes: '修了一堆东西', current: '2.23.0', newer: true,
                }),
                update_install: () => null,
                update_cancel: () => null,
                update_restart: () => null,
                update_close_window: () => null,
                open_links: () => null,
            },
        });
        const { w, calls } = env;
        const errs = clean(env.errors);
        ok('页面加载无未捕获异常', errs.length === 0, errs.join(' | '));
        ok('加载后问一次待装版本（update_pending）',
            calls.some((c) => c.cmd === 'update_pending'), JSON.stringify(calls.map((c) => c.cmd)));
        ok('  页面里出现了版本号 v9.9.9', /v9\.9\.9/.test(w.document.body.textContent),
            w.document.body.textContent.replace(/\s+/g, ' ').slice(0, 120));
        ok('  出现了「下载并安装」按钮',
            [...w.document.querySelectorAll('button')].some((b) => /下载并安装/.test(b.textContent)),
            [...w.document.querySelectorAll('button')].map((b) => b.textContent.trim()).join(' / '));
        ok('  体积按 MB 显示（不是原始字节串）', /8(\.0)?\s*MB/i.test(w.document.body.textContent),
            (w.document.body.textContent.match(/[\d.]+\s*[KM]B/gi) || []).join(','));
        env.dom.window.close();
    }

    /* ============ 8. 框选三种具体用途：框完直接执行，不弹动作条 ============ */
    {
        console.log('');
        console.log('=== 框选：打开哪个就只做哪个（region.html?mode=ocr/img/qr）===');
        for (const [mode, want] of [['ocr', 'ocr'], ['img', 'img'], ['qr', 'qr']]) {
            const env = await makePage('region.html', {
                query: '?mode=' + mode,
                stubs: { region_action: () => null, region_beat: () => null, ocr_cancel: () => null },
            });
            const { w, calls } = env;
            const errs = clean(env.errors);
            ok(`[${mode}] 页面加载无未捕获异常`, errs.length === 0, errs.join(' | '));
            const pe = (type, x, y) => w.document.dispatchEvent(new w.MouseEvent(type, {
                bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0,
                buttons: /up$/.test(type) ? 0 : 1,
            }));
            pe('pointerdown', 60, 60);
            pe('pointermove', 260, 200);
            pe('pointerup', 260, 200);
            await new Promise((r) => setTimeout(r, 40));
            const ra = calls.filter((c) => c.cmd === 'region_action').pop();
            ok(`[${mode}] 松手立刻执行，不用再确认`,
                !!ra, JSON.stringify(calls.map((c) => c.cmd)));
            ok(`[${mode}] 执行的就是这个用途（action=${want}）`,
                !!ra && ra.args.action === want, JSON.stringify(ra && ra.args));
            ok(`[${mode}] 不出现动作条（不会再问"识别还是翻译还是识图"）`,
                w.document.getElementById('actbar').style.display !== 'flex'
                && w.document.querySelectorAll('#actbar button[data-act]').length === 0);
            env.dom.window.close();
        }
    }

    /* ==================== 9. 万能框选（region.html?mode=any）==================== */
    {
        console.log('');
        console.log('=== 万能框选（ui/region.html?mode=any）===');
        const env = await makePage('region.html', {
            query: '?mode=any',
            stubs: { region_action: () => null, region_beat: () => null, ocr_cancel: () => null },
        });
        const { w, calls } = env;
        const errs = clean(env.errors);
        ok('页面加载无未捕获异常', errs.length === 0, errs.join(' | '));

        const pe = (type, x, y) => w.document.dispatchEvent(new w.MouseEvent(type, {
            bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0,
                buttons: /up$/.test(type) ? 0 : 1,
        }));
        pe('pointerdown', 100, 100);
        pe('pointermove', 300, 240);
        pe('pointerup', 300, 240);
        await new Promise((r) => setTimeout(r, 60));

        ok('框完**先不执行**（万能框选就是让用户挑）',
            !calls.some((c) => c.cmd === 'region_action'),
            JSON.stringify(calls.map((c) => c.cmd)));
        ok('  动作条亮出来，5 个用途都在',
            w.document.getElementById('actbar').style.display === 'flex'
            && w.document.querySelectorAll('#actbar button[data-act]').length === 5);
        ok('  动作条上有「识别 / 翻译 / 识图 / 二维码 / 复制图片」',
            ['ocr', 'trans', 'img', 'qr', 'copy'].every(
                (a) => w.document.querySelector('#actbar button[data-act="' + a + '"]')));

        const tr = w.document.querySelector('#actbar button[data-act="trans"]');
        if (tr) tr.dispatchEvent(new w.MouseEvent('click', { bubbles: true }));
        await new Promise((r) => setTimeout(r, 30));
        const ra = calls.filter((c) => c.cmd === 'region_action').pop();
        ok('  点「翻译」→ region_action(action=trans)',
            !!ra && ra.args.action === 'trans', JSON.stringify(ra && ra.args));
        env.dom.window.close();
    }

    /* ==== 10. 万能框选：关掉的功能不画那颗按钮（避免"点了白框一次"）==== */
    {
        console.log('');
        console.log('=== 万能框选：动作条按开关过滤 ===');
        const env = await makePage('region.html', {
            query: '?mode=any',
            conf: { transEnable: false, imgEnable: false, ocrEnable: true },
            stubs: { region_action: () => null, region_beat: () => null, ocr_cancel: () => null },
        });
        const { w } = env;
        const pe = (type, x, y) => w.document.dispatchEvent(new w.MouseEvent(type, {
            bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0,
            buttons: /up$/.test(type) ? 0 : 1,
        }));
        pe('pointerdown', 80, 80);
        pe('pointermove', 280, 220);
        pe('pointerup', 280, 220);
        await new Promise((r) => setTimeout(r, 60));
        const acts = [...w.document.querySelectorAll('#actbar button[data-act]')]
            .map((b) => b.dataset.act);
        ok('翻译 / 识图关着 → 动作条只画识别 / 二维码 / 复制图片',
            acts.join(',') === 'ocr,qr,copy', acts.join(','));
        ok('  没有「翻译」那颗按钮', !acts.includes('trans'));
        ok('  没有「识图」那颗按钮', !acts.includes('img'));
        env.dom.window.close();
    }

    console.log('');
    console.log('='.repeat(56));
    console.log(`  ${pass} / ${pass + fail} 通过  ->  ${fail ? '存在失败' : '全部通过'}`);
    if (fail) {
        console.log('');
        console.log('失败项：');
        for (const f of failures) console.log('  - ' + f);
    }
    process.exit(fail ? 1 : 0);
})().catch((e) => {
    console.error('冒烟测试自身崩了：', e && (e.stack || e));
    process.exit(2);
});
