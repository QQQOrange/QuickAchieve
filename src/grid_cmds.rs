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
