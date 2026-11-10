/* ================================================================== */
/* 引擎面板懒建 / 分片填充（v2.23.99 从 bar.js 拆出）                    */
/*                                                                    */
/* 测试与页面都按"先加载本文件、再加载 bar.js"排序。bar.js 末尾把本文件  */
/* 需要的东西挂到 window.QSBar（getter 返回当时的 let/const 值），这里   */
/* 通过 qsb() 取用，避免跨脚本顶层 let 不可见。                         */
/* ================================================================== */

const qsb = () => window.QSBar;

let panelIO = null;
/* r70：懒填之后的补量。
 *
 * 为什么必须补：格子是**滚到跟前才建**的，而 pushLayout 发生在重渲染后的下一帧 ——
 * 那一刻只有第一节填了格子，其余还是空壳（高度 0）。空壳后来被观察器补上，
 * 面板随之长高，**窗口却不会自己跟着长** → 底栏被窗口下缘裁掉。
 * 这正是用户报的「点软件引擎之后设置按钮就不见了」的真正根因：
 * 点胶囊 → 重渲染 → 窗口按瞬态短高度收缩 → 懒填补齐 → 面板长出窗口。
 * （r69 当时归因为 max-height 钳制，只加了一道钳、没管量窗时机，所以没修住。）
 *
 * 做法：每次懒填完排一次 pushLayout。它有载荷去重（尺寸没变一条 IPC 都不发），
 * 滚动时持续补填也不会刷屏；rAF 合并同一帧里的多次补填。 */
let panelPushRaf = 0;
function schedulePanelPush() {
    if (panelPushRaf) return;
    panelPushRaf = requestAnimationFrame(() => {
        panelPushRaf = 0;
        if (qsb().panelEl().hidden || qsb().panelFav()) return;
        qsb().pushLayout();
    });
}

function panelFillOne(it) {
    if (!it || it.done) return;
    it.done = true;
    for (const row of it.list) it.grid.appendChild(qsb().panelRowCell(row));
    it.grid.dataset.filled = '1';
    // r70：见 schedulePanelPush 的注释 —— 补填改变了面板高度，窗口要跟上
    schedulePanelPush();
}

/* ------------------------------------------------------------------ */
/* r81：分片填充（搜索/过滤时不再一次性建 777 个格子）                  */
/*                                                                     */
/* 用户实测：在「更多引擎」里敲下**第一个字符**，界面会卡住一两秒。     */
/* 原因就是原来过滤入口那句 `if (q) panelFillAllNow()` —— 过滤是按      */
/* `.cell` 找的，所以有词时必须先把所有空壳填满；而一次建 777 个格子、 */
/* 每个格子还要跑一次「统一图标大小」的 canvas 像素测量，全在主线程上   */
/* 同步做完。                                                          */
/*                                                                     */
/* 现在改成**按时间片分批**：每片最多 8ms（约半帧），片与片之间让出     */
/* 主线程。用户看到的是"结果一片片出现"，而不是"卡死两秒然后突然全出来"。*/
/* 填完之后再按当前词收一次过滤，保证最终结果和一次性填充**完全一致**。 */
/* ------------------------------------------------------------------ */
let panelFillJob = 0;

/** 把还没填的格子分批填完；返回"是否真的还有活要干"。
 *
 *  没有待填格子 → 返回 false（调用方可以立刻做后面的过滤）。
 *  有 → 起一个后台分片任务，返回 true（这一趟先只用已有格子过滤）。 */
function panelFillAllChunked() {
    const todo = qsb().panelFillers().filter((it) => !it.done);
    if (!todo.length) return false;
    const id = ++panelFillJob;   // 新的任务作废旧任务
    let i = 0;
    const step = () => {
        if (id !== panelFillJob) return;          // 被更新的输入取代了，收手
        const t0 = performance.now();
        // 8ms ≈ 半帧：再长就会掉帧
        while (i < todo.length && performance.now() - t0 < 8) {
            panelFillOne(todo[i++]);
        }
        if (i < todo.length) {
            setTimeout(step, 0);
            return;
        }
        panelFillJob = 0;
        // 填完了：按当前过滤词再收一遍（新出现的格子必须参与过滤，
        // 否则用户会看到"搜索了，但列表里还留着没命中的旧格子"）
        if (qsb().engineFilterText()) qsb().applyEngineFilter(qsb().engineFilterText());
    };
    setTimeout(step, 0);
    return true;
}
function panelWatch() {
    if (panelIO) { panelIO.disconnect(); panelIO = null; }
    // 搜索/过滤词在的时候偷不了懒；没有 IntersectionObserver 也一样
    if (qsb().engineFilterText() || qsb().panelSearchMode() || typeof IntersectionObserver !== 'function') {
        /* ⚠ 这里同样走**分批**（r81）：面板在"有过滤词"状态下重画时，
         *   一次性填 777 格依然是主线程上的一两秒卡顿。
         *   分批本身会在填完后按当前词再过滤一遍，结果与一次性填充一致。 */
        panelFillAllChunked();
        return;
    }
    panelIO = new IntersectionObserver((ents) => {
        for (const en of ents) {
            if (!en.isIntersecting) continue;
            const it = qsb().panelFillers().find((x) => x.grid === en.target);
            if (it) panelFillOne(it);
            panelIO.unobserve(en.target);
        }
    }, { root: qsb().panelBody(), rootMargin: '400px 0px' });
    for (const it of qsb().panelFillers()) panelIO.observe(it.grid);
}

