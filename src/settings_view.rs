//! 设置页配置视图（v2.23.111 从 main 拆出）。
//!
//! 把 `config::Config` 投影成前端 `CONF` 需要的 JSON：只读，不锁、不落盘。
//! `config_view()` / `defaults_view()` 两个命令仍留 main，共用这一份实现。

use crate::{config, ocr_engines, trans_engines};
use serde_json::{json, Map, Value};

pub(crate) fn trans_creds_view(cfg: &config::Config) -> Value {
    let mut m = Map::new();
    for e in trans_engines::ENGINES {
        let c = config::trans_creds(cfg, e.id);
        m.insert(
            e.id.to_string(),
            json!({ "key": c.key, "secret": c.secret, "base": c.base, "model": c.model }),
        );
    }
    Value::Object(m)
}


pub(crate) fn ocr_creds_view(cfg: &config::Config) -> Value {
    let mut m = Map::new();
    for e in ocr_engines::ENGINES {
        let c = crate::ocr_creds(cfg, e.id);
        m.insert(
            e.id.to_string(),
            json!({ "key": c.key, "secret": c.secret, "base": c.base, "model": c.model }),
        );
    }
    Value::Object(m)
}


pub(crate) fn config_view_of(cfg: &config::Config) -> Value {
    let mut ov = Map::new();
    for (name, file) in cfg.overrides.iter() {
        if let Some(url) = crate::icon_data_url_cached(&config::icons_dir().join(file), file) {
            ov.insert(name.clone(), Value::String(url));
        }
    }
    // #138：icons 目录里「文件名 = 引擎名」的默认图标（用户原话：读取
    // AppData\Roaming\QuickAchieve\icons 里的自定义图标，设为对应引擎的默认图标）。
    // 优先级：overrideIcons（图库显式挑的）> 这里的 dirIcons > 引擎自带
    // iconFile > 字母徽标。已被 overrides 引用的文件跳过（它已经在 ov 里，
    // 再进 dirIcons 只会让同一份图在两条通道里各出现一次）。
    let mut di: Map<String, Value> = Map::new();
    let claimed: std::collections::HashSet<String> = cfg.overrides.values().cloned().collect();
    let mut dir_bytes: usize = 0;
    for (stem, path) in config::dir_icon_defaults() {
        if di.len() >= 256 || dir_bytes >= 6 * 1024 * 1024 {
            break; // 防呆：icons 目录被谁当下载目录使，也不能把配置广播撑爆
        }
        let Some(fname) = path.file_name().and_then(|s| s.to_str()).map(|s| s.to_string()) else {
            continue;
        };
        if claimed.contains(&fname) {
            continue;
        }
        if di.contains_key(&stem) {
            continue; // 同名多扩展名只取排序后的第一个，结果稳定
        }
        if let Some(url) = crate::icon_data_url_cached(&path, &fname) {
            dir_bytes += url.len();
            di.insert(stem, Value::String(url));
        }
    }
    let uo: Map<String, Value> = cfg
        .url_overrides
        .iter()
        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
        .collect();
    let ais: Map<String, Value> = cfg
        .ai_suffixes
        .iter()
        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
        .collect();
    json!({
        "version": config::VERSION,
        // 开发者 / 这一版的构建时间：「关于」页要显示（用户明确要求）。
        // 构建时间由 build.rs 在编译期算好注入（config::BUILT_AT 的注释里
        // 说明了为什么不用 exe 的 mtime）。
        "developer": config::DEVELOPER,
        "builtAt": config::BUILT_AT,
        // 调试构建标记：QS_DEBUG=1 时前端会打开自动化用的坐标上报等辅助逻辑。
        // 正式启动不会带上，前端那条路也就永不执行。
        "debug": std::env::var("QS_DEBUG").map(|v| v == "1").unwrap_or(false),
        // 内建自检开关：QS_SELFTEST=1 时设置页会自己把真实控件点一遍并上报结果，
        // 用于开发期真机验证。正式启动不带。
        "selftest": std::env::var("QS_SELFTEST").map(|v| v == "1").unwrap_or(false),
        "triggerSelect": cfg.trigger_select,
        "triggerCopy": cfg.trigger_copy,
        "notifyFeature": cfg.notify_feature,
        // 右下角提示显示几秒（v2.6.8：内置提示，时长自己做主）
        "notifySeconds": cfg.notify_seconds,
        // 网格页（Ctrl+C）：快捷键、行列、单元格尺寸、内容项
        "gridHotkey": cfg.grid_hotkey,
        // v2.23.6：热键撞车时只激发本软件（吞掉按键，不传给别的程序）
        "hotkeyExclusive": cfg.hotkey_exclusive,
        "gridRows": cfg.grid_rows.clamp(1, 10),
        "gridCols": cfg.grid_cols.clamp(1, 10),
        "gridCellW": cfg.grid_cell_w.clamp(40, 260),
        "gridCellH": cfg.grid_cell_h.clamp(40, 200),
        "gridItems": cfg.grid_list,
        // 横条的按应用专项（设置页「按应用绑定」用它列出/编辑）
        "barAppItems": cfg.bar_app_items.iter().map(|b| json!({
            "app": b.app, "items": b.items,
        })).collect::<Vec<_>>(),
        // 多页：设置页要用（页列表 / 按应用切换 / 内容列表跟随当前页）
        "gridPages": cfg.grid_pages.iter().map(|p| json!({
            "id": p.id, "name": p.name, "items": p.items,
        })).collect::<Vec<_>>(),
        "gridDefaultPage": cfg.grid_default_page,
        "gridAppPages": cfg.grid_app_pages.iter().map(|b| json!({
            "app": b.app, "page": b.page,
        })).collect::<Vec<_>>(),
        "gridGap": cfg.grid_gap.clamp(0, 24),
        "gridRadius": cfg.grid_radius.clamp(0, 24),
        "gridShowLabel": cfg.grid_show_label,
        // v2.7.0：单项显示样式覆盖（icon / text / both），没写的项跟着上面那个走
        "gridItemStyle": cfg.grid_item_style,
        "gridEnable": cfg.grid_enable,
        "gridClick": cfg.grid_click,
        "gridAllowAdd": cfg.grid_allow_add,
        "autoStart": cfg.auto_start,
        /* v2.23.117：开机自启的**真实状态**（注册表指向谁 / 有没有计划任务 /
         * 有没有在任务管理器里被禁用）。只读缓存，不现查 —— 配置广播是热路径，
         * 不能每次广播都去跑一遍 `schtasks`。现查走命令 `autostart_state`。 */
        "autoStartState": crate::autostart::ui_state(),
        /* v2.23.24：发**真实生效**的提权状态，别再让开关骗人。
     * 便携测试版恒定 false（它不主动提权），用户明确拨过的按标记文件走。 */
    "autoElevate": crate::read_local_elevate_pref().unwrap_or(!crate::QS_LITE_PORTABLE && cfg.auto_elevate),
        // 智能直达：整条列表原样给前端（含 id / kind / name / note / icon / target）。
        // 横条靠 `@go:<id>` 哨兵来认它，所以 id 必须一起出去。
        "shortcuts": cfg.shortcuts.iter().map(|x| json!({
            "id": x.id,
            "kind": x.kind,
            "target": x.target,
            "name": x.name,
            "note": x.note,
            "icon": x.icon,
            // 默认图标类型（url / app / folder / image / video / audio / text /
            // pdf / archive / code / doc / file）。用户没自定义图标时前端按它挑图，
            // 见 go_icon_kind 的注释：类型跟着目标实时算，不进配置。
            "iconKind": crate::go_icon_kind(&x.kind, &x.target),
        })).collect::<Vec<_>>(),
        // r80：AI 抠图总开关。**模型选择不进这里** —— 它要走 ai_matting_status
        //      命令才知道"文件在不在、运行时能不能加载"，纯配置项表达不了。
        "aiMattingEnable": cfg.ai_matting_enable,
        // r80g：模型目录（空 = 默认位置）。设置页据此显示"已自定义/默认"
        "aiModelDir": cfg.ai_model_dir,
        "iconSize": cfg.icon_size,
        "alpha": cfg.alpha,
        // 待机（鼠标没悬停）时的横条不透明度。设成 100 就等于关掉这套行为。
        "barDimAlpha": cfg.bar_dim_alpha,
        // 渐隐延迟（毫秒）：弹出后先不透明，过了它才淡到 barDimAlpha。
        "barDimDelayMs": cfg.bar_dim_delay_ms,
        // 液态玻璃一组。前端按这些参数驱动 WebGL2 渲染器（liquidGL 移植版数学）。
        "barGlass": cfg.bar_glass,
        "barGlassRefraction": cfg.bar_glass_refraction,
        "barGlassBevel": cfg.bar_glass_bevel,
        "barGlassFrost": cfg.bar_glass_frost,
        "barGlassAberration": cfg.bar_glass_aberration,
        "barGlassSpec": cfg.bar_glass_spec,
        "barGlassTint": cfg.bar_glass_tint,
        "barGlassLive": cfg.bar_glass_live,
        // v1.3.0：实时刷新帧率上限（0 = 跟随显示器）
        "barGlassFps": cfg.bar_glass_fps,
        "iconStyle": cfg.icon_style,
        "unifyIcons": cfg.unify_icons,
        "barPadding": cfg.bar_padding,
        "itemRadius": cfg.item_radius,
        "theme": cfg.theme,
        "animations": cfg.animations,
        "shadow": cfg.shadow,
        "barBlur": cfg.bar_blur,
        // v2.23.83：磨砂样式下横条底板（非图标部分）的不透明度。
        "barBgAlpha": cfg.bar_bg_alpha,
        "anchorAtMouse": cfg.anchor_at_mouse,
        // v1.5.4：横条弹出位置（"above" = 文字上方 / "below" = 下方）。
        // 贴鼠标弹（anchorAtMouse / Ctrl+C 触发）时本项无效。
        "barPos": cfg.bar_pos,
        "delayMs": cfg.delay_ms,
        // 2026-10-23：横条自己收起来的两条阈值（见 config.rs 对应字段的注释）。
        "barIdleSec": cfg.bar_idle_sec,
        "barAwayPx": cfg.bar_away_px,
        // 没取到文字就不弹（默认开）—— 误触乱弹的主要来源。
        "barNeedText": cfg.bar_need_text,
        // v2.16.0：框选之后先停一下（可拖角微调 + 换用途）。关掉 = 松手立即执行。
        "regionAdjust": cfg.region_adjust,
        // v2.18.1：上次在动作条上点的动作 —— 页面拿它当"双击执行"的默认项
        "regionLastAction": cfg.region_last_action,
        // v2.14.0：横条触发方式（"always" = 选中即弹 / "ctrl" = 按住 Ctrl 才弹）。
        // 两者互斥，值只有这两个。
        "barTriggerMode": cfg.bar_trigger_mode,
        "filterMode": cfg.filter_mode,
        "filterList": cfg.filter_list,
        "ocrEnable": cfg.ocr_enable,
        "ocrKey": cfg.ocr_key,
        "ocrSecret": cfg.ocr_secret,
        "ocrLang": cfg.ocr_lang,
        "ocrAccurate": cfg.ocr_accurate,
        "ocrEngine": cfg.ocr_engine,
        // 每家引擎的凭据（需求 2）：设置页据此渲染"当前引擎要填哪几样 + 已填了什么"。
        // 百度那两家老字段（ocrKey/ocrSecret）已并进来，界面不会显示成"没填"。
        "ocrCreds": ocr_creds_view(cfg),
        // r47：「同时识别」名单 —— 默认引擎之外的、要一起跑的那几家。
        // 结果窗的版式（单框 / 多框 + 底部档位）由它 + ocrEngine 派生的
        // **参与名单**决定，所以这里必须发（老代码发的是 ocrParallel）。
        "ocrMulti": cfg.ocr_multi,
        // r49：结果窗是否**固定在默认引擎**那一档（默认开）。前端 ocr.html 的
        // pickDefaultMode 读它决定"谁先出字就先显示谁"还是"一直等默认引擎"。
        "ocrStayDefault": cfg.ocr_stay_default,
        // ⚠ ocrParallel 只留作**向下兼容**（老版本读它）：r47 起它是废弃字段，
        //   sanitize 里会在迁移后把它落回 false。前端已不再使用它。
        "ocrParallel": cfg.ocr_parallel,
        // OCR 文本整理的默认开关（需求 7）。跟着备份走：这是用户偏好，不是
        // 逐机器的东西（对比下面 ocrWinW/H 那段说明）。
        "ocrCleanGarbage": cfg.ocr_clean_garbage,
        "ocrCleanSpace": cfg.ocr_clean_space,
        "ocrCleanWidth": cfg.ocr_clean_width,
        "ocrCleanUnwrap": cfg.ocr_clean_unwrap,
        // ⚠ ocrWinW / ocrWinH **故意不进备份**：OCR 结果窗尺寸是逐机器偏好
        // （4K 台式机上拉好的 1400×1000，同步到小笔记本会开出比屏幕还大的窗）。
        // 用户对备份的要求是「数据」—— 设置、引擎、图标、收藏夹、密钥、
        // WebDAV、排序，这些都在；窗口大小属于「别的」，只在本机 config.json
        // 里持久化。哪天真要跟着备份走，先给 ocr_window_size 加屏幕边界钳制。
        "barList": cfg.bar_list,
        // v2.23.44：这次运行里配置写盘失败的原因（空 = 正常）。
        // 设置页在顶部显示一条红提示 —— 以前只有托盘气泡，错过就不知道了。
        "saveError": config::last_save_error(),
        "favorites": cfg.favorites,
        "favHotkey": cfg.fav_hotkey,
        "backupPassword": cfg.backup_password,
        // 备份目录。以前只写不读（apply_one_setting 收它，config_view 却不发），
        // 于是「导出/云端恢复」之后用户自定义的备份目录**不会跟着回来**，
        // 设置页那一行会悄悄变回「默认」。目录在新机器上不可用也没关系：
        // config::backup_dir_of 会建不出来就退回数据目录（见那里的注释）。
        "backupDir": cfg.backup_dir,
        "favPanelSizes": cfg.fav_panel_sizes,
        // v2.6.9：网格页「添加到这一格」选板的长宽（[宽, 高]，空 = 用默认 560×470）
        "pickSize": cfg.pick_size,
        "sepStyle": cfg.sep_style,
        "customEngines": cfg.custom_engines,
        "apps": cfg.apps,
        "overrideIcons": Value::Object(ov),
        // #138：icons 目录里按引擎名兜底的默认图标（前端插在 overrideIcons
        // 与 iconFile 之间查）
        "dirIcons": Value::Object(di),
        "urlOverrides": Value::Object(uo),
        "aiSuffix": cfg.ai_suffix,
        "aiSuffixes": Value::Object(ais),
        "aiCopyPrompt": cfg.ai_copy_prompt,
        "webdavUrl": cfg.webdav_url,
        "webdavUser": cfg.webdav_user,
        "webdavPass": cfg.webdav_pass,
        "webdavOnStart": cfg.webdav_on_start,
        "webdavOnExit": cfg.webdav_on_exit,
        // v1.2.0：自动更新开关。设置页「关于」里那个勾选框就是它。
        "autoUpdate": cfg.auto_update,
        // v1.5.7：GitHub 个人访问令牌。和 webdavPass 一样是"要回显给本人改"的
        // 凭据 —— 页面是本机文件，config.json 整体加密；但它**不进日志**。
        "githubToken": cfg.github_token,
        // v1.5.16：翻译（多引擎，对齐 OCR 那套）。Key 和 githubToken /
        // webdavPass 一样"回显给本人改"—— 页面是本机文件、config.json 整体加密，
        // 但它**不进日志**。
        "transEnable": cfg.trans_enable,
        // v2.23.80：翻译按钮在不在横条上（用户移出过 = false）。
        // 前端不需要它也能工作（成员资格看 barList），发出来是为了让
        // 「加进横条」那颗按钮的判断和导出/导入口径一致。
        "transAddToBar": cfg.trans_add_to_bar,
        "transEngine": cfg.trans_engine,
        "transMulti": cfg.trans_multi,
        "transCreds": trans_creds_view(cfg),
        "transTarget": cfg.trans_target,
        "lastEngine": cfg.last_engine,
        // r54 识图搜索。跟设置走（是用户偏好，不是逐机器的东西）——
        // 换电脑恢复之后识图引擎的勾选要还在。
        "imgEnable": cfg.img_enable,
        "imgAddToBar": cfg.img_add_to_bar,
        "imgKeepClip": cfg.img_keep_clip,
        "imgDefaultEngine": cfg.img_default_engine,
        // ⚠ 发出之前先归一一遍：老配置里可能存着已下线的引擎 id，
        //   界面拿到直接渲染会出现"选了个不存在的引擎"。
        "imgEngines": crate::imgsearch::normalize_list(&cfg.img_engines),
        "imgPublicUrl": cfg.img_public_url,
        // r76：AI 识图的按键时序 + AI 优先 + 多家自动关按键
        "imgAiPasteDelay": cfg.img_ai_paste_delay,
        "imgAiEnterDelay": cfg.img_ai_enter_delay,
        "imgAiFirst": cfg.img_ai_first,
        "imgAiAutoOffOnMulti": cfg.img_ai_auto_off_on_multi,
        // r56：「最近使用」那一栏。跟着设置走（换电脑恢复之后"最近用过什么"
        // 也还在，用户不会觉得"换了台机器就不认识我了"）。
        // 名单可能很长（每点一次记一笔），展示时按 recentMax 截断在 UI 侧做；
        // 这里发全量，上限由用户选的那档决定。
        "recentEngines": json!(cfg.recent_engines),
        "recentMax": cfg.recent_max,
        // v1.5.13：界面语言（"zh" / "en"）。所有页面都靠它决定要不要走英文，
        // 所以必须随每次广播下发 —— 设置页一改，横条/OCR 窗立刻跟着换。
        "uiLang": config::normalize_ui_lang(&cfg.ui_lang),
        // v1.5.13：启动密码**只报"有没有设"**，绝不把哈希发给前端。
        // 前端据此决定"清除密码"那颗按钮要不要亮。
        "startupPasswordSet": !cfg.startup_password_hash.is_empty(),
    })
}

