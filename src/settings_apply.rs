//! 单个设置项写入（v2.23.112 从 main 拆出）。
//!
//! 只改 `crate::config::Config`（部分项会 emit 事件 / 立即应用快捷键），
//! 不负责落盘与广播 —— 那些仍在 main 命令壳里。

use crate::config::normalize_recent_max;
use crate::t::Emitter;
use serde_json::Value;

pub(crate) fn apply_one_setting(cfg: &mut crate::config::Config, key: &str, value: Value) -> Result<(), String> {
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
            cfg.bar_trigger_mode = crate::config::normalize_bar_trigger_mode(value.as_str().unwrap_or(""))
        }
        // v1.5.4：弹出位置。白名单和 sanitize 同一条口径（crate::config::normalize_bar_pos），
        // 脏值当场归一成 "above"，不会出现"界面选了下、存进去是别的"。
        "barPos" => {
            cfg.bar_pos = crate::config::normalize_bar_pos(value.as_str().unwrap_or(""))
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
                        crate::config::grid_item_style_valid(&v).map(|s| (k.to_string(), s.to_string()))
                    })
                    .collect();
            }
        }
        // 收藏夹整树（设置页本地编辑完一次性写回）
        "favorites" => {
            if let Ok(f) = serde_json::from_value::<Vec<crate::config::FavGroup>>(value.clone()) {
                cfg.favorites = f;
                // 删目录的同时把横条上指向它的哨兵一起剪掉。
                // 不剪的话：横条本体（bar.js 会过滤）看着正常，但设置页
                // 「横条上的引擎」列表是照 bar_list 直出渲染的，会把
                // `@fav:xxxx` 这串 id 当引擎名显示出来 —— 用户报的"乱码"就是它。
                let pruned = crate::config::prune_dangling_fav_sentinels(cfg);
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
        // 都落到这里（见 crate::config::backup_dir_of）。
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
            let has = cfg.bar_list.iter().any(|n| n == crate::config::BAR_TRANS);
            if !on && has {
                cfg.bar_list.retain(|n| n != crate::config::BAR_TRANS);
            } else if on && !has && cfg.trans_add_to_bar {
                cfg.bar_list.push(crate::config::BAR_TRANS.to_string());
            }
        }
        "transEngine" => {
            let e = value.as_str().unwrap_or("siliconflow").trim().to_string();
            if crate::trans_engines::find(&e).is_some() {
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
            let has = cfg.bar_list.iter().any(|n| n == crate::config::BAR_IMG);
            if !on && has {
                cfg.bar_list.retain(|n| n != crate::config::BAR_IMG);
            } else if on && !has && cfg.img_add_to_bar {
                cfg.bar_list.push(crate::config::BAR_IMG.to_string());
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
            let has = cfg.bar_list.iter().any(|n| n == crate::config::BAR_IMG);
            if on && !has && cfg.img_enable {
                cfg.bar_list.push(crate::config::BAR_IMG.to_string());
            } else if !on && has {
                cfg.bar_list.retain(|n| n != crate::config::BAR_IMG);
            }
        }
        "imgDefaultEngine" => {
            // 只收注册表里认识的 id；空串合法（=「不设默认」，回到多选名单）
            let e = value.as_str().unwrap_or("").trim().to_string();
            if e.is_empty() || crate::imgsearch::find(&e).is_some() {
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
            cfg.img_engines = crate::imgsearch::normalize_list(&list);
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
        // v1.5.13：界面语言。归一只认 zh / en（crate::config::normalize_ui_lang）；
        // 落盘之后由 broadcast() 把新值带给所有窗口，各页面自己换文案。
        "uiLang" => {
            cfg.ui_lang = crate::config::normalize_ui_lang(value.as_str().unwrap_or("zh"));
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

pub(crate) fn apply_view(v: &Value, cfg: &mut crate::config::Config) {
    let b = |k: &str| v.get(k).and_then(|x| x.as_bool());
    let n = |k: &str| v.get(k).and_then(|x| x.as_u64());
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(|x| x.to_string());

    /* 智能直达：备份/导入恢复。
     *
     * 与别的键一样是**整段替换**（恢复备份的语义就是"回到那个时间点"），
     * 但不做盲信：非法条目跳过、同类型同目标去重、总量封顶 200 条 ——
     * 手改过的备份、或别的版本导出的文件，都不该让配置变得不可用。 */
    if let Some(arr) = v.get("shortcuts").and_then(|x| x.as_array()) {
        let mut list: Vec<crate::config::Shortcut> = Vec::new();
        for it in arr.iter().take(400) {
            let Ok(mut sc) = serde_json::from_value::<crate::config::Shortcut>(it.clone()) else {
                continue;
            };
            // 三样缺一不可：id 用来钉横条、target 是目标、kind 决定怎么打开
            if sc.id.trim().is_empty() || sc.target.trim().is_empty() {
                continue;
            }
            if !matches!(sc.kind.as_str(), "app" | "url" | "file") {
                continue;
            }
            sc.name = if sc.name.trim().is_empty() {
                sc.target.clone()
            } else {
                sc.name.trim().to_string()
            };
            if list.iter().any(|x| x.id == sc.id) {
                continue; // id 撞了：后来的丢掉，避免横条哨兵指到两条
            }
            if list
                .iter()
                .any(|x| x.kind == sc.kind && crate::go_same_target(&x.target, &sc.target))
            {
                continue; // 同类型同目标 = 重复条目
            }
            if list.len() >= 200 {
                break;
            }
            list.push(sc);
        }
        cfg.shortcuts = list;
    }
    if let Some(x) = b("triggerSelect") { cfg.trigger_select = x; }
    if let Some(x) = b("triggerCopy") { cfg.trigger_copy = x; }
    // 网格页：备份/导入恢复。夹取到合法范围，避免手改过的备份把界面弄崩
    if let Some(x) = s("gridHotkey") {
        // v2.12.6：老备份里可能是 ctrl+c —— 那是系统复制，导入时直接丢掉
        // （配置保持原样），免得一份旧备份又把"复制被抢"这件事带回来。
        if crate::parse_hotkey(&x).is_some() && crate::config::grid_hotkey_allowed(&x) {
            cfg.grid_hotkey = x;
        }
    }
    if let Some(x) = b("hotkeyExclusive") { cfg.hotkey_exclusive = x; }
    if let Some(x) = n("gridRows") { cfg.grid_rows = x.clamp(1, 10) as u32; }
    if let Some(x) = n("gridCols") { cfg.grid_cols = x.clamp(1, 10) as u32; }
    if let Some(x) = n("gridCellW") { cfg.grid_cell_w = x.clamp(40, 260) as u32; }
    if let Some(x) = n("gridCellH") { cfg.grid_cell_h = x.clamp(40, 200) as u32; }
    if let Some(x) = n("gridGap") { cfg.grid_gap = x.clamp(0, 24) as u32; }
    if let Some(x) = n("gridRadius") { cfg.grid_radius = x.clamp(0, 24) as u32; }
    if let Some(x) = b("gridEnable") { cfg.grid_enable = x; }
    if let Some(x) = s("gridClick") {
        let v = x.trim().to_ascii_lowercase();
        if v == "auto" || v == "home" {
            cfg.grid_click = v;
        }
    }
    if let Some(x) = b("gridAllowAdd") { cfg.grid_allow_add = x; }
    /* ── 多页网格 ──────────────────────────────────────────────────────
     * config_view 把网格拆成 `gridPages` / `gridDefaultPage` / `gridAppPages`
     * 三份发出去，备份（backup_payload = config_view）里也确实带着它们；
     * 但这里以前**一个都没读**，只认扁平的 gridItems。
     *
     * 而 gridItems 也不是另一条能用的路：随后执行 `crate::config::sanitize` 时，
     * `normalize_grid_pages` 会用**本机旧的** grid_pages 把 grid_list 再写回去，
     * 刚灌进去的内容立刻被覆盖。结果是「恢复成功，但我的网格页一个都没回来」。
     *
     * 顺序要紧：先灌页结构，再决定要不要动 grid_list。 */
    let had_pages = v.get("gridPages").and_then(|x| x.as_array()).is_some();
    if let Some(arr) = v.get("gridPages").and_then(|x| x.as_array()) {
        if let Ok(list) =
            serde_json::from_value::<Vec<crate::config::GridPage>>(Value::Array(arr.clone()))
        {
            if !list.is_empty() {
                cfg.grid_pages = list;
            }
        }
    }
    if let Some(x) = s("gridDefaultPage") {
        cfg.grid_default_page = x;
    }
    if let Some(arr) = v.get("gridAppPages").and_then(|x| x.as_array()) {
        if let Ok(list) =
            serde_json::from_value::<Vec<crate::config::GridAppPage>>(Value::Array(arr.clone()))
        {
            cfg.grid_app_pages = list;
        }
    }
    if let Some(x) = b("gridShowLabel") { cfg.grid_show_label = x; }
    if let Some(obj) = v.get("gridItemStyle").and_then(|x| x.as_object()) {
        let mut m = std::collections::BTreeMap::new();
        for (k, val) in obj {
            let style = val.as_str().unwrap_or("").trim().to_ascii_lowercase();
            if style == "icon" || style == "text" || style == "both" {
                m.insert(k.clone(), style);
            }
        }
        cfg.grid_item_style = m;
    }
    /* 老备份（没有 gridPages）才按扁平列表灌；有页结构时 grid_list 由
     * sanitize 的 normalize_grid_pages 从默认页派生，别再覆盖它。 */
    if !had_pages {
        if let Some(arr) = v.get("gridItems").and_then(|x| x.as_array()) {
            cfg.grid_list = arr
                .iter()
                .filter_map(|x| x.as_str())
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .take(60)
                .collect();
        }
    }
    /* 按应用的横条专项（同一页里编辑的那份） */
    if let Some(arr) = v.get("barAppItems").and_then(|x| x.as_array()) {
        if let Ok(list) =
            serde_json::from_value::<Vec<crate::config::BarAppItems>>(Value::Array(arr.clone()))
        {
            cfg.bar_app_items = list;
        }
    }
    if let Some(x) = s("barPos") { cfg.bar_pos = x; }
    if let Some(x) = b("notifyFeature") { cfg.notify_feature = x; }
    if let Some(x) = n("notifySeconds") { cfg.notify_seconds = x.clamp(0, 60) as u32; }
    if let Some(x) = s("regionLastAction") { cfg.region_last_action = x; }
    if let Some(arr) = v.get("pickSize").and_then(|x| x.as_array()) {
        let list: Vec<u32> = arr
            .iter()
            .filter_map(|x| x.as_u64())
            .map(|x| x.clamp(16, 256) as u32)
            .collect();
        if !list.is_empty() {
            cfg.pick_size = list;
        }
    }
    if let Some(x) = b("autoStart") { cfg.auto_start = x; }
    if let Some(x) = b("autoElevate") { cfg.auto_elevate = x; }
    // r80：AI 抠图开关（批量导入那条路）
    if let Some(x) = b("aiMattingEnable") { cfg.ai_matting_enable = x; }
    if let Some(x) = s("aiModelDir") { cfg.ai_model_dir = x.to_string(); }
    if let Some(x) = n("iconSize") { cfg.icon_size = x.clamp(14, 96) as u32; }
    if let Some(x) = n("alpha") { cfg.alpha = x.clamp(30, 100) as u32; }
    // 待机透明度允许压得更低（最低 10%），但夹在 10-100：0% 会让横条直接
    // 消失看不见，和「它在那儿只是淡」是两回事。
    if let Some(x) = n("barDimAlpha") { cfg.bar_dim_alpha = x.clamp(10, 100) as u32; }
    if let Some(x) = n("barDimDelayMs") { cfg.bar_dim_delay_ms = x.min(10_000) as u32; }
    // 玻璃一组：恢复备份时原样灌回（老备份没有这些键 → 保持默认，不强行开）
    if let Some(x) = b("barGlass") { cfg.bar_glass = x; }
    if let Some(x) = n("barGlassRefraction") { cfg.bar_glass_refraction = x.clamp(0, 60) as u32; }
    if let Some(x) = n("barGlassBevel") { cfg.bar_glass_bevel = x.clamp(0, 30) as u32; }
    if let Some(x) = n("barGlassFrost") { cfg.bar_glass_frost = x.clamp(0, 12) as u32; }
    if let Some(x) = n("barGlassAberration") { cfg.bar_glass_aberration = x.clamp(0, 100) as u32; }
    if let Some(x) = n("barGlassSpec") { cfg.bar_glass_spec = x.clamp(0, 200) as u32; }
    if let Some(x) = n("barGlassTint") { cfg.bar_glass_tint = x.clamp(0, 100) as u32; }
    if let Some(x) = b("barGlassLive") { cfg.bar_glass_live = x; }
    // v1.3.0：实时刷新帧率上限。白名单夹取（只认 0/30/60/120），
    // 别的值一律当 0（跟随显示器）—— 免得手改配置写出个怪值。
    if let Some(x) = n("barGlassFps") {
        let v = x as u32;
        cfg.bar_glass_fps = if matches!(v, 0 | 30 | 60 | 120) { v } else { 30 };
    }
    if let Some(x) = n("barPadding") { cfg.bar_padding = x.clamp(0, 12) as u32; }
    if let Some(x) = n("itemRadius") { cfg.item_radius = x.clamp(0, 18) as u32; }
    if let Some(x) = s("theme") {
        cfg.theme = match x.as_str() {
            "light" | "dark" => x,
            _ => "system".to_string(),
        };
    }
    if let Some(x) = b("unifyIcons") { cfg.unify_icons = x; }
    if let Some(x) = b("animations") { cfg.animations = x; }
    if let Some(x) = b("shadow") { cfg.shadow = x; }
    if let Some(x) = n("barBlur") { cfg.bar_blur = x.clamp(0, 32) as u32; }
    if let Some(x) = n("barBgAlpha") { cfg.bar_bg_alpha = x.clamp(0, 100) as u32; }
    if let Some(x) = b("anchorAtMouse") { cfg.anchor_at_mouse = x; }
    if let Some(x) = n("delayMs") { cfg.delay_ms = x.clamp(0, 5000) as u32; }
    /* 恢复备份：老备份里没有这三个键 → 保持当前默认值（不要硬写 0，
     * 那等于把"自动收起"给用户关掉了）。 */
    if let Some(x) = n("barIdleSec") { cfg.bar_idle_sec = x.clamp(0, 300) as u32; }
    if let Some(x) = n("barAwayPx") { cfg.bar_away_px = x.clamp(0, 4000) as i32; }
    if let Some(x) = b("barNeedText") { cfg.bar_need_text = x; }
    /* v2.16.0 引入、v2.23.3 改默认。老备份里**没有这个键**就是老版本留下的 →
     * 保持当前值（默认 **false** = 框完直接执行），不要因为恢复一份旧备份
     * 就把用户刚关掉的"先停一下"又打开。 */
    if let Some(x) = b("regionAdjust") { cfg.region_adjust = x; }
    /* v2.14.0：触发方式。老备份里没有这个键 → 保持当前值（= always），
     * 不要因为恢复一份 2026 年之前的备份就把用户改成"必须按 Ctrl"。 */
    if let Some(x) = s("barTriggerMode") {
        cfg.bar_trigger_mode = crate::config::normalize_bar_trigger_mode(&x);
    }
    if let Some(x) = s("filterMode") {
        cfg.filter_mode = match x.as_str() {
            "blacklist" | "whitelist" => x,
            _ => "off".to_string(),
        };
    }
    if let Some(a) = v.get("filterList").and_then(|x| x.as_array()) {
        cfg.filter_list = a.iter().filter_map(|x| x.as_str()).map(|s| s.to_string()).collect();
    }
    if let Some(x) = s("ocrLang") {
        // 归一交给 crate::ocr::normalize_lang：百度没有「纯中文」这个值，
        // 老配置里的 CHN 留着会一直让识别失败。
        cfg.ocr_lang = crate::ocr::normalize_lang(&x).to_string();
    }
    if let Some(x) = b("ocrAccurate") { cfg.ocr_accurate = x; }
    if let Some(x) = s("ocrEngine") {
        let e = x.trim().to_string();
        if crate::ocr_engines::find(&e).is_some() { cfg.ocr_engine = e; }
    }
    if let Some(x) = b("ocrParallel") { cfg.ocr_parallel = x; }
    // r49：结果窗固定在默认引擎那一档（跟着备份走，是用户偏好）
    if let Some(x) = b("ocrStayDefault") { cfg.ocr_stay_default = x; }
    // r47：「同时识别」名单跟着备份走（是用户偏好，不是逐机器的东西）。
    // 只收注册表里认识的 id，别的丢弃 —— 备份可能来自装了更多引擎的版本。
    if let Some(arr) = v.get("ocrMulti").and_then(|x| x.as_array()) {
        cfg.ocr_multi = arr
            .iter()
            .filter_map(|x| x.as_str())
            .filter(|id| crate::ocr_engines::find(id).is_some())
            .map(|s| s.to_string())
            .collect();
    }
    if let Some(x) = b("ocrCleanGarbage") { cfg.ocr_clean_garbage = x; }
    if let Some(x) = s("ocrCleanSpace") {
        let v = x.trim().to_string();
        if matches!(v.as_str(), "off" | "smart" | "force") { cfg.ocr_clean_space = v; }
    }
    if let Some(x) = b("ocrCleanWidth") { cfg.ocr_clean_width = x; }
    if let Some(x) = b("ocrCleanUnwrap") { cfg.ocr_clean_unwrap = x; }
    // OCR 结果窗尺寸不跟着备份走（见 config_view 里的说明）：窗口大小是逐机器
    // 偏好，恢复备份时保留本机已经拉好的尺寸，不拿旧机器的大小覆盖。
    if let Some(x) = b("ocrEnable") { cfg.ocr_enable = x; }
    // 和 WebDAV 一样的道理：空值不覆盖，免得从旧备份恢复时把密钥抹掉
    if let Some(x) = s("ocrKey") { if !x.trim().is_empty() { cfg.ocr_key = x; } }
    if let Some(x) = s("ocrSecret") { if !x.is_empty() { cfg.ocr_secret = x; } }
    // 每家引擎的凭据（需求 2）。**空值不覆盖**，理由同上面两行：
    // 备份可能是在还没配那家引擎时导出的，照单全收会把已经填好的密钥抹掉。
    if let Some(o) = v.get("ocrCreds").and_then(|x| x.as_object()) {
        for (id, c) in o {
            if crate::ocr_engines::find(id).is_none() {
                continue; // 不认识的引擎（旧版本 / 手改过的备份）跳过
            }
            let g = |f: &str| {
                c.get(f)
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string()
            };
            let (k, sec, b, m) = (g("key"), g("secret"), g("base"), g("model"));
            for (map, val) in [
                (&mut cfg.ocr_keys, k),
                (&mut cfg.ocr_secrets, sec),
                (&mut cfg.ocr_bases, b),
                (&mut cfg.ocr_models, m),
            ] {
                if !val.is_empty() {
                    map.insert(id.clone(), val);
                }
            }
        }
    }
    if let Some(x) = b("webdavOnStart") { cfg.webdav_on_start = x; }
    if let Some(x) = b("webdavOnExit") { cfg.webdav_on_exit = x; }
    // v1.5.16：翻译跟着备份走（换了电脑不用重填 Key）。口径同 OCR 凭据：
    // 空值不覆盖 —— 备份可能是在还没配翻译时导出的，照单全收会抹掉已填的 Key。
    if let Some(x) = b("transEnable") { cfg.trans_enable = x; }
    // v2.23.80：翻译按钮在不在横条上也要跟着备份走 —— 否则换台电脑导入后
    // 「移出过」这件事丢了，下次启动又会被补回横条。
    if let Some(x) = b("transAddToBar") { cfg.trans_add_to_bar = x; }
    if let Some(x) = s("transEngine") {
        if crate::trans_engines::find(x.trim()).is_some() {
            cfg.trans_engine = x.trim().to_string();
        }
    }
    if let Some(a) = v.get("transMulti").and_then(|x| x.as_array()) {
        cfg.trans_multi = a
            .iter()
            .filter_map(|x| x.as_str())
            .filter(|id| crate::trans_engines::find(id).is_some())
            .map(|s| s.to_string())
            .collect();
    }
    if let Some(o) = v.get("transCreds").and_then(|x| x.as_object()) {
        for (id, c) in o {
            if crate::trans_engines::find(id).is_none() {
                continue; // 不认识的引擎（旧版本 / 手改过的备份）跳过
            }
            let g = |f: &str| {
                c.get(f)
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string()
            };
            for (map, val) in [
                (&mut cfg.trans_keys, g("key")),
                (&mut cfg.trans_secrets, g("secret")),
                (&mut cfg.trans_bases, g("base")),
                (&mut cfg.trans_models, g("model")),
            ] {
                if !val.is_empty() {
                    map.insert(id.clone(), val);
                }
            }
        }
    }
    if let Some(x) = s("transTarget") {
        if !x.trim().is_empty() {
            cfg.trans_target = crate::trans::normalize_lang(&x);
        }
    }
    // v1.2.0：自动更新开关（跟着备份/恢复走 —— 它是用户偏好，不是逐机器的东西）
    if let Some(x) = b("autoUpdate") { cfg.auto_update = x; }
    // GitHub 令牌跟着备份走（换了电脑不用重填）。空值不覆盖：备份里没有这个键
    // 是很常见的（旧版本导出的），照单全收会把本机已填的令牌清掉。
    if let Some(x) = s("githubToken") {
        if !x.trim().is_empty() {
            cfg.github_token = x.trim().to_string();
        }
    }
    // 注意：这三项**空值不覆盖**。备份是在填 WebDAV 之前导出的很常见，
    // 要是照单全收把地址/密码清空，之后所有同步都会失败，用户还不一定反应过来。
    if let Some(x) = s("webdavUrl") {
        if !x.trim().is_empty() {
            // 只存用户填的那一段（协议 + 尾斜杠），**不**把软件自己的子目录写进去。
            // 子目录由 crate::webdav::normalize_base 在真正发请求时自动补 —— 这样设置页
            // 的输入框里就只显示用户该填的根地址，不会让人以为还得手填一层目录。
            cfg.webdav_url = crate::webdav::normalize_input(&x);
        }
    }
    if let Some(x) = s("webdavUser") {
        if !x.trim().is_empty() { cfg.webdav_user = x; }
    }
    if let Some(x) = s("webdavPass") {
        if !x.is_empty() { cfg.webdav_pass = x; }
    }
    if let Some(x) = s("aiSuffix") { cfg.ai_suffix = x; }
    if let Some(x) = b("aiCopyPrompt") { cfg.ai_copy_prompt = x; }
    if let Some(m) = v.get("aiSuffixes").and_then(|x| x.as_object()) {
        for (k, val) in m {
            if let Some(t) = val.as_str() {
                cfg.ai_suffixes.insert(k.clone(), t.to_string());
            }
        }
    }
    if let Some(s) = v.get("iconStyle").and_then(|x| x.as_str()) {
        cfg.icon_style = if s == "letter" { "letter".to_string() } else { "auto".to_string() };
    }
    if let Some(m) = v.get("urlOverrides").and_then(|x| x.as_object()) {
        for (k, val) in m {
            if let Some(t) = val.as_str() {
                if t.contains("%s") {
                    cfg.url_overrides.insert(k.clone(), t.to_string());
                }
            }
        }
    }
    if let Some(a) = v.get("barList").and_then(|x| x.as_array()) {
        let list: Vec<String> = a
            .iter()
            .filter_map(|x| x.as_str())
            .map(|s| s.to_string())
            .collect();
        if !list.is_empty() {
            cfg.bar_list = list;
        }
    }
    if let Some(s) = v.get("sepStyle").and_then(|x| x.as_str()) {
        cfg.sep_style = s.to_string();
    }
    // 自定义引擎：备份里是同结构的 JSON 数组，直接反序列化。
    // 走 serde 而不是手抄字段，以后加字段不会漏。
    if let Some(a) = v.get("customEngines") {
        if let Ok(list) = serde_json::from_value::<Vec<crate::config::CustomEngine>>(a.clone()) {
            /* ⚠ 一份**缺字段**的备份不能把本机能用的定义打回空值。
             *
             * 用户实测事故（2026-10-22）：13:56 那次「云端备份较新，已恢复到本机」
             * 之后，两条快捷键引擎变成了 `{kind:"search", url:"", hotkey:""}` ——
             * 云端那份 View 备份里根本没有 kind/hotkey/app_path 这些字段
             * （老版本导出的），serde 按默认值一填，`kind` 就成了 search、
             * `hotkey` 成了空串。结果：横条上画不出来、引擎库里也没有，
             * 用户看到的就是「我的快捷键引擎被删了」。
             *
             * 本机那份明明是好的，凭什么被一份更缺信息的备份覆盖？
             * 和上面 backupPassword「空值不覆盖」是同一条口径。 */
            cfg.custom_engines = crate::merge_restored_custom_engines(&cfg.custom_engines, list);
        }
    }
    // 外部软件：同上。图标不在这个结构里（走 overrideIcons，键是 @app:<id>），
    // 所以这里只恢复「名字 + 地址 + id」三样。
    if let Some(a) = v.get("apps") {
        if let Ok(list) = serde_json::from_value::<Vec<crate::config::ExternalApp>>(a.clone()) {
            cfg.apps = list;
        }
    }
    // 收藏夹整树（含图片 dataURL）—— 备份/恢复因此天然覆盖收藏内容。
    if let Some(a) = v.get("favorites") {
        if let Ok(list) = serde_json::from_value::<Vec<crate::config::FavGroup>>(a.clone()) {
            cfg.favorites = list;
        }
    }
    if let Some(s) = v.get("favHotkey").and_then(|x| x.as_str()) {
        cfg.fav_hotkey = s.trim().to_string();
    }
    if let Some(s) = v.get("backupPassword").and_then(|x| x.as_str()) {
        /* ⚠ v1.4.1：**空值不覆盖**（和其它密钥一个口径）。
         *
         * 原来是无条件赋值。后果很重：用户恢复一份"当年还没设密码"的旧备份，
         * 密码就被清空 → `save_to` 里 `crate::stash_backup_password("")` 会把
         * `backup.key` 删掉 → **这台机器上所有用旧密码加密的备份当场全部解不开**。
         * 密码是"安全设置"，不该被一份更老的备份降级掉；真要清空，
         * 用户在设置里清一次就行。 */
        if !s.trim().is_empty() {
            cfg.backup_password = s.to_string();
        }
    }
    if let Some(s) = v.get("backupDir").and_then(|x| x.as_str()) {
        /* 空值不覆盖：一份"没设备份目录"的备份，不该把本机精心设置的
         * D 盘备份目录清空。真要恢复默认，用户在设置里手动清一次。 */
        if !s.trim().is_empty() {
            cfg.backup_dir = s.trim().to_string();
        }
    }
    // 上次用的搜索引擎。config_view 一直在 emit 它，以前 restore 侧没接 ——
    // 换电脑恢复后横条默认落回第一个引擎，用户「明明一切恢复了」的错觉就被
    // 这一个细节打破。空串 / null = 没记录过，不动本机的值。
    if let Some(x) = s("lastEngine") {
        cfg.last_engine = if x.is_empty() { None } else { Some(x) };
    }
    // r54 识图搜索跟着备份走。同样的道理：名单只收注册表里认识的 id
    // （备份可能来自装了更多引擎的版本，或者来自更早的版本）。
    if let Some(x) = b("imgEnable") { cfg.img_enable = x; }
    if let Some(x) = b("imgAddToBar") { cfg.img_add_to_bar = x; }
    if let Some(x) = b("imgKeepClip") { cfg.img_keep_clip = x; }
    if let Some(x) = s("imgDefaultEngine") {
        if x.is_empty() || crate::imgsearch::find(&x).is_some() {
            cfg.img_default_engine = x;
        }
    }
    if let Some(arr) = v.get("imgEngines").and_then(|x| x.as_array()) {
        let list: Vec<String> = arr
            .iter()
            .filter_map(|x| x.as_str())
            .map(|s| s.to_string())
            .collect();
        cfg.img_engines = crate::imgsearch::normalize_list(&list);
    }
    if let Some(x) = s("imgPublicUrl") {
        if x.starts_with("http://") || x.starts_with("https://") {
            cfg.img_public_url = x;
        }
    }
    /* r76：AI 识图的三个可调项。延迟要**钳范围** —— 备份可能来自别的版本 /
     * 手改过的文件，一个 4e9 的延迟能让横条线程睡几天。
     *   上限 60 秒：等待的意义只是"等浏览器把首屏画出来"，超过就是用户走开了。 */
    if let Some(x) = n("imgAiPasteDelay") {
        cfg.img_ai_paste_delay = (x as u32).min(60_000);
    }
    if let Some(x) = n("imgAiEnterDelay") {
        cfg.img_ai_enter_delay = (x as u32).min(60_000);
    }
    if let Some(x) = b("imgAiFirst") {
        cfg.img_ai_first = x;
    }
    if let Some(x) = b("imgAiAutoOffOnMulti") {
        cfg.img_ai_auto_off_on_multi = x;
    }
    // r56：最近使用名单 + 最多摆几个。恢复备份时整份灌回来，
    // 再走一遍 normalize_recent（备份可能来自另一台机器，那边的引擎
    // 这台不一定有 —— 不清的话面板会给出不存在的格子）。
    if let Some(m) = v.get("recentEngines") {
        if let Ok(list) = serde_json::from_value::<Vec<String>>(m.clone()) {
            cfg.recent_engines = list;
            crate::config::normalize_recent(cfg);
        }
    }
    if let Some(x) = n("recentMax") {
        cfg.recent_max = normalize_recent_max(x as usize);
    }
    if let Some(m) = v.get("favPanelSizes") {
        if let Ok(map) =
            serde_json::from_value::<std::collections::BTreeMap<String, Vec<f64>>>(m.clone())
        {
            cfg.fav_panel_sizes = map;
        }
    }
    // 自定义图标：备份里是 data URL，这里还原成 icons 目录下的文件
    if let Some(m) = v.get("overrideIcons").and_then(|x| x.as_object()) {
        for (name, url) in m {
            if let Some(u) = url.as_str() {
                if let Some(f) = crate::config::restore_icon(name, u) {
                    cfg.overrides.insert(name.clone(), f);
                }
            }
        }
    }
    // 备份里可能带着「指向已删除收藏目录」的横条哨兵（那份备份就是在这个目录
    // 还在的时候导出的）。灌回去之前先剪掉，否则恢复完横条上立刻多一格乱码。
    let pruned = crate::config::prune_dangling_fav_sentinels(cfg);
    if pruned > 0 {
        crate::win::log(&format!(
            "apply_view: 备份里有 {pruned} 个指向已删除收藏目录的横条哨兵，已剪掉"
        ));
    }
    // 同理：备份里的 @app:<id> 哨兵可能指向一个不在 apps 里的 id
    // （那份备份之后用户删过软件）。剪掉，别让恢复完横条上多一格乱码。
    let pruned_apps = crate::config::prune_dangling_app_sentinels(cfg);
    if pruned_apps > 0 {
        crate::win::log(&format!(
            "apply_view: 备份里有 {pruned_apps} 个指向已删除外部软件的横条哨兵，已剪掉"
        ));
    }
    /* v2.23.10：**界面语言也要跟着恢复**。
     *
     * `config_view` 一直在广播 `uiLang`，`apply_view` 却漏了这一项 ——
     * 于是「整份备份 / 挑选导出」恢复之后，别的设置都回来了，界面语言还是
     * 恢复前那个。用户看到的是"我把配置导回来了，但语言没回来"。
     * 这类"广播有、应用没有"的漏项很难自己发现，靠的是逐键比对。 */
    if let Some(l) = s("uiLang") {
        cfg.ui_lang = crate::config::normalize_ui_lang(&l);
    }
}
