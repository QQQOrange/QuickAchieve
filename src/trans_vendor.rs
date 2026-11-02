//! 老牌翻译厂商的**专用协议**（百度翻译 / 谷歌翻译 / 有道智云）。
//!
//! 为什么单独一个模块：这三家的接口跟 OpenAI 兼容那套**毫不相干** ——
//!   · **百度翻译**（通用翻译 API）：表单 POST，凭据是「APP ID + 密钥」，
//!     请求要带 `sign = md5(appid + q + salt + secret)`，返回在
//!     `trans_result[].dst`；
//!   · **谷歌翻译**（Cloud Translation v2）：表单 POST，凭据是一个 API Key
//!     （放在查询串的 `key=` 上），返回在 `data.translations[].translatedText`；
//!   · **有道智云**（文本翻译）：表单 POST，凭据是「应用 ID + 应用密钥」，
//!     请求要带 `sign = sha256(appKey + input + salt + curtime + secret)`，
//!     其中 `input` 是 q 的「前 10 + 长度 + 后 10」截断（≤20 字时就是 q 本身），
//!     返回在 `translation[]`（**字符串数组**，不是对象数组）。
//!
//! 所以它们**共用不上** `crate::trans` 那份 OpenAI 兼容实现，各写一份。
//! 但**公共部分**（长度上限 / 语言码归一 / 错误分类 / 端点可注入）在这里
//! 统一处理，单测用本地仿真服务把"拼装 → 签名 → 解析"整条验一遍
//! （不联网、不花额度、不需要账号 —— 和 `crate::ocr_engines` 同一套办法）。
//!
//! ⚠ 语言码**每家不一样**，所以要按家映射（见下面三个 `*_lang` 函数）：
//!   我们的语言码（`trans::LANGS`）  百度    谷歌        有道
//!   zh                            zh      zh-CN      zh-CHS
//!   zh-TW                         cht     zh-TW      zh-CHT
//!   ja                            jp      ja         ja
//!   fr                            fra     fr         fr
//!   es                            spa     es         es
//!   ar                            ara     ar         ar
//!
//! 源语言一律用各家的「自动检测」：百度/有道是 `auto`，谷歌是 `source=auto`。

use serde_json::Value;

use crate::trans_engines::{Creds, EngineInfo, Kind};

/// 单次最多翻多少字（和 OpenAI 兼容那套一致的口径）。
pub const MAX_CHARS: usize = 20000;

/// 按引擎类型分发到各家实现。
///
/// 这是**翻译的唯一入口** —— `trans_run`（翻译窗）、`trans_test`（设置页
/// 「测试连接」）都走它，OpenAI 兼容那 8 家转给 [`crate::trans::translate`]。
pub fn translate(
    info: &EngineInfo,
    creds: &Creds,
    text: &str,
    target: &str,
) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("没有要翻译的文字".to_string());
    }
    if text.chars().count() > MAX_CHARS {
        return Err(format!(
            "这段文字太长（{} 字），一次最多翻 {MAX_CHARS} 字",
            text.chars().count()
        ));
    }
    match info.kind {
        Kind::OpenAi => crate::trans::translate(
            info.name,
            &creds.key,
            &creds.model_for(info),
            &creds.base_for(info),
            text,
            target,
        ),
        Kind::Baidu => baidu(info, creds, text, target),
        Kind::Google => google(info, creds, text, target),
        Kind::Youdao => youdao(info, creds, text, target),
    }
}

/// 流式版本：OpenAI 兼容那几家边生成边吐字（见 [`crate::trans::translate_stream`]），
/// 老牌三家一问一答、几十毫秒就回来，直接走 [`translate`]、不调 `on_delta`。
///
/// 为什么不做成"三家也假装流一下"：它们的响应本来就是一整段 JSON，硬拆成
/// 单字往外喂只会多一层假动作，一点时间都省不下来。
pub fn translate_stream(
    info: &EngineInfo,
    creds: &Creds,
    text: &str,
    target: &str,
    on_delta: &mut dyn FnMut(&str),
) -> Result<String, String> {
    if info.kind != Kind::OpenAi {
        return translate(info, creds, text, target);
    }
    crate::trans::translate_stream(
        info.name,
        &creds.key,
        &creds.model_for(info),
        &creds.base_for(info),
        text,
        target,
        on_delta,
    )
}

