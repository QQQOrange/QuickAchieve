/* ================================================================== */
/* 「恢复本节默认」按钮（v2.23.100 从 settings.js 拆出）                 */
/*                                                                    */
/* 依赖由 settings.js 的 installCardResets 包装器传入：                 */
/*   defaults/current、SLIDERS、CONF/DEFAULTS 的读写、invoke、toast。  */
/* settings.js 仍是唯一持有这些状态的地方，本文件不保存任何配置。       */
/* ================================================================== */

window.QSCardReset = {
    /* 给每张带数字/开关控件的卡片装一颗「恢复本节默认」。 */
install(deps) {
        const { defaults, current, sliders, getConf, getDefaults, setDefaults, invoke, toast } = deps;
    const cards = document.querySelectorAll('.card');
    /* 滑块的“默认值”和 Rust `Config::default()` 不是一回事：
     * `SLIDERS` 里存的是**该样式下的默认**（磨砂强度默认 12，而全局默认配置
     * 是 0 = 默认样式）。恢复「磨砂」这一节应该回到 12，不是把磨砂关掉。
     * 开关类没有这种样式默认，仍用 Rust 的值。 */
    const sliderDefault = new Map();
    for (const it of sliders) {
        if (it && it.length >= 4) sliderDefault.set(String(it[2]), it[3]);
    }
    /* 样式默认值 ≠ 全局默认值：磨砂这一节的「默认」应该是**磨砂开着**的
     * 默认强度 12，而不是全局默认样式里的 0（0 会把磨砂关掉）。 */
    const styleDefault = { barBlur: 12 };
    /** 一张卡里可以“恢复默认”的控件 → [配置 key, 默认值]。 */
    const keysOf = (card, D) => {
        const out = [];
        for (const el of card.querySelectorAll('input[type="range"], input[type="checkbox"]')) {
            if (!/^s-/.test(el.id)) continue;
            const key = el.id.slice(2);
            if (el.type === 'range') {
                if (Object.prototype.hasOwnProperty.call(styleDefault, key)) {
                    out.push([key, styleDefault[key]]);
                } else if (sliderDefault.has(key)) {
                    out.push([key, sliderDefault.get(key)]);
                }
                continue;
            }
            if (!(key in D)) continue;
            if (typeof D[key] !== 'boolean') continue;
            out.push([key, D[key]]);
        }
        return out;
    };
    for (const card of cards) {
        if (card.querySelector('.card-reset')) continue;
        const h2 = card.querySelector('h2');
        if (!h2) continue;
        /* 控件是现成的；默认值**点的时候再取**。
         *
         * 为什么不在 boot 里先 await 一份：`defaults_view` 在某些启动时序下
         * 会先失败/返回空（页面已经用 v2.23.87 的启动续跑验证过），
         * 那会让整页按钮全部装不上。懒加载 + 失败提示更稳。 */
        if (!card.querySelector('input[type="range"], input[type="checkbox"]')) continue;
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'btn ghost card-reset';
        btn.textContent = '恢复本节默认';
        btn.title = '把这一节里改过的数字/开关恢复为默认值（不碰路径、凭据）';
        btn.addEventListener('click', async () => {
            const C = current || getConf();
            let D = defaults || getDefaults();
            if (!D) {
                try {
                    D = await invoke('defaults_view');
                    setDefaults(D);
                } catch (e) {
                    toast('读取默认值失败：' + String((e && e.message) || e));
                    return;
                }
            }
            if (!D || typeof D !== 'object') { toast('读取默认值失败'); return; }
            const pairs = [];
            for (const [key, def] of keysOf(card, D)) {
                const now = C ? C[key] : undefined;
                if (JSON.stringify(now) !== JSON.stringify(def)) pairs.push([key, def]);
            }
            if (!pairs.length) { toast('这一节已经是默认值'); return; }
            try {
                await invoke('set_settings', { pairs });
                toast('已恢复本节默认');
            } catch (e) {
                toast('恢复失败：' + String((e && e.message) || e));
            }
        });
        h2.appendChild(btn);
    }
}
};
