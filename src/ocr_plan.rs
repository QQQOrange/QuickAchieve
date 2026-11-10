//! OCR 计划与凭据判断（v2.23.94 从 main 拆出）。
//!
//! 这一块只依赖 `config` 与 `ocr_engines` 注册表：给"这一轮跑哪几家"和
//! "某家凭据齐不齐"的纯判断，不碰窗口、不碰剪贴板。`ocr_start_flow` 等
//! 编排逻辑留在 main。

use crate::{config, ocr_engines};


/// **一轮识别到底跑哪几家**（r47 起由「默认引擎 + 同时识别名单」派生）。
///
/// 顺序 = 默认引擎在前，其余按**注册表顺序**（不是用户勾选顺序）——
/// 结果页上下排列的顺序因此稳定，同一次识别里不会因为勾选先后而变。
///
/// ⚠ 老实现叫 `parallel_engines()`，返回写死的 `["windows", online]`：
///   ① 用户配了第三家（硅基流动/Kimi），并行那一路照样跑"本地 + 百度"；
///   ② 关掉并行开关后，结果页仍会冒出"本地识别"可点 —— 用户 r47 报的就是它：
///      「就算我打开了，也要我把本地的识别引擎添加在多引擎识别里面，它才可以」。
///   现在本地和别家一样，**被勾选才参与**。
pub(crate) fn engine_plan(cfg: &config::Config) -> Vec<String> {
    /* 0️⃣ 格子绑定了具体模型（`@ocr:<id>` / `@ocr:a+b`，见 `set_ocr_force_engine`）：
     *    这一轮**只跑这几家**（顺序 = 绑定顺序 = 结果窗里的先后）。
     *    用户原话是"一旦选定某一项，后续相关功能必须直接使用该已选定的具体项执行，
     *    而不是回退到默认值或其他选项"—— 所以这里连"同时识别"名单都不掺。
     *    全局配置不改：下一轮从横条/快捷键进来时 force 已被 `ocr_start` 清空。 */
    let forced = crate::ocr_forced_engines();
    if !forced.is_empty() {
        return forced;
    }
    let mut out: Vec<String> = Vec::new();
    let def = cfg.ocr_engine.clone();
    if ocr_engines::find(&def).is_some() {
        out.push(def.clone());
    }
    // 额外那几家：按注册表顺序挑，保证展示顺序稳定
    for e in ocr_engines::ENGINES {
        if e.id == def {
            continue;
        }
        if cfg.ocr_multi.iter().any(|x| x == e.id) {
            out.push(e.id.to_string());
        }
    }
    /* r76 兜底：上面两步一份都没凑出来（默认引擎那一栏是个注册表里没有的值，
     * 也没勾任何一家）时，补本地引擎。
     *
     * 这一步修的是一个"点 OCR 完全没反应"的真 bug：默认引擎无效 → 计划为空 →
     * 凭据检查拿 baidu 当"在线引擎"一问，用户当然没配密钥 → ocr_start_flow
     * 直接 return，**连框选遮罩都不开**。用户看到的就是"点了开始识别 /
     * 横条点 OCR 都没反应"。
     *
     * ⚠ 放在循环**之后**、且只在 out 为空时补：默认引擎就是本地时它已经在
     *    out 里了，再补一次会产出 ["windows","windows"]，结果页上出现两行
     *    一模一样的"本地识别"。 */
    if out.is_empty() && ocr_engines::find("windows").is_some() {
        out.push("windows".to_string());
    }
    out
}


/// 算出这一轮 OCR 到底跑哪几家，必要时**临时**补上本地引擎。
///
/// r77 修正：**只在这一次的名单里补，不动配置**（r76 那版会 `config::save`）。
///
/// 为什么改主意：落盘会在用户不知情时改他的设置。真机自检立刻抓到了后果 ——
/// 「同时识别」那颗 windows 胶囊本来是"额外参与的一家"，可一旦默认引擎被悄悄
/// 改成 windows，它就从"额外名单"里消失了（默认引擎本来就一定参与），
/// 用户看到的是"我勾了本地识别，但它没进同时识别名单"。那不是他做过的操作。
///
/// 现在的口径：
///   · 这一轮用本地跑（让功能可用），并返回 `true` 让调用方在日志/提示里说明；
///   · 用户的 `ocr_engine` **原样留着** —— 他配好百度密钥之后，下次点 OCR
///     自然还是走百度，不需要回来改设置。
pub(crate) fn plan_with_local_fallback(cfg: &config::Config) -> (Vec<String>, bool) {
    let run = engine_plan(cfg);
    /* ⚠ v2.11.2：**用户点的是哪一家，就跑哪一家 —— 绝不替换**。
     *
     * 用户口径（原话）：「一旦选定某一项，后续相关功能必须直接使用该已选定的
     * 具体项执行，而不是回退到默认值或其他选项」。
     *
     * 以前这里会把"缺凭据的绑定引擎"悄悄换成 Windows 本地：结果窗跑的是本地，
     * 用户绑了百度云却看到本地的界面 —— 表现就是「换成百度之后还是老样子」。
     * 现在缺凭据由 `ocr_start_flow` 直接说明（并把设置页打开到 OCR 那一栏），
     * 不再偷偷换人。 */
    if !crate::ocr_forced_engines().is_empty() {
        return (run, false);
    }
    /* 只要名单里**有一家凭据齐**就照原样跑，一点都不动。
     * ⚠ 这一句就覆盖了"用户已经勾了本地引擎"的情形 —— 本地永远 `creds_ready`，
     *   所以那种配置会在这里直接返回，走不到下面的插入分支（也就不会重复插）。 */
    if run.iter().any(|id| creds_ready(cfg, id)) {
        return (run, false);
    }
    if ocr_engines::find("windows").is_some() && !run.iter().any(|id| id == "windows") {
        let mut with_local = run;
        with_local.insert(0, "windows".to_string());
        return (with_local, true);
    }
    (run, false)
}


/// 这家引擎的凭据齐了吗 —— 要填哪几样由**注册表**决定，不在代码里写死
/// （不然每加一家引擎就要来这里补一个 if）。
pub(crate) fn creds_ready(cfg: &config::Config, id: &str) -> bool {
    let Some(info) = ocr_engines::find(id) else {
        return false;
    };
    if info.kind == ocr_engines::Kind::Local {
        return true;
    }
    let c = crate::ocr_creds(cfg, id);
    if info.need_key && c.key.trim().is_empty() {
        return false;
    }
    if info.need_secret && c.secret.trim().is_empty() {
        return false;
    }
    if info.need_model && c.model_for(info).trim().is_empty() {
        return false;
    }
    if info.need_base && c.base_for(info).trim().is_empty() {
        return false;
    }
    true
}


/// 设置里配的那个在线引擎；配了个不认识的（老配置 / 手改过文件）就退回百度。
pub(crate) fn online_engine(cfg: &config::Config) -> String {
    if ocr_engines::is_online(&cfg.ocr_engine) {
        cfg.ocr_engine.clone()
    } else {
        "baidu".to_string()
    }
}

