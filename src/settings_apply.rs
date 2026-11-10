//! 单个设置项写入（v2.23.112 从 main 拆出）。
//!
//! 只改 `config::Config`（部分项会 emit 事件 / 立即应用快捷键），
//! 不负责落盘与广播 —— 那些仍在 main 命令壳里。

use crate::config::normalize_recent_max;
use crate::t::Emitter;
use crate::{config, imgsearch, trans_engines};
use serde_json::Value;

pub(crate) fn apply_one_setting(cfg: &mut config::Config, key: &str, value: Value) -> Result<(), String> {
    match key {
        "triggerSelect" => cfg.trigger_select = value.as_bool().unwrap_or(true),
        "triggerCopy" => cfg.trigger_copy = value.as_bool().unwrap_or(false),
        // 右下角"正在使用什么功能"的通知（默认开）
        "notifyFeature" => cfg.notify_feature = value.as_bool().unwrap_or(true),
        // 提示显示几秒（1~15，默认 3）
        "notifySeconds" => {
            cfg.notify_seconds = value.as_u64().unwrap_or(3).clamp(1, 15) as u32;
        }
        /* 2026-10-23：横条"自己收起来"的两条阈值 + 误弹开关。
         * 数值一律夹在合法区间（界面是滑块，但 IPC 也可能来自探针/手改配置）。 */
        "barIdleSec" => {
            cfg.bar_idle_sec = value.as_u64().unwrap_or(5).min(300) as u32
        }
        "barAwayPx" => {
            cfg.bar_away_px = value.as_i64().unwrap_or(400).clamp(0, 4000) as i32
        }
        "barNeedText" => cfg.bar_need_text = value.as_bool().unwrap_or(true),
        /* v2.23.3：框选后是否停下给调整/换用途。**默认关** ——
         * 框完立刻执行你打开的那个功能（识别 / 识图 / 二维码），不再弹动作条。
         * 开着才有手柄 + 动作条（也才有"复制图片"那条路）。 */
        "regionAdjust" => cfg.region_adjust = value.as_bool().unwrap_or(false),
        /* v2.23.6：快捷键独占。关掉之后热键撞车时**两家都会响应**（老行为）。 */
        "hotkeyExclusive" => cfg.hotkey_exclusive = value.as_bool().unwrap_or(true),
        // v2.18.1：上次动作。白名单之外（手改、老页面）一律清空 = 按本次用途执行。
        "regionLastAction" => {
            cfg.region_last_action = crate::normalize_region_action(value.as_str().unwrap_or(""))
        }
        /* v2.14.0：触发方式（`always` / `ctrl`，互斥）。
         * 白名单与 sanitize 同一条口径；值不在白名单里（老页面、探针、手改）
         * 一律回 `always` —— 宁可多弹，也不能让横条因为这个值再也不弹。 */
        "barTriggerMode" => {
            cfg.bar_trigger_mode = config::normalize_bar_trigger_mode(value.as_str().unwrap_or(""))
        }
        // v1.5.4：弹出位置。白名单和 sanitize 同一条口径（config::normalize_bar_pos），
        // 脏值当场归一成 "above"，不会出现"界面选了下、存进去是别的"。
        "barPos" => {
            cfg.bar_pos = config::normalize_bar_pos(value.as_str().unwrap_or(""))
        }
        /* 网格页：「格子里显示名称还是只显示图标」。
         * 单独一个键（不并进 set_grid_behavior），因为它是"文字要不要显示"，
         * 和"启用 / 点击行为"是两回事，设置页里也是各自即时保存的。 */
        "gridShowLabel" => cfg.grid_show_label = value.as_bool().unwrap_or(true),
        /* v2.7.0：单项显示样式覆盖（`{ "<格子名>": "icon"|"text"|"both" }`）。
         * 整张表一次写回 —— 设置页那边是"改一项就重发整张表"，
         * 比逐项增删少一次"删除时要不要写空值"的判断。
         * 非法值在这里就丢掉（和 normalize_grid_pages 第 ⑥ 步同一口径）。 */
        "gridItemStyle" => {
            if let Ok(m) = serde_json::from_value::<std::collections::BTreeMap<String, String>>(
                value.clone(),
            ) {
                cfg.grid_item_style = m
                    .into_iter()
                    .filter_map(|(k, v)| {
                        let k = k.trim();
                        if k.is_empty() {
                            return None;
                        }
                        config::grid_item_style_valid(&v).map(|s| (k.to_string(), s.to_string()))
                    })
                    .collect();
            }
        }
        // 收藏夹整树（设置页本地编辑完一次性写回）
        "favorites" => {
            if let Ok(f) = serde_json::from_value::<Vec<config::FavGroup>>(value.clone()) {
                cfg.favorites = f;
                // 删目录的同时把横条上指向它的哨兵一起剪掉。
                // 不剪的话：横条本体（bar.js 会过滤）看着正常，但设置页
                // 「横条上的引擎」列表是照 bar_list 直出渲染的，会把
                // `@fav:xxxx` 这串 id 当引擎名显示出来 —— 用户报的"乱码"就是它。
                let pruned = config::prune_dangling_fav_sentinels(cfg);
                if pruned > 0 {
                    crate::win::log(&format!(
                        "set_setting: 收藏树变了，剪掉 {pruned} 个失效的横条哨兵"
                    ));
                }
            }
        }
        "favHotkey" => cfg.fav_hotkey = value.as_str().unwrap_or("").trim().to_string(),
        // 备份密码：非空时所有备份自动加密（见 crypto.rs）
        "backupPassword" => {
            cfg.backup_password = value.as_str().unwrap_or("").to_string();
        }
        // 本地备份目录：空 = 数据目录（默认）。.bak、WebDAV 同步前快照
        // 都落到这里（见 config::backup_dir_of）。
        "backupDir" => {
            cfg.backup_dir = value.as_str().unwrap_or("").trim().to_string();
        }
        // 收藏夹面板的手动尺寸（按目录记忆）
        "favPanelSizes" => {
            if let Ok(m) = serde_json::from_value::<
                std::collections::BTreeMap<String, Vec<f64>>,
            >(value.clone())
            {
                cfg.fav_panel_sizes = m;
            }
        }
        /* v2.6.9：网格页选板被拖出来的长宽（用户要求"调整之后以后都这样"）。
         * 收 [宽, 高]；这里再夹一次区间 —— 配置可能来自手改或旧备份，
         * 给个 0 或 99999 的话下次打开选板就没法用了。 */
        "pickSize" => {
            if let Ok(v) = serde_json::from_value::<Vec<u64>>(value.clone()) {
                let w = v.first().copied().unwrap_or(0).clamp(360, 4000) as u32;
                let h = v.get(1).copied().unwrap_or(0).clamp(260, 4000) as u32;
                if w > 0 && h > 0 {
                    cfg.pick_size = vec![w, h];
                }
            }
        }
        "autoStart" => {
            let on = value.as_bool().unwrap_or(false);
            cfg.auto_start = on;
            crate::win::set_autostart(on);
        }
        "iconSize" => {
            cfg.icon_size = value.as_u64().unwrap_or(30).clamp(14, 96) as u32;
        }
        "alpha" => {
            cfg.alpha = value.as_u64().unwrap_or(100).clamp(30, 100) as u32;
        }
        // 待机（未悬停）不透明度。和 alpha 共用一套 clamp 语义，只是下限放到
        // 10 —— 压得太低横条会"几乎看不见"，用户主动要求时也随他。
        "barDimAlpha" => {
            cfg.bar_dim_alpha = value.as_u64().unwrap_or(45).clamp(10, 100) as u32;
        }
        // 渐隐延迟（毫秒）：0 = 立即淡；上限 10 秒（和 sanitize 同一口径）。
        "barDimDelayMs" => {
            cfg.bar_dim_delay_ms = value.as_u64().unwrap_or(1500).min(10_000) as u32;
        }
        // 液态玻璃一组。set_setting 是白名单式的，不登记就是"看着有反应其实没存"。
        "barGlass" => cfg.bar_glass = value.as_bool().unwrap_or(true),
        "barGlassRefraction" => {
            cfg.bar_glass_refraction = value.as_u64().unwrap_or(20).clamp(0, 60) as u32;
        }
        "barGlassBevel" => {
            cfg.bar_glass_bevel = value.as_u64().unwrap_or(12).clamp(0, 30) as u32;
        }
        "barGlassFrost" => {
            cfg.bar_glass_frost = value.as_u64().unwrap_or(0).clamp(0, 12) as u32;
        }
        "barGlassAberration" => {
            cfg.bar_glass_aberration = value.as_u64().unwrap_or(35).clamp(0, 100) as u32;
        }
        "barGlassSpec" => {
            cfg.bar_glass_spec = value.as_u64().unwrap_or(100).clamp(0, 200) as u32;
        }
        "barGlassTint" => {
            cfg.bar_glass_tint = value.as_u64().unwrap_or(12).clamp(0, 100) as u32;
        }
        "barGlassLive" => cfg.bar_glass_live = value.as_bool().unwrap_or(false),
        // v1.3.0：实时刷新帧率上限。允许 0（跟随）/ 30 / 60 / 120。
        // 夹到合法集合里：别的值一律当 0（跟随显示器），免得手改配置写出个怪值。
        "barGlassFps" => {
            let v = value.as_u64().unwrap_or(0) as u32;
            cfg.bar_glass_fps = if matches!(v, 0 | 30 | 60 | 120) { v } else { 30 };
        }
        "iconStyle" => {
            let s = value.as_str().unwrap_or("auto");
            cfg.icon_style = if s == "letter" { "letter".to_string() } else { "auto".to_string() };
        }
        // 「统一图标大小」。⚠ set_setting 是**白名单式**的：不在这里登记，
        //   前端点开关会「看着有反应、其实没存」—— 页内自检抓到过一次。
        "unifyIcons" => cfg.unify_icons = value.as_bool().unwrap_or(true),
        "itemRadius" => {
            cfg.item_radius = value.as_u64().unwrap_or(9).clamp(0, 18) as u32;
        }
        "theme" => {
            let s = value.as_str().unwrap_or("system");
            cfg.theme = match s {
                "light" | "dark" | "page" => s.to_string(),
                _ => "system".to_string(),
            };
        }
        "barPadding" => {
            cfg.bar_padding = value.as_u64().unwrap_or(5).clamp(0, 12) as u32;
        }
        "animations" => cfg.animations = value.as_bool().unwrap_or(true),
        "shadow" => cfg.shadow = value.as_bool().unwrap_or(false),
        "barBlur" => {
            cfg.bar_blur = value.as_u64().unwrap_or(0).clamp(0, 32) as u32;
        }
        /* v2.23.83：横条底板（非图标部分）的不透明度。只在磨砂样式下生效，
         * 但阈值本身任何时候都允许改（切回磨砂时立即按新值画）。 */
        "barBgAlpha" => {
            cfg.bar_bg_alpha = value.as_u64().unwrap_or(55).min(100) as u32;
        }
        "anchorAtMouse" => cfg.anchor_at_mouse = value.as_bool().unwrap_or(false),
        "delayMs" => {
            cfg.delay_ms = value.as_u64().unwrap_or(0).min(5000) as u32;
        }
        "filterMode" => {
            let m = value.as_str().unwrap_or("off");
            cfg.filter_mode = match m {
                "blacklist" | "whitelist" => m.to_string(),
                _ => "off".to_string(),
            };
        }
        "filterList" => {
            cfg.filter_list = value
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default();
        }
        "ocrLang" => {
            // 归一而不是放行：设置页以前给过「纯中文 = CHN」，百度根本不认。
            cfg.ocr_lang = crate::ocr::normalize_lang(value.as_str().unwrap_or("")).to_string();
        }
        // 默认 false = 标准版（每日 500 次免费额度）。默认 true 会让没开通
        // 付费的账号一识别就报「余额不足」，看着像 Key 不对，非常难自查。
        "ocrAccurate" => cfg.ocr_accurate = value.as_bool().unwrap_or(false),
        "ocrEngine" => {
            let e = value.as_str().unwrap_or("baidu").trim().to_string();
            if crate::ocr_engines::find(&e).is_some() {
                cfg.ocr_engine = e;
            }
        }
        // 双引擎并行：默认关。开了之后点一次识别，本地 + 百度同时跑，
        // 结果窗里两个结果上下排布、各自标注（见 ocr_region 的并行分支）。
        "ocrParallel" => cfg.ocr_parallel = value.as_bool().unwrap_or(false),
        // r49：结果窗固定在默认引擎那一档（默认**开** —— 用户要求的默认口径）。
        "ocrStayDefault" => cfg.ocr_stay_default = value.as_bool().unwrap_or(true),
        // OCR 文本整理的默认开关（需求 7，设置页「文字识别」里那几项）。
        // ⚠ set_setting 是白名单式的，不登记就是"看着有反应其实没存"。
        "ocrCleanGarbage" => cfg.ocr_clean_garbage = value.as_bool().unwrap_or(false),
        "ocrCleanSpace" => {
            let v = value.as_str().unwrap_or("smart");
            cfg.ocr_clean_space = match v {
                "off" | "force" => v.to_string(),
                _ => "smart".to_string(),
            };
        }
        "ocrCleanWidth" => cfg.ocr_clean_width = value.as_bool().unwrap_or(false),
        "ocrCleanUnwrap" => cfg.ocr_clean_unwrap = value.as_bool().unwrap_or(false),
        "ocrEnable" => cfg.ocr_enable = value.as_bool().unwrap_or(true),
        "ocrKey" => cfg.ocr_key = value.as_str().unwrap_or("").trim().to_string(),
        "ocrSecret" => cfg.ocr_secret = value.as_str().unwrap_or("").trim().to_string(),
        "autoElevate" => {
            let on = value.as_bool().unwrap_or(true);
            cfg.auto_elevate = on;
            /* v2.11.8：**同时**记到本机标记文件里。
             * 配置文件会被云端整份恢复、也会被前端保存的旧快照覆盖，
             * 而"这台机器要不要管理员"是纯粹的本地决定，不该跟着配置走。 */
            crate::write_local_elevate_pref(on);
        }
        // r80：AI 抠图开关。默认 false —— 不开时走本地算法，不该替用户默认开启
        //      （它需要额外下载模型，见 config.rs 字段上的取舍说明）。
        "aiMattingEnable" => cfg.ai_matting_enable = value.as_bool().unwrap_or(false),
        // r80g：模型目录。⚠ 这里只落配置，**不做迁移** ——
        // 迁移必须与"选目录"同一个动作（见 ai_model_dir_pick 的注释）。
        "aiModelDir" => cfg.ai_model_dir = value.as_str().unwrap_or("").to_string(),
        "webdavOnStart" => cfg.webdav_on_start = value.as_bool().unwrap_or(false),
        "webdavOnExit" => cfg.webdav_on_exit = value.as_bool().unwrap_or(false),
        // v1.2.0：自动更新。⚠ `set_setting` 是白名单式的 —— 不在这里登记，
        // 前端勾选框「看着有反应、其实没存」（这个坑页内自检抓到过一次）。
        "autoUpdate" => cfg.auto_update = value.as_bool().unwrap_or(true),
        /* GitHub 个人访问令牌（v1.5.7）。存原样，读的时候由
         * `update::effective_token` 兼容 `用户名:令牌` 老格式；
         * 这里只做一次 trim（前端可能带空格）。
         * ⚠ 绝不写日志 —— 这是凭据。 */
        "githubToken" => cfg.github_token = value.as_str().unwrap_or("").trim().to_string(),
        /* v1.5.16：翻译（多引擎）。凭据的四样（Key/模型/base）走
         * `trans_set_cred` 那个专门命令（和 OCR 的 ocr_set_cred 一样：
         * 空串 = 删掉这一项，"清空密钥"才能正常工作），这里只留
         * 开关 / 默认引擎 / 目标语言这三个单值。 */
        "transEnable" => {
            let on = value.as_bool().unwrap_or(true);
            cfg.trans_enable = on;
            /* v2.23.80：总开关和 `@trans` 哨兵是同一件事的两面（和 imgEnable
             * 那段同一个口径）。关掉 → 横条上不该再画翻译；打开 → 只有用户
             * 没把它移出过（`trans_add_to_bar`）才补回去，绝不能把用户主动
             * 移出的按钮又塞回横条。 */
            let has = cfg.bar_list.iter().any(|n| n == config::BAR_TRANS);
            if !on && has {
                cfg.bar_list.retain(|n| n != config::BAR_TRANS);
            } else if on && !has && cfg.trans_add_to_bar {
                cfg.bar_list.push(config::BAR_TRANS.to_string());
            }
        }
        "transEngine" => {
            let e = value.as_str().unwrap_or("siliconflow").trim().to_string();
            if trans_engines::find(&e).is_some() {
                cfg.trans_engine = e;
            }
        }
        "transTarget" => {
            cfg.trans_target = crate::trans::normalize_lang(value.as_str().unwrap_or(""))
        }
        "webdavUrl" => {
            // 单键修改走的是设置页实时保存。这里只做「补协议 + 补尾斜杠」，
            // **不**把软件子目录写进配置（用户要求：地址栏只填根地址）。
            cfg.webdav_url = crate::webdav::normalize_input(value.as_str().unwrap_or(""));
        }
        "webdavUser" => cfg.webdav_user = value.as_str().unwrap_or("").trim().to_string(),
        "webdavPass" => cfg.webdav_pass = value.as_str().unwrap_or("").to_string(),
        "aiSuffix" => cfg.ai_suffix = value.as_str().unwrap_or("").to_string(),
        "sepStyle" => {
            let v = value.as_str().unwrap_or("line");
            cfg.sep_style = if matches!(v, "line" | "dot" | "space") {
                v.to_string()
            } else {
                "line".to_string()
            };
        }
        "aiCopyPrompt" => cfg.ai_copy_prompt = value.as_bool().unwrap_or(true),
        "aiSuffixOf" => {
            // value = { "engine": "...", "suffix": "..." }
            let eng = value.get("engine").and_then(|x| x.as_str()).unwrap_or("");
            let sfx = value.get("suffix").and_then(|x| x.as_str()).unwrap_or("");
            if !eng.is_empty() {
                if sfx.trim().is_empty() {
                    // 清空 = 回到全局默认
                    cfg.ai_suffixes.remove(eng);
                } else {
                    cfg.ai_suffixes.insert(eng.to_string(), sfx.to_string());
                }
            }
        }
        "urlOverride" => {
            let eng = value.get("engine").and_then(|x| x.as_str()).unwrap_or("");
            let url = value.get("url").and_then(|x| x.as_str()).unwrap_or("").trim();
            if !eng.is_empty() {
                if url.is_empty() {
                    cfg.url_overrides.remove(eng);
                } else if url.contains("%s") {
                    cfg.url_overrides.insert(eng.to_string(), url.to_string());
                } else {
                    let _ = crate::app().emit_to(
                        "settings",
                        "qs://toast",
                        "链接模板里必须带 %s（它代表选中的文字），没保存",
                    );
                    return Err("链接模板里必须带 %s".into());
                }
            }
        }
        // ---------------- r54 识图搜索 ----------------
        // ⚠ set_setting 是**白名单式**的：不登记就是「设置页点着有反应、其实没存」。
        //    页内自检专门盯这一类（ocrClean* 那组就这么栽过一次）。
        /* r72：总开关关掉就当"不在横条上"（哨兵一起剪掉），打开且「加到横条」
         * 也开着就补回哨兵 —— 和下面 imgAddToBar 那段同一个口径。 */
        "imgEnable" => {
            let on = value.as_bool().unwrap_or(true);
            cfg.img_enable = on;
            let has = cfg.bar_list.iter().any(|n| n == config::BAR_IMG);
            if !on && has {
                cfg.bar_list.retain(|n| n != config::BAR_IMG);
            } else if on && !has && cfg.img_add_to_bar {
                cfg.bar_list.push(config::BAR_IMG.to_string());
            }
        }
        /* r72：识图按钮改成 bar_list 里的一等成员（`@img` 哨兵），位置/拖拽/
         * 移出都走和普通引擎同一套。开关和哨兵是同一件事的两面，必须一起改 ——
         * 只改开关会出现「开关说在横条上、列里没有它」（用户点了移出却还在），
         * 或反过来「列里有、开关说没开」（按钮灰着但横条上有）。
         * 什么时候才真加：总开关也开着（imgEnable）。否则加进去也画不出来。 */
        "imgKeepClip" => cfg.img_keep_clip = value.as_bool().unwrap_or(false),
        "imgAddToBar" => {
            let on = value.as_bool().unwrap_or(true);
            cfg.img_add_to_bar = on;
            let has = cfg.bar_list.iter().any(|n| n == config::BAR_IMG);
            if on && !has && cfg.img_enable {
                cfg.bar_list.push(config::BAR_IMG.to_string());
            } else if !on && has {
                cfg.bar_list.retain(|n| n != config::BAR_IMG);
            }
        }
        "imgDefaultEngine" => {
            // 只收注册表里认识的 id；空串合法（=「不设默认」，回到多选名单）
            let e = value.as_str().unwrap_or("").trim().to_string();
            if e.is_empty() || imgsearch::find(&e).is_some() {
                cfg.img_default_engine = e;
            }
        }
        "imgEngines" => {
            // 统一过 normalize_list：只留认识的、去重、按注册表顺序排
            let list = value
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str())
                        .map(|s| s.trim().to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            cfg.img_engines = imgsearch::normalize_list(&list);
        }
        "imgPublicUrl" => {
            let u = value.as_str().unwrap_or("").trim().to_string();
            // 只要 http(s) 开头的；别的（file:// 、本地路径、乱填的）一律清空，
            // 免得 Url 类引擎拿它拼出一个注定失败的请求。
            cfg.img_public_url = if u.starts_with("http://") || u.starts_with("https://") {
                u
            } else {
                String::new()
            };
        }
        /* r76：AI 识图的三个可调项（用户原话见 config.rs 那几个字段的注释）。
         * 延迟钳 0..=60000：设置页会给步进按钮，但 IPC 这条路上来的值不可信 ——
         * 一个 4e9 能把横条线程睡几天。 */
        "imgAiPasteDelay" => {
            cfg.img_ai_paste_delay = (value.as_u64().unwrap_or(1200) as u32).min(60_000);
        }
        "imgAiEnterDelay" => {
            cfg.img_ai_enter_delay = (value.as_u64().unwrap_or(900) as u32).min(60_000);
        }
        "imgAiFirst" => {
            cfg.img_ai_first = value.as_bool().unwrap_or(true);
        }
        "imgAiAutoOffOnMulti" => {
            cfg.img_ai_auto_off_on_multi = value.as_bool().unwrap_or(true);
        }
        // r56：最近使用那一栏摆几个。只认 5/10/15/20 四档（设置页就给这四个），
        // 别的值退回默认 10 —— 不静默接受 0（"一个都不显示"看着像坏了）
        // 也不接受 1000（那等于把全表又抄一遍）。
        "recentMax" => {
            cfg.recent_max = normalize_recent_max(value.as_u64().unwrap_or(10) as usize);
        }
        // v1.5.13：界面语言。归一只认 zh / en（config::normalize_ui_lang）；
        // 落盘之后由 broadcast() 把新值带给所有窗口，各页面自己换文案。
        "uiLang" => {
            cfg.ui_lang = config::normalize_ui_lang(value.as_str().unwrap_or("zh"));
        }
        /* v1.5.13：启动密码。
         *
         * 传进来的 `value` 是**明文**（用户刚输的那串），这里立刻算成 Argon2id
         * 哈希再存；原文不落盘、不进日志、不出这个函数。空串 = 关闭功能。
         *
         * ⚠ 这条**只在设置页"设密码 / 改密码"时**由用户显式操作触发。
         *   它和 backupPassword 是两条完全独立的路（用户要求互不影响）：
         *   改这里绝不会碰备份密钥，改备份密码也不会碰这里的哈希。 */
        "startupPassword" => {
            let plain = value.as_str().unwrap_or("");
            /* v1.5.14：**空串不再等于"关闭"**。
             *
             * 用户要求：「要取消密码，必须要再输一次解锁密码」。清哈希这条路
             * 因此只留 `startup_password_disable`（先验后清）；从 set_setting
             * 传空值一律拒绝 —— 否则一句 set_setting 就能把保护卸掉，
             * "再输一次密码"就成了摆设。 */
            if plain.trim().is_empty() {
                return Err("关闭启动密码要先输入当前密码".to_string());
            }
            cfg.startup_password_hash = crate::startup_password_make_hash(plain)?;
        }
        // 名单本身由**后端**在每次引擎被点时自己追加（note_recent_engine），
        // 不接受前端直接写 —— 否则"最近使用"的语义就没了（前端可以随便塞）。
        _ => return Err(format!("未知设置键：{key}")),
    }
    Ok(())
}
