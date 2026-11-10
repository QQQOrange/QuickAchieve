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


    return { buildIndex: buildIndex };
})();
