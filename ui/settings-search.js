/* ================================================================== */
/* 设置页搜索索引（v2.23.101 从 settings.js 拆出）                       */
/*                                                                    */
/* 只负责"把 .row / 动作扫成条目"：分页名、动作表、索引构建。            */
/* 状态（findIndex、findCursor）和绑定/跳转交互仍留在 settings.js。      */
/* buildIndex 的 pyInitialsOf 由调用方传入（settings.js 持有）。         */
/* ================================================================== */

window.QSSearch = (function () {
function findPaneName(pane) {
    /* 子页（备份 / 帮助 / 数据…）没有顶栏 tab，只有分页里的 chip。
     * 原来只查 `#tabs button[data-tab]`，于是这些页在结果列表里显示成英文 key
     * （"backup" / "help"）—— 用户看到的是"这一半中文一半英文"的怪东西。 */
    const nav = document.querySelector('#tabs button[data-tab="' + pane + '"]')
        || document.querySelector('.subnav .chip[data-sub="' + pane + '"]');
    if (!nav) return pane;
    /* ⚠ 先把图标那个 span 的文字**精确**摘掉，再用剩下的当分页名。
     * 我第一版是用一条"去掉行首非空白"的正则去剪，而那条正则里的量词是
     * 贪婪的 —— 会把「⚙️通用」整串吃掉，分页名于是退回英文 key
     * （测试里表现为"结果里没有分页名"）。 */
    let label = nav.textContent || '';
    const icon = nav.querySelector('.tb-i');
    if (icon) label = label.replace(icon.textContent || '', '');
    // 顶栏 tab 的图标在 `.tb-i` 里，而分页里的 chip 是把 emoji 直接写在文字里的
    // （`☁️ 备份与同步`）—— 把开头那串非文字字符一并剪掉，两种写法都干净。
    label = label.replace(/^[^\p{L}\p{N}]+/u, '').trim();
    return label || pane;
}

/** 搜索里可以直接执行的**动作**（v2.23.44）。
 *
 * 这就是"命令面板"的轻量版：不用另开窗口，搜索框里打「导出」就能直接跑。
 * 每条给：标题、所属分页（借它的名字做分组）、要点的那颗按钮（优先）或 invoke 命令。
 * 为什么不直接 invoke：这些动作里有一半会弹确认框 / 选路径（导入、恢复默认），
 * 走原有按钮的点击处理才是**同一条路径**，不会出现两套行为。 */
const FIND_ACTIONS = [
    { label: '导出配置备份（明文）', pane: 'data', click: 'btnExport' },
    { label: '载入配置备份（导入）', pane: 'data', click: 'btnImport' },
    { label: '恢复全部默认设置', pane: 'data', click: 'btnReset' },
    { label: '复制诊断信息', pane: 'about', click: 'btnDiag' },
    { label: '打开配置目录', pane: 'data', invoke: 'open_data_dir' },
];

/** 把动作也塞进搜索索引（搜到设置项的同时，动作也能被搜到）。 */
function findActionEntries(pyInitialsOf) {
    return FIND_ACTIONS.map((a) => ({
        pane: a.pane,
        paneName: '动作',
        label: a.label,
        body: a.label + ' 动作',
        py: pyInitialsOf(a.label),
        action: a,
        el: null,
    }));
}
function buildIndex(pyInitialsOf) {
    const out = [];
    const norm = (s) => String(s || '').replace(/\s+/g, ' ').trim();
    document.querySelectorAll('.pane[data-pane]').forEach((paneEl) => {
        const pane = paneEl.dataset.pane;
        const paneName = findPaneName(pane);
        /* 一行的"标题"优先取 .rt > b；没有 .rt 的（开关行、小标题行）取自身文字。 */
        paneEl.querySelectorAll('.row, .tools-title, .help-item > summary').forEach((el) => {
            const b = el.querySelector('.rt > b, .rt b');
            const label = norm(b ? b.textContent : el.textContent).split('\n')[0];
            if (!label || label.length > 60) return;
            const body = norm(el.textContent);
            /* 拼音首字母串：只在建索引时算一次（每个汉字都要和 23 个边界字比，
             * 每次输入都重算会把搜索框拖卡）。 */
            const py = pyInitialsOf(label);
            out.push({ pane, paneName, label, body, py, el });
        });
    });
    // 动作排在设置项之后：搜「导出」时先看到设置项、再看到「导出配置备份」
    out.push(...findActionEntries(pyInitialsOf));
    return out;
}



/* ---- 运行时：绑定/跳转/高亮（v2.23.107） ---- */
let findIndex = null;
let findCursor = -1;

function close() {
    const list = document.getElementById('setSearchList');
    if (list) {
        list.hidden = true;
        list.textContent = '';
    }
    findCursor = -1;
}

/** 把一个元素"亮出来"：摘掉祖先上的 hidden，滚到视野中间，闪一下。 */
function reveal(el) {
    for (let n = el; n && n !== document.body; n = n.parentElement) {
        if (n.hidden) n.hidden = false;
        if (n.tagName === 'DETAILS') n.open = true;
    }
    try {
        el.scrollIntoView({ block: 'center', behavior: 'smooth' });
    } catch (_) { }
    el.classList.remove('find-hit');
    void el.offsetWidth;
    el.classList.add('find-hit');
    setTimeout(() => el.classList.remove('find-hit'), 1400);
}

function bind(deps) {
    const { $, gotoPane, invoke, pyInitialsOf } = deps;
    const input = $('setSearch');
    const list = $('setSearchList');
    if (!input || !list) return;

    /* 查询串是不是 initials 的**子序列**（"zd" ⊂ "zidongshouqi"）。
     * 只对纯字母查询生效：中文查询走"包含"那条路。 */
    const isSubseq = (q, s) => {
        let i = 0;
        for (let j = 0; j < s.length && i < q.length; j++) if (s[j] === q[i]) i++;
        return i === q.length;
    };
    const matches = (it, q) => {
        if ((it.label + ' ' + it.body).replace(/\s+/g, '').toLowerCase().includes(q)) return true;
        return /^[a-z0-9]+$/.test(q) && q.length >= 2 && it.py && isSubseq(q, it.py);
    };

    /* 最近点过的 5 项：空着搜索框时直接列出来。
     * 存 localStorage（不是配置）—— 它只是"这个窗口里的使用痕迹"，
     * 不值得进备份、也不该跨机器同步。 */
    const RECENT_KEY = 'qsFindRecent';
    const readRecent = () => {
        try {
            const a = JSON.parse(localStorage.getItem(RECENT_KEY) || '[]');
            return Array.isArray(a) ? a.slice(0, 5) : [];
        } catch (_) { return []; }
    };
    const pushRecent = (it) => {
        try {
            const a = readRecent().filter((x) => !(x.pane === it.pane && x.label === it.label));
            a.unshift({ pane: it.pane, paneName: it.paneName, label: it.label });
            localStorage.setItem(RECENT_KEY, JSON.stringify(a.slice(0, 5)));
        } catch (_) { }
    };

    /** 画一组"可以直接点"的条目（最近用过的）。 */
    const paintRecent = () => {
        const recent = readRecent();
        list.textContent = '';
        if (!recent.length) {
            list.hidden = true;
            return;
        }
        const head = document.createElement('div');
        head.className = 'find-empty';
        head.textContent = '最近改过的';
        list.appendChild(head);
        recent.forEach((it) => {
            const row = document.createElement('div');
            row.className = 'find-item';
            const t = document.createElement('span');
            t.className = 'fi-t';
            t.textContent = it.label;
            const p = document.createElement('span');
            p.className = 'fi-p';
            p.textContent = it.paneName || '';
            row.appendChild(t);
            row.appendChild(p);
            row.addEventListener('mousedown', (e) => {
                e.preventDefault();
                findPick(it);
            });
            list.appendChild(row);
        });
        list.hidden = false;
        findCursor = -1;
    };

    const paint = () => {
        const q = String(input.value || '').replace(/\s+/g, '').toLowerCase();
        list.textContent = '';
        findCursor = -1;
        if (!q) {
            paintRecent();
            return;
        }
        const idx = findIndex || (findIndex || (findIndex = buildIndex(pyInitialsOf)));
        /* 排序：标题里直接包含查询串的最相关，其次正文包含，最后才是拼音命中。
         * 不排的话"zdsq"这种拼音查询会把一堆正文里恰好包含 z…d… 的行排在前面。 */
        const score = (it) => {
            if (it.label.replace(/\s+/g, '').toLowerCase().includes(q)) return 0;
            if (it.body.replace(/\s+/g, '').toLowerCase().includes(q)) return 1;
            return 2;
        };
        const hits = idx
            .map((it, i) => ({ it, i }))
            .filter(({ it }) => matches(it, q))
            .sort((a, b) => score(a.it) - score(b.it) || a.i - b.i)
            .slice(0, 20)
            .map(({ it }) => it);
        if (!hits.length) {
            const d = document.createElement('div');
            d.className = 'find-empty';
            d.textContent = '没找到相关设置';
            list.appendChild(d);
            list.hidden = false;
            return;
        }
        hits.forEach((it, i) => {
            const row = document.createElement('div');
            row.className = 'find-item' + (i === 0 ? ' on' : '');
            const t = document.createElement('span');
            t.className = 'fi-t';
            t.textContent = it.label;
            const p = document.createElement('span');
            p.className = 'fi-p';
            p.textContent = it.paneName;
            row.appendChild(t);
            row.appendChild(p);
            row.dataset.i = String(i);
            row.addEventListener('mousedown', (e) => {
                e.preventDefault();   // 别让输入框先失焦导致列表先关
                findPick(hits[i]);
            });
            list.appendChild(row);
        });
        findCursor = 0;
        list.hidden = false;
    };

    const findPick = (it) => {
        close();
        input.blur();
        input.value = '';
        pushRecent(it);
        // 动作：走原有按钮的点击路径（同一条代码路径，不会出现两套行为）
        if (it.action) {
            if (it.action.click) {
                const b = $(it.action.click);
                if (b) { b.click(); return; }
            }
            if (it.action.invoke) { invoke(it.action.invoke).catch(() => { }); return; }
        }
        if (it.pane) gotoPane(it.pane);
        /* 元素可能已经被重画过：按标题文字在那一页里重新找一遍。 */
        let el = it.el;
        if (!el || !el.isConnected) {
            const paneEl = document.querySelector('.pane[data-pane="' + it.pane + '"]');
            el = paneEl ? [...paneEl.querySelectorAll('.row, .tools-title')]
                .find((x) => {
                    const b = x.querySelector('.rt > b, .rt b');
                    return String((b || x).textContent).replace(/\s+/g, ' ').trim() === it.label;
                }) : null;
        }
        if (el) reveal(el);
    };

    input.addEventListener('input', paint);
    input.addEventListener('focus', () => paint());
    input.addEventListener('blur', () => setTimeout(findClose, 120));
    input.addEventListener('keydown', (e) => {
        const items = [...list.querySelectorAll('.find-item')];
        if (e.key === 'Escape') {
            input.value = '';
            close();
            input.blur();
            return;
        }
        if (!items.length) return;
        if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
            e.preventDefault();
            const step = e.key === 'ArrowDown' ? 1 : -1;
            findCursor = (findCursor + step + items.length) % items.length;
            items.forEach((x, i) => x.classList.toggle('on', i === findCursor));
            items[findCursor].scrollIntoView({ block: 'nearest' });
        } else if (e.key === 'Enter') {
            e.preventDefault();
            const idx = findIndex || (findIndex || (findIndex = buildIndex(pyInitialsOf)));
            const q = String(input.value || '').replace(/\s+/g, '').toLowerCase();
            const hits = idx.filter((it) => matches(it, q));
            const pick = hits[Math.max(0, findCursor)];
            if (pick) findPick(pick);
        }
    });

    /* 全局快捷键：Ctrl+F 直接聚焦搜索框（设置页里也符合直觉）。 */
    document.addEventListener('keydown', (e) => {
        if ((e.ctrlKey || e.metaKey) && (e.key === 'f' || e.key === 'F')) {
            e.preventDefault();
            input.focus();
            input.select();
        }
    });
}


    return { buildIndex: buildIndex, bind: bind, reveal: reveal, close: close };
})();
