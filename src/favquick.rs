//! 收藏夹「一步收」的纯逻辑（v2.23.46：从 `main.rs` 拆出的第二块）。
//!
//! 不碰 STATE、不广播、不写盘 —— 所以能直接单测；命令层只负责加锁、存盘、广播。

use crate::config;
/// 生成一个**本进程内唯一**的 id。
///
/// 为什么不能只用毫秒时间戳：连点两次（或脚本连续调用）很容易落在同一毫秒里，
/// 两条收藏就会拿到同一个 id —— 之后按 id 找条目（`find_fav_item` 取第一条）
/// 会出现"点第二条却动了第一条"。加一个自增序号兜住这种情况。
fn next_id(prefix: &str, now_ms: i64) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}{now_ms}_{n}")
}

pub(crate) fn quick_add_into(groups: &mut Vec<config::FavGroup>, text: &str, now_ms: i64) -> Result<String, String> {
    let body = text.trim();
    if body.is_empty() {
        return Err("没有可收藏的文字".into());
    }
    if body.chars().count() > 20_000 {
        return Err("内容太长（超过 2 万字），没有收藏".into());
    }
    const GROUP: &str = "快速收藏";
    let gi = match groups.iter().position(|g| g.name == GROUP) {
        Some(i) => i,
        None => {
            groups.push(config::FavGroup {
                id: next_id("g", now_ms),
                name: GROUP.to_string(),
                icon: String::new(),
                items: Vec::new(),
                children: Vec::new(),
            });
            groups.len() - 1
        }
    };
    if groups[gi].items.iter().any(|it| it.kind == "text" && it.text == body) {
        return Ok(format!("已经在「{GROUP}」里了"));
    }
    groups[gi].items.push(config::FavItem {
        id: next_id("q", now_ms),
        kind: "text".to_string(),
        text: body.to_string(),
        html: String::new(),
        image: String::new(),
        remark: String::new(),
    });
    Ok(format!("已收藏到「{GROUP}」"))
}

#[cfg(test)]
mod fav_quick_tests {
    use super::*;

    #[test]
    fn 一步收藏会建分组并去重() {
        let mut g: Vec<config::FavGroup> = Vec::new();
        let m1 = quick_add_into(&mut g, "  第一段  ", 1000).unwrap();
        assert!(m1.contains("快速收藏"), "{m1}");
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].items.len(), 1);
        assert_eq!(g[0].items[0].text, "第一段", "两头空白要去掉");

        // 同一条再收藏一次：不新增，给出"已在"提示
        let m2 = quick_add_into(&mut g, "第一段", 2000).unwrap();
        assert!(m2.contains("已经"), "{m2}");
        assert_eq!(g[0].items.len(), 1, "不许重复添加");

        // 换一条内容：进同一个分组
        quick_add_into(&mut g, "第二段", 3000).unwrap();
        assert_eq!(g.len(), 1, "不该再建第二个分组");
        assert_eq!(g[0].items.len(), 2);
    }

    /// 同一毫秒连点两次：两条收藏的 id 必须不同（否则按 id 操作会串到第一条上）。
    #[test]
    fn 同一毫秒连续收藏不会出现重复id() {
        let mut g: Vec<config::FavGroup> = Vec::new();
        quick_add_into(&mut g, "第一条", 1234).unwrap();
        quick_add_into(&mut g, "第二条", 1234).unwrap();
        let ids: Vec<&str> = g[0].items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1], "同一毫秒的两条收藏不许同 id");
    }
    #[test]
    fn 空内容与超长内容都要拒绝() {
        let mut g: Vec<config::FavGroup> = Vec::new();
        assert!(quick_add_into(&mut g, "   ", 1).is_err());
        let long = "字".repeat(20_001);
        assert!(quick_add_into(&mut g, &long, 2).is_err());
        assert!(g.is_empty(), "被拒绝时不能留下空分组");
    }

    #[test]
    fn 已有同名分组时直接复用() {
        let mut g = vec![config::FavGroup {
            id: "g0".into(),
            name: "快速收藏".into(),
            icon: String::new(),
            items: Vec::new(),
            children: Vec::new(),
        }];
        quick_add_into(&mut g, "x", 10).unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].items.len(), 1);
    }
}