/* v2.23.103：从 bar.js 搬来的「最近使用」一栏（依赖走 QSBar 桥）。 */
function renderRecentSection() {
    const cfg = window.CONF || {};
    const names = Array.isArray(cfg.recentEngines) ? cfg.recentEngines : [];
    if (!names.length) return;
    const max = Math.max(1, Math.min(20, Number(cfg.recentMax) || 10));
    /* r57：**横条上已经固定住的不放进「最近使用」。**
     *
     * 用户原话：「最近使用的引擎一定要跟横条里已经固定的不重复，毕竟都在横条里
     * 固定住了，那你再放到最近使用就没有必要了，因为放到横条里的肯定是要经常
     * 使用的。」
     *
     * 这条不只是"去重"——它决定了这一栏**还剩什么价值**：横条上的引擎本来就在
     * 眼皮底下、一格就能点，再把它们抄到"最近使用"里，等于这一栏永远只显示
     * 你早就有的东西，真正需要"捞回来"的那几家反而被挤到 max 之外。
     *
     * ⚠ 用 qsb().barList()（横条此刻真正的成员，悬空哨兵已剪掉），别读 CONF.barList
     *   原始数组 —— 里面可能有已删软件的哨兵，会误判"在横条上"。
     * ⚠ 过滤在**取前 max 个之前**做：先排除再截断，否则名额被横条上的引擎
     *   占掉，这一栏实际显示的数会比用户设的少。 */
    const inBar = qsb().barList();
    const barSet = new Set(inBar);
    const rows = window.qsAllEngineRows(qsb().panelTree());
    const byName = {};
    for (const r of rows) byName[r.name] = r;
    const picks = [];
    for (const n of names) {
        if (picks.length >= max) break;
        if (barSet.has(n)) continue;   // 横条上已经有了，不重复
        const r = byName[n];
        // 名单里可能有已经不存在的引擎（配置恢复 / 引擎被删）：跳过，
        // 别给一颗点了 404 的格子。
        if (r) picks.push(r);
    }
    /* 排除完横条上的之后可能一颗都不剩（用户最近用的全是横条上那几家）。
     * 这时**整栏不显示** —— 显示一个只有标题的空栏，比不显示更像坏了。 */
    if (!picks.length) return;

    // 标题：和分类标题同一形状（.cat-title），所以滚动同步/锚点那套机器
    // 认它也不碍事 —— 但它**没有** data-cat，锚点导航不会给它发胶囊。
    const t = document.createElement('div');
    t.className = 'cat-title recent-title';
    t.dataset.recent = '1';
    const txt = document.createElement('span');
    txt.textContent = qsb().barT('最近使用');
    t.appendChild(txt);
    const hint = document.createElement('small');
    hint.textContent = qsb().barT('你刚用过的引擎，按最近顺序排');
    t.appendChild(hint);
    qsb().panelBody().appendChild(t);

    const grid = document.createElement('div');
    grid.className = 'grid recent-grid';
    /* 直接填，不走懒建：这一栏最多 20 颗，懒建的观察器开销比省下的 DOM 还大。
     *
     * ⚠ 必须打 `data-filled="1"`。面板的懒建有一套不变量：
     *   「所有 .grid 都填过 ⇔ filled 数 === grids 数」（r51 探针 ④ 就是按这个判
     *   "懒建没漏内容"）。这一栏是**急切**填的，不打这个标记就会让那个等式
     *   差 1，看上去像"有一节漏填了" —— 实际是标记没跟上。
     *   打上之后它既是"已填"，也和懒建那套账对得上。 */
    grid.dataset.filled = '1';
    for (const r of picks) grid.appendChild(qsb().panelRowCell(r));
    qsb().panelBody().appendChild(grid);
}


