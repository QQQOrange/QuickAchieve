//! 网格页的纯配置操作（v2.23.109 从 main 拆出）。
//!
//! 这里只改 `config::Config`：不碰全局状态、不落盘、不广播、不写日志。
//! 落盘 / 广播 / 日志由 main 的命令壳负责，命令签名与前端完全不变。

use crate::config;

/// 新建一页，返回 `(id, name)`。名字空则用「网格页 N」。
pub(crate) fn page_add(cfg: &mut config::Config, name: String) -> Result<(String, String), String> {
    if cfg.grid_pages.len() >= 20 {
        return Err("最多 20 个网格页".into());
    }
    // 生成一个没被占用的 id：p1 / p2 / …
    let mut n = 1usize;
    let id = loop {
        let cand = format!("p{n}");
        if config::grid_page_index(cfg, &cand).is_none() {
            break cand;
        }
        n += 1;
    };
    let name = name.trim().to_string();
    let name = if name.is_empty() {
        format!("网格页 {}", cfg.grid_pages.len() + 1)
    } else {
        name
    };
    cfg.grid_pages.push(config::GridPage {
        id: id.clone(),
        name: name.clone(),
        items: Vec::new(),
    });
    Ok((id, name))
}

/// 改页名（id 不变 —— 应用绑定靠 id，改名不会把绑定弄丢）。
pub(crate) fn page_rename(
    cfg: &mut config::Config,
    id: String,
    name: String,
) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("页名不能为空".into());
    }
    let Some(i) = config::grid_page_index(cfg, &id) else {
        return Err("找不到这个网格页".into());
    };
    cfg.grid_pages[i].name = name;
    Ok(())
}

/// 删一页。**至少留一页**；指向它的应用绑定一并剪掉（不留悬空绑定）。
/// 返回 `(被删页的名字, 顺带剪掉的应用绑定数)`。
pub(crate) fn page_delete(cfg: &mut config::Config, id: String) -> Result<(String, usize), String> {
    if cfg.grid_pages.len() <= 1 {
        return Err("至少要保留一个网格页".into());
    }
    let Some(i) = config::grid_page_index(cfg, &id) else {
        return Err("找不到这个网格页".into());
    };
    let removed = cfg.grid_pages.remove(i);
    let before_bind = cfg.grid_app_pages.len();
    cfg.grid_app_pages.retain(|b| b.page != id);
    if cfg.grid_default_page == id {
        cfg.grid_default_page = cfg
            .grid_pages
            .first()
            .map(|p| p.id.clone())
            .unwrap_or_default();
    }
    cfg.grid_list = config::grid_page_index(cfg, &cfg.grid_default_page)
        .and_then(|i| cfg.grid_pages.get(i))
        .map(|p| p.items.clone())
        .unwrap_or_default();
    Ok((removed.name, before_bind - cfg.grid_app_pages.len()))
}

/// 把某页设为默认页（没有应用绑定时用它）。
pub(crate) fn page_set_default(cfg: &mut config::Config, id: String) -> Result<(), String> {
    if config::grid_page_index(cfg, &id).is_none() {
        return Err("找不到这个网格页".into());
    }
    cfg.grid_default_page = id.clone();
    cfg.grid_list = config::grid_page_index(cfg, &id)
        .and_then(|i| cfg.grid_pages.get(i))
        .map(|p| p.items.clone())
        .unwrap_or_default();
    Ok(())
}

/// 「这个应用默认用哪一页」。同名应用再调一次就是**改绑定**（不会重复）。
pub(crate) fn app_page_set(
    cfg: &mut config::Config,
    app: String,
    page: String,
) -> Result<(), String> {
    let app = app.trim().to_string();
    if app.is_empty() {
        return Err("应用名不能为空（填进程名，如 chrome.exe；可用 * 通配）".into());
    }
    if config::grid_page_index(cfg, &page).is_none() {
        return Err("目标网格页不存在".into());
    }
    if let Some(b) = cfg
        .grid_app_pages
        .iter_mut()
        .find(|b| b.app.eq_ignore_ascii_case(&app))
    {
        b.app = app;
        b.page = page;
    } else {
        if cfg.grid_app_pages.len() >= 60 {
            return Err("最多 60 条应用绑定".into());
        }
        cfg.grid_app_pages.push(config::GridAppPage { app, page });
    }
    Ok(())
}

/// 解除某个应用的绑定。传空串 = 清空全部绑定。
pub(crate) fn app_page_remove(cfg: &mut config::Config, app: String) -> Result<(), String> {
    let app = app.trim().to_string();
    if app.is_empty() {
        cfg.grid_app_pages.clear();
    } else {
        cfg.grid_app_pages
            .retain(|b| !b.app.eq_ignore_ascii_case(&app));
    }
    Ok(())
}



