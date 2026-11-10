/* ================================================================== */
/* 剪贴板历史（v2.23.108 从 settings.js 拆出）                          */
/*                                                                    */
/* 隐私三原则：只存本机、不参与同步、随时可清空。                      */
/* 数据在 cliphist-data.json，设置独立存 cliphist.json；不进 config，  */
/* 不会被备份 / WebDAV 带出去。                                       */
/* 依赖由 settings.js 的 bindClipHist() 传入：$ / invoke / toast。     */
/* ================================================================== */

window.QSClipHist = (function () {
    let $ = null;
    let invoke = null;
    let toast = null;

let clipHistCfg = null;

function clipHistTimeText(ms) {
    if (!ms) return '';
    const d = new Date(ms);
    const p = (n) => String(n).padStart(2, '0');
    return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

async function paintClipHist() {
    const box = $('clipHistList');
    if (!box) return;
    let items = [];
    try { items = await invoke('clip_hist_list'); } catch (_) { items = []; }
    box.textContent = '';
    if (!Array.isArray(items) || !items.length) {
        const d = document.createElement('div');
        d.className = 'ci-empty';
        d.textContent = '还没有记录（打开上面的开关后，复制文字就会出现在这里）';
        box.appendChild(d);
        return;
    }
    items.forEach((it) => {
        const row = document.createElement('div');
        row.className = 'ci-item';
        row.title = '点一下复制：' + String(it.text || '').slice(0, 200);
        const t = document.createElement('span');
        t.className = 'ci-t';
        const one = String(it.text || '').replace(/\s+/g, ' ').trim();
        t.textContent = one.length > 80 ? one.slice(0, 79) + '…' : one;
        const m = document.createElement('span');
        m.className = 'ci-m';
        m.textContent = [it.app || '', clipHistTimeText(it.at)].filter(Boolean).join(' · ');
        row.appendChild(t);
        row.appendChild(m);
        row.addEventListener('click', () => {
            invoke('copy_text', { text: it.text || '' })
                .then(() => toast('已复制回剪贴板'))
                .catch(() => toast('复制失败'));
        });
        box.appendChild(row);
    });
}

async function loadClipHist() {
    try { clipHistCfg = await invoke('clip_hist_cfg'); } catch (_) { clipHistCfg = null; }
    const c = clipHistCfg || {};
    const set = (id, v) => { const el = $(id); if (el) { if (el.type === 'checkbox') el.checked = !!v; else el.value = v; } };
    set('s-clipHistEnabled', c.enabled);
    set('s-clipHistMax', c.max || 30);
    set('s-clipHistPersist', c.persist);
    paintClipHist();
}

async function saveClipHist() {
    const c = {
        enabled: !!($('s-clipHistEnabled') && $('s-clipHistEnabled').checked),
        max: Number(($('s-clipHistMax') && $('s-clipHistMax').value) || 30),
        persist: !!($('s-clipHistPersist') && $('s-clipHistPersist').checked),
    };
    try {
        await invoke('clip_hist_set', { v: c });
        toast(c.enabled ? '剪贴板历史已开启' : '剪贴板历史已关闭（已清空内存里的记录）');
        loadClipHist();
    } catch (e) {
        toast('保存失败：' + ((e && e.message) || e));
    }
}

function bind(deps) {
        ({ $, invoke, toast } = deps);
    if (!$('clipHistList')) return;
    ['s-clipHistEnabled', 's-clipHistMax', 's-clipHistPersist'].forEach((id) => {
        const el = $(id);
        if (el) el.addEventListener('change', saveClipHist);
    });
    const clear = $('btnClipHistClear');
    if (clear) {
        clear.addEventListener('click', async () => {
            try {
                await invoke('clip_hist_clear');
                toast('剪贴板历史已清空');
                paintClipHist();
            } catch (e) {
                toast('清空失败：' + ((e && e.message) || e));
            }
        });
    }
    loadClipHist();
}

    return { bind: bind, paint: paintClipHist, load: loadClipHist, save: saveClipHist };
})();
