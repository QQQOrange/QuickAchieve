/* 英文模式下还剩多少中文？—— 用**真的 i18n 运行时**跑真页面，数残留文本节点。
 *
 * 为什么不用"扫源码里的字符串"：i18n 的键是中文原文，动态拼接的句子走
 * RULES 正则；静态扫描既会漏（拼出来的）也会多（注释、日志、用户数据）。
 * 这里只认"页面上真的显示出来的、含汉字的**可见文本**"，
 * 再按 i18n 自己的跳过规则（data-i18n-skip / .nm / .letter 等）排除用户数据。
 *
 * 用法：node i18n-coverage.js [页面名...]
 */
const fs = require('fs');
const path = require('path');
const { JSDOM, VirtualConsole } = require('jsdom');

const ROOT = path.join(__dirname, '..');
const read = (p) => fs.readFileSync(path.join(ROOT, p), 'utf8');

const PAGES = process.argv.slice(2).length ? process.argv.slice(2)
    : ['index.html', 'settings.html', 'ocr.html', 'trans.html', 'qr.html', 'qrgen.html',
        'region.html', 'lock.html', 'update.html', 'toast.html', 'picker.html', 'iconedit.html'];

const CJK = /[\u3400-\u9FFF\uF900-\uFAFF]/;

function makeEnv(file) {
    const errors = [];
    const vc = new VirtualConsole();
    vc.on('jsdomError', (e) => errors.push(String((e && e.message) || e)));
    const conf = {
        uiLang: 'en', theme: 'dark', barList: ['百度'], gridItems: [],
        gridPages: [{ id: 'p1', name: '默认页', items: [] }], gridDefaultPage: 'p1',
        /* 这些是 config_view 一定会带上的容器型字段（Rust 侧 `json!` 里是
         * 空对象/空数组）。少给一个，设置页渲染横条列表时就 TypeError —— * 那是页面自己的健壮性问题，但为了让这份 i18n 统计能跑完，先把
         * 形状补全（真实后端永远是全的）。 */
        overrideIcons: {}, urlOverrides: {}, dirIcons: {}, customEngines: [],
        apps: [], favorites: [], separators: [], recentEngines: [],
        aiSuffixes: {}, aiSuffix: '', ocrMulti: [], ocrKeys: {}, ocrSecrets: {},
        transEngines: [], transCreds: {}, imgEngines: [], imgKeys: {},
    };
    const install = (w) => {
        w.matchMedia = () => ({
            matches: false, media: '', addListener() { }, removeListener() { },
            addEventListener() { }, removeEventListener() { },
        });
        w.__TAURI__ = {
            core: {
                invoke: (cmd) => {
                    if (cmd === 'get_config') return Promise.resolve(conf);
                    if (cmd === 'backup_info') {
                        return Promise.resolve({ dir: '', isCustom: false, configPath: '', files: [] });
                    }
                    if (cmd === 'toast_pull') return Promise.resolve(null);
                    if (cmd === 'qr_take_image' || cmd === 'qrgen_take_text') return Promise.resolve('');
                    /* 一堆 `*_list` 命令真实返回的是数组，给 null 会让页面在
                     * `.length` 上直接炸 —— 按形状给个空数组，让渲染能走完。 */
                    if (/_list$|_engines$|_groups$|_langs$|_rows$/.test(cmd)) return Promise.resolve([]);
                    if (cmd === 'ocr_env_check') return Promise.resolve({ ok: true, deps: [] });
                    if (cmd === 'grid_config') return Promise.resolve({});
                    if (cmd === 'wrap_pairs') return Promise.resolve([]);
                    if (cmd === 'backup_security_state') return Promise.resolve({ hasPassword: false, envelope: 'dpapi', portable: false, hasToken: false, secretCount: 0 });
                    if (cmd === 'github_rate_state') return Promise.resolve({ ok: true, authed: false, limit: 60, remaining: 60, used: 0, reset: 0 });
                    if (cmd === 'update_pending') return Promise.resolve({ has: false });
                    if (cmd === 'app_is_locked') return Promise.resolve(false);
                    if (cmd === 'startup_password_configured') return Promise.resolve(false);
                    return Promise.resolve(null);
                },
            },
            event: {
                listen: () => Promise.resolve(() => { }),
                once: () => Promise.resolve(() => { }),
                emit: () => Promise.resolve(),
            },
            window: {
                getCurrent: () => Promise.resolve({ label: file, listen: () => Promise.resolve(() => { }) }),
            },
        };
        // jsdom 不提供 fetch（Node 的全局 fetch 不在 window 上）。设置页 boot 时
        // 会 `fetch('data/engines.json')`，不打桩整页初始化就断在第一步。
        w.fetch = (url) => {
            const rel = String(url).replace(/^\.?\//, '');
            const p = path.join(ROOT, 'ui', rel);
            if (fs.existsSync(p)) {
                const body = fs.readFileSync(p, 'utf8');
                return Promise.resolve({
                    ok: true, status: 200,
                    text: () => Promise.resolve(body),
                    json: () => Promise.resolve(JSON.parse(body)),
                });
            }
            return Promise.resolve({ ok: false, status: 404, json: () => Promise.reject(new Error('404')) });
        };
    };
    const abs = path.join(ROOT, 'ui', file).split(path.sep).join('/');
    const dom = new JSDOM(read('ui/' + file), {
        runScripts: 'dangerously', resources: 'usable', pretendToBeVisual: true,
        url: 'file:///' + encodeURI(abs), virtualConsole: vc, beforeParse: install,
    });
    return { dom, w: dom.window, errors };
}

/* 与 ui/i18n.js 的 SKIP_CLASS 口径一致：这些 class 里是**用户数据**，不该翻译 */
const SKIP_CLASS = ['nm', 'letter', 'eng-name', 'fav-name'];

function shouldSkip(node) {
    for (let el = node.parentElement; el; el = el.parentElement) {
        if (el.hasAttribute('data-i18n-skip')) return true;
        const cls = el.className;
        if (typeof cls === 'string' && SKIP_CLASS.some((c) => cls.split(/\s+/).includes(c))) return true;
        if (/^(SCRIPT|STYLE|NOSCRIPT|TEXTAREA|CODE|PRE)$/.test(el.tagName)) return true;
        if (el.hidden) return true;
    }
    return false;
}

async function page(file) {
    const env = makeEnv(file);
    await new Promise((r) => {
        let done = false;
        const fin = () => { if (!done) { done = true; r(); } };
        env.w.addEventListener('load', fin);
        setTimeout(fin, 1200);
    });
    await new Promise((r) => setTimeout(r, 120));
    const { w } = env;

    if (!w.QS_I18N) return { file, missing: null, note: '没挂 i18n' };
    w.QS_I18N.setLang('en');
    await new Promise((r) => setTimeout(r, 120));

    const misses = new Map();
    const walk = (n) => {
        if (n.nodeType === 3) {
            const t = String(n.nodeValue || '').replace(/\s+/g, ' ').trim();
            if (t && CJK.test(t) && !shouldSkip(n)) {
                misses.set(t, (misses.get(t) || 0) + 1);
            }
            return;
        }
        for (const c of n.childNodes) walk(c);
    };
    walk(w.document.body);

    // placeholder / title / aria-label 也要看
    for (const el of w.document.querySelectorAll('[placeholder],[title],[aria-label]')) {
        if (shouldSkip(el)) continue;
        for (const attr of ['placeholder', 'title', 'aria-label']) {
            const v = el.getAttribute(attr);
            if (v && CJK.test(v)) misses.set(attr + '=' + v.trim(), (misses.get(attr + '=' + v.trim()) || 0) + 1);
        }
    }
    return { file, missing: [...misses.entries()], errors: env.errors };
}

(async function main() {
    let total = 0;
    const out = {};
    for (const p of PAGES) {
        if (!fs.existsSync(path.join(ROOT, 'ui', p))) continue;
        const r = await page(p);
        out[p] = r;
        if (r.note) { console.log(`\n--- ${p}：${r.note}`); continue; }
        total += r.missing.length;
        console.log(`\n--- ${p}：残留 ${r.missing.length} 条`);
        for (const [k, n] of r.missing.slice(0, 40)) console.log(`   ${n > 1 ? 'x' + n + ' ' : '   '}${k}`);
        if (r.errors.length) console.log(`   (加载期异常 ${r.errors.length}: ${r.errors[0]})`);
    }
    console.log(`\n合计残留：${total}`);
    fs.writeFileSync(path.join(__dirname, 'i18n-coverage.json'), JSON.stringify(out, null, 2));
    /* jsdom 的 pretendToBeVisual 会挂一个常驻 rAF 定时器，不显式退出的话
     * 进程拿到结果后还要空转很久才自己结束（CI 里看着像卡死）。 */
    process.exit(0);
})();