/* ------------------------------------------------------------------ */
/* 百度翻译                                                            */
/* ------------------------------------------------------------------ */

/// 百度翻译开放平台 · 通用翻译 API。
///
/// 官方文档：https://fanyi-api.baidu.com/doc/21
///
/// 签名：`sign = md5(appid + q + salt + secret)` —— 注意 **q 用原文**
/// （不 URL 编码），编码是 HTTP 层的事。
fn baidu(info: &EngineInfo, creds: &Creds, text: &str, target: &str) -> Result<String, String> {
    let appid = creds.key.trim();
    let secret = creds.secret.trim();
    if appid.is_empty() || secret.is_empty() {
        return Err("百度翻译要同时填「APP ID」和「密钥」（设置 → 翻译）".into());
    }
    let salt = salt();
    let url = creds.base_for(info);
    let lang = baidu_lang(&crate::trans::normalize_lang(target));

    let sign = md5_hex(format!("{appid}{text}{salt}{secret}").as_bytes());
    let form = vec![
        ("q", text.to_string()),
        ("from", "auto".to_string()),
        ("to", lang.to_string()),
        ("appid", appid.to_string()),
        ("salt", salt),
        ("sign", sign),
    ];
    let v = post_form(&url, &form)?;
    // 百度出错时也回 200，错误码在 error_code（"52000" 之类的通用错误，
    // "54001" = 签名错，"54003" = 访问频率超限）。
    if let Some(code) = v.get("error_code").and_then(|x| x.as_str()) {
        let msg = v.get("error_msg").and_then(|x| x.as_str()).unwrap_or("");
        return Err(format!("百度翻译报错 {code}：{msg}"));
    }
    /* ⚠ `trans_result` 是**按段落分行**的数组：请求 q 里的每个 `\n` 对应一个
     * `{"src","dst"}` 元素。以前只取 `[0]`，多行原文除第一行外全部静默丢弃
     * （不报错、界面也不提示），用户会以为百度只翻了一行。这里按 `\n` 拼回去。 */
    let out = v
        .get("trans_result")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|e| e.get("dst").and_then(|d| d.as_str()))
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "百度翻译返回里没有译文（trans_result 是空的）".to_string())?;
    Ok(out)
}

/// 我们的语言码 → 百度的语言码。
fn baidu_lang(code: &str) -> &'static str {
    match code {
        "zh" => "zh",
        "zh-TW" => "cht",
        "en" => "en",
        "ja" => "jp",
        "ko" => "kor",
        "fr" => "fra",
        "de" => "de",
        "es" => "spa",
        "ru" => "ru",
        "pt" => "pt",
        "it" => "it",
        "ar" => "ara",
        _ => "zh",
    }
}

/* ------------------------------------------------------------------ */
/* 谷歌翻译                                                            */
/* ------------------------------------------------------------------ */

/// Google Cloud Translation v2。
///
/// 官方文档：https://cloud.google.com/translate/docs/reference/rest/v2/translate
///
/// 凭据是**一个** API Key，放在查询串 `key=` 上（不是 Authorization 头）。
fn google(info: &EngineInfo, creds: &Creds, text: &str, target: &str) -> Result<String, String> {
    let key = creds.key.trim();
    if key.is_empty() {
        return Err("还没有填谷歌翻译的 API Key（设置 → 翻译）".into());
    }
    let lang = google_lang(&crate::trans::normalize_lang(target));
    let base = creds.base_for(info);
    // 官方端点是 /language/translate/v2，Key 走查询串。
    let url = format!("{base}?key={key}");

    let form = vec![
        ("q", text.to_string()),
        ("target", lang.to_string()),
        ("source", "auto".to_string()),
        ("format", "text".to_string()),
    ];
    let v = post_form(&url, &form)?;
    if let Some(err) = v.get("error") {
        let msg = err
            .get("message")
            .and_then(|x| x.as_str())
            .unwrap_or("未知错误");
        let code = err.get("code").and_then(|x| x.as_i64()).unwrap_or(0);
        return Err(format!("谷歌翻译报错 {code}：{msg}"));
    }
    let out = v
        .pointer("/data/translations/0/translatedText")
        .and_then(|x| x.as_str())
        .map(|s| {
            // 谷歌在 format=text 时有时仍会返回 HTML 实体（&amp; 之类）
            decode_entities(s)
        })
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "谷歌翻译返回里没有译文（data.translations 是空的）".to_string())?;
    Ok(out)
}

