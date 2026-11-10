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
    /* v2.23.114：搜索/过滤时**按数据过滤**，只建命中的格子。
     * 旧行为是把 260 个格子全建出来、再按 DOM 隐藏 —— 等于为了搜索
     * 先付一次全量渲染。现在每个 filler 带 `renderList`：
     *   · 过滤词变了 → 清掉旧格子，只留匹配行；
     *   · 过滤词清空 → 恢复完整 list（下一帧分片补回）；
     *   · 没命中的 grid 直接标 filled + hidden，不建任何格子。
     * 非搜索态不调用本函数（懒建仍按滚动观察器走）。 */
    const q = String(qsb().engineFilterText() || '').trim().toLowerCase();
    for (const it of qsb().panelFillers()) {
        if (q) {
            if (it.filteredFor === q) continue;
            it.filteredFor = q;
            it.done = false;
            it.grid.textContent = '';
            it.grid.dataset.filled = '0';
            it.renderList = it.list.filter((row) => qsb().engineMatches(
                row.appName || row.name || row.engine || '', q));
        } else if (it.filteredFor) {
            it.filteredFor = '';
            it.done = false;
            it.grid.textContent = '';
            it.grid.dataset.filled = '0';
            it.renderList = null;
        }
    }
    const fillers = qsb().panelFillers();
    if (!q) {
        /* 清空过滤词：恢复完整列表。面板多半已 hidden（关闭路径），
         * 这里同步补齐 260 格，保证下次打开就是完整 DOM；隐藏状态下
         * 用户看不到这次重排，也符合旧的"收起再打开列表完整"行为。 */
        for (const it of fillers) {
            if (it.done) continue;
            if (it.renderList && it.renderList.length === 0) {
                it.done = true;
                it.grid.dataset.filled = '1';
                it.grid.hidden = true;
            } else {
                panelFillOne(it);
            }
        }
        return true;
    }
    const todo = fillers.filter((it) => !it.done);
    if (!todo.length) return false;
    const id = ++panelFillJob;   // 新的任务作废旧任务
    let i = 0;
    const step = () => {
        if (id !== panelFillJob) return;          // 被更新的输入取代了，收手
        const t0 = performance.now();
        // 8ms ≈ 半帧：再长就会掉帧
        while (i < todo.length && performance.now() - t0 < 8) {
            const it = todo[i++];
            if (it.renderList && it.renderList.length === 0) {
                // 这一节一个都没命中：标成已填，让 applyEngineFilter 藏掉它
                it.done = true;
                it.grid.dataset.filled = '1';
                it.grid.hidden = true;
            } else {
                panelFillOne(it);
            }
        }
        if (i < todo.length) {
            setTimeout(step, 0);
            return;
        }
        panelFillJob = 0;
        // 填完了：按当前过滤词再收一遍（新出现的格子必须参与过滤，
        // 否则用户会看到"搜索了，但列表里还留着没命中的旧格子"）
        const cur = String(qsb().engineFilterText() || '').trim().toLowerCase();
        if (cur) qsb().applyEngineFilter(cur);
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

/* v2.23.105：面板平铺区渲染（依赖走 QSBar 桥）。 */
function panelSubMeta(groups) {
    const m = {};
    for (const g of groups) {
        for (const s of g.subs) m[s.key] = { group: g.key, gtitle: g.title, title: s.title };
    }
    for (const k of qsb().PANEL_SUBS()) if (!m[k]) m[k] = qsb().PANEL_SUB_FALLBACK()[k];
    return m;
}


function renderFlatBody(groups, filterKey) {
    const rows = window.qsAllEngineRows(groups);
    const meta = panelSubMeta(groups);
    const bySub = {};
    for (const r of rows) {
        if (filterKey && r.group !== filterKey) continue;
        // r53：胶囊里选中的二级 / 三级也要真的过滤内容（"选哪个就只显示哪个"）
        if (qsb().panelSubFilter() && r.sub !== qsb().panelSubFilter()) continue;
        if (qsb().panelCatFilter() && (r.cat || '未分类') !== qsb().panelCatFilter()) continue;
        (bySub[r.sub] = bySub[r.sub] || []).push(r);
    }
    qsb().setPanelFillers([]);
    for (const sub of qsb().PANEL_SUBS()) {
        const list = bySub[sub];
        if (!list || !list.length) continue;
        const m = meta[sub];
        const t = document.createElement('div');
        t.className = 'cat-title';
        t.id = 'cat-' + sub;
        t.dataset.cat = sub;
        t.textContent = (m.gtitle || qsb().PANEL_GROUP_FALLBACK()[m.group]) + ' · ' + m.title;
        qsb().panelBody().appendChild(t);
        /* r52：中类里面再按**小类**分小节（AI 工具 / 知识 / 开发 / 购物…）。
         *  用户要求「网页搜索引擎里面再分，比如说 ai 工具、知识开发、搜索等等」，
         *  并且横条这侧也要有 —— 所以小类的标题 + 单独的格子容器都在这儿建，
         *  懒建的粒度也从"整个中类"细化成"一个小类"。 */
        const byCat = new Map();
        for (const r of list) {
            const k = r.cat || '未分类';
            if (!byCat.has(k)) byCat.set(k, []);
            byCat.get(k).push(r);
        }
        for (const [cat, rowsOfCat] of byCat) {
            const sh = document.createElement('div');
            sh.className = 'cat-sub';
            sh.dataset.sub = sub + '::' + cat;
            sh.textContent = cat;
            const sn = document.createElement('span');
            sn.className = 'n';
            sn.textContent = rowsOfCat.length + ' 个';
            sh.appendChild(sn);
            qsb().panelBody().appendChild(sh);
            const grid = document.createElement('div');
            grid.className = 'grid';
            qsb().panelBody().appendChild(grid);
            /* 格子**先不建**：挂进待填清单，滚到跟前（panelWatch）再说 */
            qsb().panelFillers().push({ grid: grid, list: rowsOfCat, done: false });
        }
    }
    /* ⚠ 第一个分类**必须同步建出来**：观察器是异步的（下一帧才回调），
     * 全留给它的话，面板打开瞬间上面那一节是**空的** —— 实测要 350ms 才
     * 冒出第一批格子，比改之前还难看。
     *
     * 2026-10-25：**从"填一节"改成"填够一屏"**。
     *
     * 只填一节的问题：一节通常 5~35 个格子，撑不满 .panel-body 那 344px 的
     * 上限，于是面板先按"矮"的样子被量一次 → 窗口按矮尺寸摆好 → 观察器
     * 补填下一节 → 面板长高 → 窗口再改一次尺寸。用户看到的就是
     * 「点开更多引擎，画面跳两三次才稳定」。
     *
     * 现在同步填到 scrollHeight 够 344px 为止（每个分类几毫秒，最多多花
     * 十几毫秒）。高度第一帧就是终值，观察器后面补的全落在滚动区里，
     * 不再改变整体高度、也就不会再动窗口。
     *
     * 344 = bar.css 里 .panel-body 的 max-height，到量就没有继续填的必要
     * （再填只影响滚动内容，不影响面板高度）。引擎少、填完全部还不到 344
     * 也无所谓 —— 那种情况下懒填本来就没东西可填。 */
    for (let i = 0; i < qsb().panelFillers().length; i++) {
        panelFillOne(qsb().panelFillers()[i]);
        if (qsb().panelBody().scrollHeight >= 344) break;
    }
    panelWatch();
}

/* v2.23.106：面板整体渲染（依赖走 QSBar 桥）。 */
function renderPanel(focusFilter, keepScroll) {
    /* 面板要重画时，先把当前滚动位置记下来。
     *
     * ⚠ `keepScroll` 是给「配置广播引起的重画」用的（见 qs://config 那段）。
     *   那种重画**不是用户要求换内容**，纯粹是别的地方改了设置 ——
     *   如果顺手把用户正在浏览的位置滚回顶部，用户看到的就是
     *   「我正翻着引擎列表，它自己跳回最上面了」。
     *   用户主动打开面板（togglePanel）、下钻、改过滤词时仍然归零，那是应该的。 */
    const prevScroll = keepScroll ? qsb().panelBody().scrollTop : 0;
    /* r69/r70 的历史结论（盒模型部分至今有效，高度策略已被 r74 取代 —— 见下一段）：
     *
     * 用户原话：「在更多引擎弹出来的页面下面有一个设置按钮，如果当我点击软件引擎
     * 之后，这个设置按钮就不见了。」
     *
     * 实测根因（r70 修正 r69 的结论）：面板内容一多就顶到 454px 高，而窗口没有
     * 跟着长（或横条落在屏幕下半部分、下面没地方了），底栏那颗「设置…」被挤到
     * **窗口外面** —— 按钮本身 display/visibility 都正常，纯粹是位置在视口外。
     *
     * ⚠ r69 只加了道 max-height 钳制，**没修盒模型** —— .panel 当时还是普通
     *   块布局，max-height 只钳面板框、三个子盒（head/body/foot）一个都不让位，
     *   底栏被挤到面板框外照样被窗口裁掉。
     *   r70 修好了盒模型（这个结论一直有效）：
     *     .panel 改 flex 列 + body min-height:0（和收藏面板同一套盒模型，
     *     见 bar.css）—— 被钳高时 body 自己收窄出滚动条，底栏永远在框内。
     *   r74 起高度策略换成下一段那套（不再按余量收窄），但盒模型不能退回去 ——
     *   两种策略都依赖它保证「底栏永远看得见」。 */
    /* r74：引擎面板**只按屏幕百分比封顶**，不再按「横条下面还剩多少」收窄。
     *
     * 老逻辑（r69/r70）是 min(屏幕 78%, barRoomBelow)：横条落在屏幕中下部时
     * 余量往往只有一两百 px，面板被钳成一条 —— sticky 头（收藏夹入口 + 大类
     * 胶囊 + 二级胶囊）就把它占满了。用户原话：「在屏幕中下位置触发横条，这时候
     * 点开更多引擎，因为横条位置原因导致所有的内容都被遮挡住了，只看得到收藏夹
     * 和引擎大类，但看不到任何一个引擎的内容」。
     *
     * 现在的口径（用户原话：「肯定是要以显示更多引擎的内容为主，这时候把横条
     * 往上面移一点都可以，但收起更多引擎之后，横条要回到原来的位置」）：
     *   · 面板按自然高度长（.panel-body 自己还有 344px 上限，总共约 450px）；
     *   · 屏幕放不下时，Rust 的 layout_bar 拿「横条 ∪ 面板」这个**包围盒**往
     *     工作区里钳（`iy2 + pih > area.bottom` 就把 iy2 整体上移）—— 横条
     *     临时上移、面板完整可见，这正是用户允许的「往上面移一点」；
     *   · 收起面板后包围盒缩回横条自己，iy2 回到锚点算出的原位 —— 复位是
     *     Rust 钳制的自然结果，不需要前端额外记状态、也不怕失步。
     *
     * ⚠ 收藏面板（fav-mode）**不走这套**：那边还是「横条不动、面板按剩余空间
     *    收窄、内部滚动」的老策略（用户当年点名「一点收藏夹，横条立刻跑到别的
     *    地方去了」）。两个面板两种策略，见 renderFavPanel 与 qs://bar-room
     *    handler 里各自的注释。 */
    if (!qsb().panelFav()) {
        const scrH = (window.screen && screen.availHeight) || 900;
        qsb().panelEl().style.maxHeight = Math.round(scrH * 0.78) + 'px';
    }

    /* r92：**内容没变就不要重建 DOM**（用户报「更多引擎页面闪烁」的修复）。
     *
     * # 为什么会闪
     *
     * 这里原来是 `qsb().panelBody().textContent = ''` 无条件清空，然后把几百个格子
     * 重新 createElement 一遍。一次重建要几十毫秒，这期间浏览器可能已经
     * 把"空面板"画到屏幕上了 —— 视觉上就是一帧白闪。
     *
     * 什么时候会重复渲染？`qs://config` 广播（收藏面板拖尺寸 600ms 一次、
     * 改任何设置、横条尺寸变化都会触发）都会走到这里。用户翻着引擎列表时
     * 面板每隔几百毫秒重建一次，闪得很有节奏 —— 就是他看到的那样。
     *
     * # 判据
     *
     * 用「渲染输入」的指纹：大类/中类/小类过滤 + 当前是否收藏模式 + 收藏
     * 数据 + 自定义引擎/外部软件名单 + 图标覆盖。这些都没变，重画出来的
     * DOM 一模一样，重建没有任何收益，只剩闪烁。
     *
     * ⚠ 和设置页 `renderAllList` 的指纹是同一套思路（那边早就这么做了，
     *   面板这边一直漏着）。指纹**不含滚动位置** —— 位置由 prevScroll 单独管。 */
    const sig = [
        qsb().panelGroupFilter() || '',
        qsb().panelSubFilter() || '',
        qsb().panelCatFilter() || '',
        qsb().panelFav() || '',
        (qsb().getConf().customEngines || []).map((x) => x.name).join('\u0001'),
        (qsb().getConf().apps || []).map((x) => x.name).join('\u0001'),
        window.qsIconSigOf ? window.qsIconSigOf(qsb().getConf().overrideIcons) : '',
        window.qsIconSigOf ? window.qsIconSigOf(qsb().getConf().dirIcons) : '',
        // 收藏树整体（展开/折叠、增删条目都会变）
        JSON.stringify(qsb().getConf().favorites || []),
        /* 2026-10-25：**「最近使用」也算渲染输入**。
         *
         * 这一栏画的是 qsb().getConf().recentEngines（用过哪些引擎）+ qsb().getConf().recentMax（露几个），
         * 而且会剔掉横条上已经固定的那几个（见 renderRecentSection，用的是
         * barList() 而不是 qsb().getConf().barList 原始数组）。这三样以前都没进指纹 ——
         * 别处不改，指纹就一直一样，于是"刚用了某个引擎"这一栏要等下一次
         * 真重画才更新。
         *
         * 补进来是为了配合下面那条新口径：缓存必须覆盖**所有**影响 DOM 的输入，
         * 否则"不重建"就会变成"显示过期内容"。 */
        (qsb().getConf().recentEngines || []).join('\u0001'),
        Number(qsb().getConf().recentMax) || 0,
        barList().join('\u0001'),
    ].join('\u0002');
    /* 指纹一样 + 面板里已经有内容 → **一个节点都不动**。
     *
     * ⚠ 这里以前是 `if (!focusFilter && ...)` —— 把 `focusFilter`（= 打开面板）
     *   当成了"必须重画"的信号。可打开面板恰恰是**最不该重画**的那一次：
     *   内容跟上次收起时一模一样，重建的收益是零，代价却是一帧白闪 ——
     *   `qsb().panelBody().textContent = ''` 之后要 createElement 几百个格子，这几十
     *   毫秒里浏览器已经把"空面板"合成到屏幕上了。
     *
     *   用户报的「选择一段文字后点开更多引擎，页面要闪好几次才稳定」，
     *   第一下就是它（第二下见 renderFlatBody 末尾"一次填够一屏"，
     *   第三下见 togglePanel 里收起时恢复 DOM 过滤状态那一段）。
     *
     * 现在只要指纹没变，就只做三件事：复位滚动、把光标送回搜索框、同步分类高亮。
     * 打开面板变成"啪"的一下 —— 没有中间态，没有白闪。
     *
     * `keepScroll` 仍然管住"配置广播引起的重画不该把用户正在看的位置滚回顶部"。 */
    /* v2.23.30：**缓存判据还要看"现在这块 DOM 是谁画的"**。
     *
     * 指纹里虽然有 `qsb().panelFav()`，但收藏面板是另一套渲染路径（renderFavPanel），
     * 它改了 qsb().panelBody() 却（在 v2.23.22 之前）不动指纹。于是用户
     * 「打开更多引擎 → 点进收藏 → 再点更多引擎」会看到收藏内容挂在那儿、
     * 引擎列表消失，而且怎么点都回不来 —— 用户报的"重启之后进更多引擎
     * 还是收藏"就是这条（他那个进程一直没退，所以看起来像跨重启）。
     *
     * v2.23.22 已经让 renderFavPanel 清空指纹；这里再加一道独立保险：
     * 记录 DOM 的归属，归属不对就**必然重建**，不再依赖指纹是否恰好不同。 */
    if (sig === qsb().panelRenderSig() && qsb().panelRenderOwner() === 'engines' && qsb().panelBody().childElementCount) {
        if (keepScroll) qsb().panelBody().scrollTop = prevScroll;
        else qsb().panelBody().scrollTop = 0;
        if (focusFilter && qsb().inputEngFilter() && !qsb().panelEl().classList.contains('fav-mode')) {
            try { qsb().inputEngFilter().focus(); } catch (_) { }
        }
        syncActiveCatNav();
        return;
    }
    qsb().setPanelRenderSig(sig);
    qsb().setPanelRenderOwner('engines');

    qsb().panelBody().textContent = '';

    /* 收藏夹入口 + 大类胶囊过滤 + 分类锚点导航共用**同一个 sticky 头**。
     *
     * ⚠ 以前是「两个 .cat-nav 各自 position:sticky; top:0」—— 它俩都在 panel-body
     *   这同一个滚动容器里、都钉在同一条顶边。滚动时后出现的引擎分类导航（DOM 靠后，
     *   同层同 z-index 下后绘制）会**盖住**先出现的收藏夹入口。现在把三者装进
     *   同一个 sticky 容器：容器占一整条，里面几行正常堆叠，钉住后同时可见。 */
    const head = document.createElement('div');
    head.className = 'panel-sticky';

    // —— 收藏夹入口（面板里也应该有收藏夹）——
    // 一级目录平铺在最上面，点一下就切到那个目录的收藏视图。
    const favs = favGroups();
    if (favs.length) {
        const favNav = document.createElement('nav');
        favNav.className = 'cat-nav fav-nav';
        favNav.setAttribute('aria-label', '收藏夹目录');
        for (const g of favs) {
            const b = document.createElement('button');
            b.type = 'button';
            b.className = 'cat-nav-item fav-entry';
            const badge = document.createElement('span');
            badge.className = 'fav-entry-badge';
            badge.textContent = (g.name || '★').trim().charAt(0) || '★';
            b.appendChild(badge);
            b.appendChild(Object.assign(document.createElement('span'), { textContent: g.name }));
            b.addEventListener('click', () => {
                qsb().setPanelFav(qsb().QS_FAV_PREFIX() + g.id);
                // #141：记下来路 —— 收起时靠它决定「回引擎面板」还是「整个收起」
                qsb().setFavFromEngines(true);
                renderFavPanel();
                requestAnimationFrame(() => setTimeout(pushLayout, 0));
            });
            favNav.appendChild(b);
        }
        head.appendChild(favNav);
    }

    const groups = panelTree();

    // —— 大类胶囊过滤行（#140）：全部引擎 / 网页引擎 / 软件引擎 / 快捷键引擎 ——
    appendGroupNav(head, groups);

    // —— 子类锚点导航：只给当前过滤下**有内容**的节发胶囊 ——
    const meta = panelSubMeta(groups);
    const sections = panelSectionSubs(groups, qsb().panelGroupFilter())
        .map((s) => ({ key: s, title: meta[s].title }));
    /* r53：二级 / 三级目录收进**一个胶囊**（用户：「所有的 2 级目录它都用一个胶囊
     * 显示，我点开胶囊之后它会展开，我选择哪一个然后就显示哪一个」）。
     * 一级（大类）仍然平铺一行，点了它才把二级露出来；选了二级才有三级。
     * 原来三层全铺开 —— 用户说"太杂、影响观感、也占内容的地方"。 */
    appendPickNav(head, groups, sections);
    applyPickFilter();   // r53：胶囊选中的那一类一定留下，其余一定收起（DOM 兜底）

    /* 头里没东西就别塞一个空盒子 —— 空容器也会占一行 padding + 下边框，
     * 让第一个分类标题凭白往下掉一截。 */
    if (head.childElementCount) qsb().panelBody().appendChild(head);

    // —— r56：「最近使用」一栏，钉在**所有引擎分类的最上面** ——
    // 用户原话：「通过横条点击更多引擎展现出来的页面，再添加一个最近使用的一栏，
    // 这一栏就默认显示，它显示在已经陈列出来的所有引擎的最上面，并且标记好是最近
    // 使用，在设置当中可以选择最多呈现最近使用的 5 个、10 个、15 个或者 20 个。」
    //
    // ⚠ r57：**有过滤条件时不显示这一栏**。用户点开胶囊筛「只看软件引擎」、
    //   或者在搜索框打了词，是在"把列表收窄"；这时顶上还杵着一栏不受任何
    //   过滤影响的"最近使用"，会把筛完的结果挤下去，观感就是"筛不干净"。
    //   默认视图（没有任何过滤）才是用户要它"默认显示"的那个场景。
    //   r51 探针 ③「点胶囊展开、选一个就只剩这一个」钉的正是这条 ——
    //   不加这个判断，那一节会从 1 变成 2。
    if (!qsb().engineFilterText() && !qsb().panelSearchMode() && !qsb().panelGroupFilter()
        && !qsb().panelSubFilter() && !qsb().panelCatFilter()) {
        renderRecentSection();
    }

    // —— 主体：平铺。胶囊行控制显示哪个大类；搜索模式只是同一份平铺的筛子 ——
    renderFlatBody(groups, qsb().panelGroupFilter());

    // 「一条都没命中」的占位。放在所有分类之后：靠 hidden 属性开关，
    // 平时完全不占位（[hidden]{display:none!important}）
    qsb().panelBody().appendChild(Object.assign(document.createElement('div'), {
        className: 'eng-noresult',
        hidden: true,
        textContent: '没有匹配的引擎（换个词试试，或者点「设置…」加一个）',
    }));

    /* ⚠ 每次重画都**显式**把滚动位置归零。
     *
     * 用户报「上次滚到的位置下次打开还在」。根因：qsb().panelBody() 是 index.html 里的
     * **常驻节点**，renderPanel 只换它的内容、不换节点；而 Chromium 在「同一个任务里
     * 先删空、再一股脑填回去」这种情况下，中间那次布局根本没发生，scrollHeight
     * 从未真正掉到 0 —— 于是滚动量既不会被自动钳制、也不会被重置，原样留到下一轮。
     * （这也解释了为什么"清空子节点"看起来没生效：确实没生效。）
     *
     * 所以必须显式置 0，而且放在**内容全部填完之后**：一是此时 scrollHeight 是终值、
     * 置 0 一定成立；二是避免置完之后再插入内容触发滚动锚定（scroll anchoring）
     * 把位置又拽回去。
     *
     * ⚠ 例外：`keepScroll` 时恢复的是**重画前的位置**（配置广播引起的重画不该
     *   动用户正在看的地方）。内容高度可能变了，浏览器自己会把值钳到合法范围内。 */
    qsb().panelBody().scrollTop = prevScroll;

    /* 重画之后把过滤词再应用一次：.cell 全是新建的、默认都"可见"，
     * 不重放一遍的话，过滤状态会在「改个设置触发的重画」之后静默消失。 */
    if (qsb().engineFilterText()) applyEngineFilter(qsb().engineFilterText());

    /* 打开面板时把光标送进引擎搜索框（只走 togglePanel 打开那条路）。 */
    if (focusFilter && qsb().inputEngFilter() && !qsb().panelEl().classList.contains('fav-mode')) {
        try { qsb().inputEngFilter().focus(); } catch (_) { }
    }

    // 渲染完之后，把当前滚动位置对应的小节标成「激活」，让用户看一眼就知道自己在哪段。
    syncActiveCatNav();
    // ⚠ 挂一次就够。qsb().panelBody() 是 index.html 里的常驻节点，renderPanel 只清内容、
    //   不换节点 —— 每次渲染都 add 一个监听的话，面板开 30 次滚一下就要跑 30 遍
    //   syncActiveCatNav（每遍两次 querySelectorAll + 读 offsetTop 强制重排）。
    if (!qsb().panelScrollBound()) {
        qsb().panelBody().addEventListener('scroll', syncActiveCatNav, { passive: true });
        /* 滚动**结束**（动画走完）→ 解除跳转锁并把控制权交还给 syncActiveCatNav。
         * `scrollend` 是 Chromium 114+ 才有的事件，老内核不认也无所谓：
         * scrollPanelToCat 里还有一个兜底超时，两条路谁先到算谁。 */
        qsb().panelBody().addEventListener('scrollend', endCatJump, { passive: true });
        qsb().setPanelScrollBound(true);
    }
}