/// 给设置页的网格配置快照（纯读，不锁、不落盘）。
pub(crate) fn config_view(c: &config::Config) -> serde_json::Value {
    serde_json::json!({
        "hotkey": c.grid_hotkey,
        "rows": c.grid_rows.clamp(1, 10),
        "cols": c.grid_cols.clamp(1, 10),
        "cellW": c.grid_cell_w.clamp(40, 260),
        "cellH": c.grid_cell_h.clamp(40, 200),
        "gap": c.grid_gap.clamp(0, 24),
        "radius": c.grid_radius.clamp(0, 24),
        "enable": c.grid_enable,
        "click": c.grid_click,
        "allowAdd": c.grid_allow_add,
        "items": c.grid_list,
        "usingBarList": c.grid_list.is_empty(),
        "barItems": c.bar_list,
        "pages": c.grid_pages.iter().map(|p| serde_json::json!({
            "id": p.id, "name": p.name, "items": p.items,
        })).collect::<Vec<_>>(),
        "defaultPage": c.grid_default_page,
        "appPages": c.grid_app_pages.iter().map(|b| serde_json::json!({
            "app": b.app, "page": b.page,
        })).collect::<Vec<_>>(),
    })
}

/// 行列 / 单元格尺寸。返回最终生效的 `(r,c,w,h,g,rad)`，日志由 main 打。
pub(crate) fn set_size(
    cfg: &mut config::Config,
    rows: u32,
    cols: u32,
    cell_w: u32,
    cell_h: u32,
    gap: Option<u32>,
    radius: Option<u32>,
) -> (u32, u32, u32, u32, u32, u32) {
    let (r, c) = (rows.clamp(1, 10), cols.clamp(1, 10));
    let (w, h) = (cell_w.clamp(40, 260), cell_h.clamp(40, 200));
    cfg.grid_rows = r;
    cfg.grid_cols = c;
    cfg.grid_cell_w = w;
    cfg.grid_cell_h = h;
    if let Some(g) = gap {
        cfg.grid_gap = g.clamp(0, 24);
    }
    if let Some(rad) = radius {
        cfg.grid_radius = rad.clamp(0, 24);
    }
    (r, c, w, h, cfg.grid_gap, cfg.grid_radius)
}

/// 功能开关：启用 / 点格子行为 / 空位可加。返回归一化后的 click。
pub(crate) fn set_behavior(
    cfg: &mut config::Config,
    enable: bool,
    click: String,
    allow_add: bool,
) -> Result<&'static str, String> {
    let mode = match click.trim().to_ascii_lowercase().as_str() {
        "home" => "home",
        "auto" => "auto",
        other => return Err(format!("未知的点格子行为：{other}")),
    };
    cfg.grid_enable = enable;
    cfg.grid_click = mode.to_string();
    cfg.grid_allow_add = allow_add;
    Ok(mode)
}

/// 网格内容项（形状净化：去空、去重、上限 60），写到指定页。
/// 返回实际写入的页名。
pub(crate) fn set_items(
    cfg: &mut config::Config,
    items: Vec<String>,
    page: Option<String>,
) -> Result<String, String> {
    let mut out: Vec<String> = Vec::new();
    for it in items {
        let name = it.trim().to_string();
        if name.is_empty() || out.contains(&name) {
            continue;
        }
        if out.len() >= 60 {
            break;
        }
        out.push(name);
    }
    let target = page.unwrap_or_default();
    let idx = if target.trim().is_empty() {
        config::grid_page_index(cfg, &cfg.grid_default_page).unwrap_or(0)
    } else {
        match config::grid_page_index(cfg, &target) {
            Some(i) => i,
            None => return Err(format!("找不到这个网格页：{target}")),
        }
    };
    if let Some(p) = cfg.grid_pages.get_mut(idx) {
        p.items = out;
    }
    cfg.grid_list = config::grid_page_index(cfg, &cfg.grid_default_page)
        .and_then(|i| cfg.grid_pages.get(i))
        .map(|p| p.items.clone())
        .unwrap_or_default();
    Ok(cfg.grid_pages.get(idx).map(|p| p.name.clone()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_add_rename_delete_keeps_default_valid() {
        let mut cfg = config::Config::default();
        let (id, name) = page_add(&mut cfg, "  工作  ".into()).unwrap();
        assert_eq!(name, "工作");
        assert_eq!(id, "p1");
        assert!(cfg.grid_pages.iter().any(|p| p.id == id));
        page_rename(&mut cfg, id.clone(), "工作 2".into()).unwrap();
        assert_eq!(cfg.grid_pages[0].name, "工作 2");
        // 新建第二页并设默认，再删掉它 → 默认页回到剩下那页
        let (id2, _) = page_add(&mut cfg, "临时".into()).unwrap();
        page_set_default(&mut cfg, id2.clone()).unwrap();
        assert_eq!(cfg.grid_default_page, id2);
        page_delete(&mut cfg, id2).unwrap();
        assert_eq!(cfg.grid_default_page, "p1");
    }

    #[test]
    fn app_page_set_is_case_insensitive_and_upserts() {
        let mut cfg = config::Config::default();
        let (id, _) = page_add(&mut cfg, "A".into()).unwrap();
        app_page_set(&mut cfg, "Chrome.exe".into(), id.clone()).unwrap();
        app_page_set(&mut cfg, "chrome.EXE".into(), id.clone()).unwrap();
        assert_eq!(cfg.grid_app_pages.len(), 1);
        assert_eq!(cfg.grid_app_pages[0].app, "chrome.EXE");
        app_page_remove(&mut cfg, "CHROME.exe".into()).unwrap();
        assert!(cfg.grid_app_pages.is_empty());
    }
}