/// 我们的语言码 → 谷歌的语言码。
fn google_lang(code: &str) -> &'static str {
    match code {
        "zh" => "zh-CN",
        "zh-TW" => "zh-TW",
        "en" => "en",
        "ja" => "ja",
        "ko" => "ko",
        "fr" => "fr",
        "de" => "de",
        "es" => "es",
        "ru" => "ru",
        "pt" => "pt",
        "it" => "it",
        "ar" => "ar",
        _ => "zh-CN",
    }
}

/* ------------------------------------------------------------------ */
/* 有道智云                                                            */
/* ------------------------------------------------------------------ */

/// 有道智云 · 文本翻译。
///
/// 官方文档：https://ai.youdao.com/DOCSIRMA/html/trans/api/wbfy/index.html
///
/// 签名（v3）：`sign = sha256(appKey + input + salt + curtime + secret)`，
/// 其中 `input` 是 q 的「截断形式」：长度 ≤ 20 时就是 q 本身，
/// 否则是 **前 10 个字符 + 长度 + 后 10 个字符**（官方 JS demo 的 `truncate()`）。
/// ⚠ 不是 `sha256(q)` —— 写成那样服务端重算必然对不上，任何超过 20 字的句子
///   都会稳定报 202（签名检验失败）。
fn youdao(info: &EngineInfo, creds: &Creds, text: &str, target: &str) -> Result<String, String> {
    let app_key = creds.key.trim();
    let secret = creds.secret.trim();
    if app_key.is_empty() || secret.is_empty() {
        return Err("有道翻译要同时填「应用 ID」和「应用密钥」（设置 → 翻译）".into());
    }
    let salt = salt();
    let curtime = now_secs().to_string();
    let url = creds.base_for(info);
    let lang = youdao_lang(&crate::trans::normalize_lang(target));

    let input = youdao_sign_input(text);
    let sign = crate::ocr_engines::sha256_hex(
        format!("{app_key}{input}{salt}{curtime}{secret}").as_bytes(),
    );
    let form = vec![
        ("q", text.to_string()),
        ("from", "auto".to_string()),
        ("to", lang.to_string()),
        ("appKey", app_key.to_string()),
        ("salt", salt),
        ("curtime", curtime),
        ("sign", sign),
        ("signType", "v3".to_string()),
    ];
    let v = post_form(&url, &form)?;
    // 有道的错误码是**字符串**，非 "0" 才是成功（101 缺参数、108 应用不存在…）
    if let Some(code) = v.get("errorCode").and_then(|x| x.as_str()) {
        if code != "0" {
            let msg = v.get("errorMsg").and_then(|x| x.as_str()).unwrap_or("");
            return Err(format!("有道翻译报错 {code}：{msg}"));
        }
    }
    /* 有道的 `translation` 是**字符串数组**（`{"translation":["你好"]}`），
     * 元素上没有 dst 字段。按 `/translation/0/dst` 取值恒为 None ——
     * 即使服务端 200 + errorCode=0（而且已经计费），也会被判成"没有译文"。
     * 多行时逐段拼回（和百度那一族同一个口径）。 */
    let out = v
        .get("translation")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "有道翻译返回里没有译文（translation 是空的）".to_string())?;
    Ok(out)
}

/// 有道 v3 签名里的 `input`：≤20 字原样返回，否则「前 10 + 长度 + 后 10」。
///
/// 长度与截取都按**字符**算（一个汉字是 3 字节，但官方算的是字符数）。
fn youdao_sign_input(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    if n <= 20 {
        return text.to_string();
    }
    let head: String = chars[..10].iter().collect();
    let tail: String = chars[n - 10..].iter().collect();
    format!("{head}{n}{tail}")
}

/// 我们的语言码 → 有道的语言码。
fn youdao_lang(code: &str) -> &'static str {
    match code {
        "zh" => "zh-CHS",
        "zh-TW" => "zh-CHT",
        "en" => "en",
        "ja" => "ja",
        "ko" => "ko",
        "fr" => "fr",
        "de" => "de",
        "es" => "es",
        "ru" => "ru",
        "pt" => "pt",
        "it" => "it",
        "ar" => "ar",
        _ => "zh-CHS",
    }
}

/* ------------------------------------------------------------------ */
/* 公共部分                                                            */
/* ------------------------------------------------------------------ */

/// 随机 salt（各家都要求一个随机串参与签名）。
fn salt() -> String {
    format!("{:x}", rand::random::<u64>())
}

/// epoch 秒。
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// MD5 的十六进制小写（百度签名用）。
pub fn md5_hex(data: &[u8]) -> String {
    let d = md5::compute(data);
    let mut s = String::with_capacity(32);
    for b in d.iter() {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// 表单 POST，返回 JSON。
///
/// 错误分类沿用 OpenAI 兼容那套的口径：**必须说清是哪一家**（用户同时勾了
/// 三家、其中一家挂了，得知道该去改哪家的凭据）。
///
/// 连接池复用 + 一次重试，理由见 [`crate::net::trans_agent`]：这三家一次翻译
/// 只发一条请求、而且用户往往隔一会儿才翻一次，正好是"空闲连接最可能被对端
/// 关掉"的用法 —— 所以撞上 Transport 错就直接原样重发一次（表单请求无副作用）。
fn post_form(url: &str, form: &[(&str, String)]) -> Result<Value, String> {
    let pairs: Vec<(&str, &str)> = form.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let mut last: Option<String> = None;
    for attempt in 0..2 {
        let resp = crate::net::trans_agent()
            .post(url)
            // 老牌三家一问一答，60 秒没回就是真出问题了
            .timeout(std::time::Duration::from_secs(60))
            .set("Content-Type", "application/x-www-form-urlencoded")
            .send_form(&pairs);
        match resp {
            Ok(r) => {
                let text = r
                    .into_string()
                    .map_err(|e| format!("读响应失败：{e}"))?;
                return serde_json::from_str(&text).map_err(|e| {
                    let t: String = text.chars().take(160).collect();
                    format!("返回的不是 JSON：{e}（{t}）")
                });
            }
            Err(ureq::Error::Status(code, r)) => {
                // 对方明确回了状态码 —— 重试没有意义，直接报
                let body = r.into_string().unwrap_or_default();
                return Err(format!(
                    "返回 HTTP {code}：{}",
                    crate::trans_engines::vendor_msg(&body)
                ));
            }
            Err(other) => {
                let msg = crate::ocr::explain(&other);
                last = Some(msg);
                if attempt == 1 {
                    break;
                }
            }
        }
    }
    Err(last.unwrap_or_else(|| "请求没发出去".to_string()))
}

/// 谷歌偶尔会把 `&` 之类编码成 HTML 实体（即使 format=text），
/// 译文中出现 `&amp;` 是错字，这里还原常见几个。
fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

/// 造一份"只填了该填的"的凭据（测试用）。
#[cfg(test)]
fn creds(key: &str, secret: &str) -> Creds {
    Creds {
        key: key.to_string(),
        secret: secret.to_string(),
        base: String::new(),
        model: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trans_engines;

    /// 起一个只服务一次的本地 HTTP 服务：把收到的请求交给 `check`，
    /// 按 `reply` 回一段 JSON。返回形如 `http://127.0.0.1:PORT/` 的根地址。
    fn serve_once(reply: &'static str, check: impl FnOnce(&str) + Send + 'static) -> String {
        use std::io::{Read, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = l.accept() {
                let mut buf: Vec<u8> = Vec::new();
                let mut tmp = [0u8; 4096];
                let head_end = loop {
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                    match s.read(&mut tmp) {
                        Ok(0) | Err(_) => break buf.len(),
                        Ok(n) => buf.extend_from_slice(&tmp[..n]),
                    }
                };
                let head = String::from_utf8_lossy(&buf[..head_end.min(buf.len())]).to_string();
                let cl = head
                    .lines()
                    .find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        if k.eq_ignore_ascii_case("content-length") {
                            v.trim().parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
                while buf.len() < head_end + cl {
                    match s.read(&mut tmp) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buf.extend_from_slice(&tmp[..n]),
                    }
                }
                check(&String::from_utf8_lossy(&buf).to_string());
                let body = reply.as_bytes();
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(head.as_bytes());
                let _ = s.write_all(body);
                let _ = s.flush();
            }
        });
        format!("http://{addr}/")
    }

    fn eng(id: &str) -> &'static EngineInfo {
        trans_engines::find(id).unwrap()
    }

    #[test]
    fn 百度签名是md5且带全四个参数() {
        let url = serve_once(
            r#"{"trans_result":[{"src":"hello","dst":"你好"}]}"#,
            |req| {
                assert!(req.contains("appid=APP123"), "{req}");
                assert!(req.contains("to=zh"), "{req}");
                assert!(req.contains("from=auto"), "{req}");
                assert!(req.contains("sign="), "{req}");
                // salt 也要在
                assert!(req.contains("salt="), "{req}");
            },
        );
        let mut c = creds("APP123", "SECRET");
        c.base = url;
        let out = translate(eng("baidu"), &c, "hello", "zh").unwrap();
        assert_eq!(out, "你好");
    }

    /// 百度把多行原文按段落拆成 `trans_result[]`，必须全部拼回 ——
    /// 以前只取 `[0]`，第二行起静默消失（不报错，用户以为只翻了一行）。
    #[test]
    fn 百度多行译文按行拼回() {
        let url = serve_once(
            r#"{"trans_result":[{"src":"a","dst":"甲"},{"src":"b","dst":"乙"},{"src":"c","dst":"丙"}]}"#,
            |_| {},
        );
        let mut c = creds("APP123", "SECRET");
        c.base = url;
        let out = translate(eng("baidu"), &c, "a\nb\nc", "zh").unwrap();
        assert_eq!(out, "甲\n乙\n丙");
    }

    #[test]
    fn 百度错误码要变成人话() {
        let url = serve_once(
            r#"{"error_code":"54001","error_msg":"Invalid Sign"}"#,
            |_| {},
        );
        let mut c = creds("APP123", "SECRET");
        c.base = url;
        let e = translate(eng("baidu"), &c, "hello", "zh").unwrap_err();
        assert!(e.contains("54001") && e.contains("Invalid Sign"), "{e}");
    }

    #[test]
    fn 百度少填一样就明确报错() {
        let e = translate(eng("baidu"), &creds("APP123", ""), "hello", "zh").unwrap_err();
        assert!(e.contains("密钥"), "{e}");
        let e = translate(eng("baidu"), &creds("", "SECRET"), "hello", "zh").unwrap_err();
        assert!(e.contains("APP ID"), "{e}");
    }

    #[test]
    fn 谷歌把key放查询串并解析译文() {
        let url = serve_once(
            r#"{"data":{"translations":[{"translatedText":"你好"}]}}"#,
            |req| {
                assert!(req.contains("key=KEY1"), "{req}");
                assert!(req.contains("target=zh-CN"), "{req}");
                assert!(req.contains("source=auto"), "{req}");
            },
        );
        let mut c = creds("KEY1", "");
        c.base = url;
        let out = translate(eng("google"), &c, "hello", "zh").unwrap();
        assert_eq!(out, "你好");
    }

    #[test]
    fn 谷歌的html实体会被还原() {
        let url = serve_once(
            r#"{"data":{"translations":[{"translatedText":"A &amp; B"}]}}"#,
            |_| {},
        );
        let mut c = creds("KEY1", "");
        c.base = url;
        let out = translate(eng("google"), &c, "hello", "zh").unwrap();
        assert_eq!(out, "A & B");
    }

    #[test]
    fn 有道用sha256签名并解析译文() {
        let url = serve_once(
            // ⚠ 真实契约：`translation` 是**字符串数组**，元素没有 dst 字段。
            //   以前这里用的是 `[{"src","dst"}]` 的伪形状，把错误解析钉死了。
            r#"{"translation":["你好"],"errorCode":"0"}"#,
            |req| {
                assert!(req.contains("appKey=AK"), "{req}");
                assert!(req.contains("signType=v3"), "{req}");
                assert!(req.contains("sign="), "{req}");
                assert!(req.contains("to=zh-CHS"), "{req}");
            },
        );
        let mut c = creds("AK", "SK");
        c.base = url;
        let out = translate(eng("youdao"), &c, "hello", "zh").unwrap();
        assert_eq!(out, "你好");
    }

    /// 有道 v3 的 `input` 规则（≤20 原样；否则 前10 + 长度 + 后10）。
    ///
    /// 这条钉的是"签名串到底怎么拼" —— 写成 `sha256(q)` 时任何长句都会 202。
    #[test]
    fn 有道签名input按前10加长度加后10截断() {
        assert_eq!(youdao_sign_input("hello"), "hello");
        // 正好 20 字：不截断
        let s20 = "一二三四五六七八九十一二三四五六七八九十";
        assert_eq!(s20.chars().count(), 20);
        assert_eq!(youdao_sign_input(s20), s20);
        // 21 字：前 10 + 21 + 后 10（后 10 是 [11..21]，即第 12 个字起）
        let s21 = "一二三四五六七八九十一二三四五六七八九十甲";
        assert_eq!(s21.chars().count(), 21);
        assert_eq!(
            youdao_sign_input(s21),
            "一二三四五六七八九十21二三四五六七八九十甲"
        );
        // 按**字符**而不是字节：中文一句 12 个字（36 字节）不该被当成"超过 20"
        let cn = "这是一句十二个汉字的话";
        assert_eq!(cn.chars().count(), 11);
        assert_eq!(youdao_sign_input(cn), cn);
    }

    /// 多行原文：服务端按段落返回数组，必须逐行拼回去，不能只留第一行。
    #[test]
    fn 有道多行译文按行拼回() {
        let url = serve_once(
            r#"{"translation":["第一行","第二行","第三行"],"errorCode":"0"}"#,
            |_| {},
        );
        let mut c = creds("AK", "SK");
        c.base = url;
        let out = translate(eng("youdao"), &c, "a\nb\nc", "zh").unwrap();
        assert_eq!(out, "第一行\n第二行\n第三行");
    }

    #[test]
    fn 有道错误码非0要报错() {
        let url = serve_once(
            r#"{"errorCode":"108","errorMsg":"invalid appKey"}"#,
            |_| {},
        );
        let mut c = creds("AK", "SK");
        c.base = url;
        let e = translate(eng("youdao"), &c, "hello", "zh").unwrap_err();
        assert!(e.contains("108"), "{e}");
    }

    #[test]
    fn 语言码三家各映射各的() {
        assert_eq!(baidu_lang("zh"), "zh");
        assert_eq!(baidu_lang("zh-TW"), "cht");
        assert_eq!(baidu_lang("ja"), "jp");
        assert_eq!(baidu_lang("fr"), "fra");
        assert_eq!(google_lang("zh"), "zh-CN");
        assert_eq!(google_lang("zh-TW"), "zh-TW");
        assert_eq!(youdao_lang("zh"), "zh-CHS");
        assert_eq!(youdao_lang("zh-TW"), "zh-CHT");
        // 认不出来的落回中文
        assert_eq!(baidu_lang("xx"), "zh");
        assert_eq!(google_lang("xx"), "zh-CN");
        assert_eq!(youdao_lang("xx"), "zh-CHS");
    }

    #[test]
    fn md5算得对() {
        // 公认的三组测试向量
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            md5_hex(b"The quick brown fox jumps over the lazy dog"),
            "9e107d9d372bb6826bd81d3542a419d6"
        );
    }

    #[test]
    fn 空文字和超长文字都拦下来() {
        let e = translate(eng("baidu"), &creds("A", "B"), "  ", "zh").unwrap_err();
        assert!(e.contains("没有要翻译"), "{e}");
        let long = "啊".repeat(MAX_CHARS + 1);
        let e = translate(eng("google"), &creds("A", "B"), &long, "zh").unwrap_err();
        assert!(e.contains("太长"), "{e}");
    }

    #[test]
    fn 三家老牌都在注册表里且不显示模型行() {
        for id in ["baidu", "google", "youdao"] {
            let e = eng(id);
            assert!(!e.kind.has_models(), "{id} 不该有模型列表");
            assert!(!e.kind.needs_model_row(), "{id} 不该显示模型栏");
            assert!(!e.base.is_empty(), "{id} 要有预置端点");
        }
        assert!(eng("baidu").need_secret);
        assert!(eng("youdao").need_secret);
        assert!(!eng("google").need_secret);
    }
}
