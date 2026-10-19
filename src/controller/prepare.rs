use pulldown_cmark::{Options, Parser, html};

use super::plugins::{read_limited, resolve_inside};
use crate::Result;
use crate::model::{Page, Plugin, Resource, ResourceKind};

pub fn prepare_page(plugin: &Plugin, resource: &Resource) -> Result<Page> {
    let init_script = init_script(plugin, resource)?;
    let title = format!("{} - {}", plugin.name, resource.title);
    match resource.kind {
        ResourceKind::Url => Ok(Page {
            title,
            url: Some(resource.src.clone()),
            html: None,
            init_script,
        }),
        ResourceKind::Html => {
            let path = resolve_inside(&plugin.root, &resource.src)?;
            Ok(Page {
                title,
                url: None,
                html: Some(read_limited(&path)?),
                init_script,
            })
        }
        ResourceKind::Markdown => {
            let path = resolve_inside(&plugin.root, &resource.src)?;
            let markdown = read_limited(&path)?;
            let html = Some(markdown_document(&title, &markdown));
            Ok(Page {
                title,
                url: None,
                html,
                init_script,
            })
        }
    }
}

fn init_script(plugin: &Plugin, resource: &Resource) -> Result<Option<String>> {
    if resource.inject.is_empty() {
        return Ok(None);
    }
    let mut css = String::new();
    for inject in &resource.inject {
        let path = resolve_inside(&plugin.root, inject)?;
        css.push_str(&read_limited(&path)?);
        css.push('\n');
    }
    Ok(Some(format!(
        "(function(){{var s=document.createElement('style');s.setAttribute('data-gaming-companion','inject');s.textContent={};(document.head||document.documentElement).appendChild(s);}})();",
        js_quote(&css)
    )))
}

fn markdown_document(title: &str, markdown: &str) -> String {
    let mut body = String::new();
    let parser = Parser::new_ext(
        markdown,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH,
    );
    html::push_html(&mut body, parser);
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>{}</title>\
<style>body{{font:16px/1.45 sans-serif;margin:24px;background:#161616;color:#eee}}a{{color:#8cf}}</style>\
</head><body>{}</body></html>",
        html_escape(title),
        body
    )
}

fn js_quote(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn html_escape(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            ch => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::plugins::load_plugin;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn css_injection_is_a_script_that_contains_the_file() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/sample");
        let plugin = load_plugin(&root).unwrap();
        let resource = plugin.resource("page").unwrap();
        let page = prepare_page(&plugin, resource).unwrap();
        assert_eq!(page.url.as_deref(), Some("https://example.com/sheet"));
        let script = page.init_script.unwrap();
        assert!(script.contains("data-gaming-companion"));
        assert!(script.contains("sample-marker"));
    }

    #[test]
    fn markdown_becomes_a_document() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("gaming-companion-md-{nanos}"));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("plugin.toml"),
            r#"
id = "pad"
name = "Pad & <co>"
entry = "note"
[[resource]]
id = "note"
title = "Note"
kind = "markdown"
src = "note.md"
"#,
        )
        .unwrap();
        fs::write(root.join("note.md"), "Hello **there**\n").unwrap();
        let plugin = load_plugin(&root).unwrap();
        let page = prepare_page(&plugin, plugin.resource("note").unwrap()).unwrap();
        let html = page.html.unwrap();
        assert!(html.contains("<strong>there</strong>"));
        assert!(html.contains("Pad &amp; &lt;co&gt;"));
        assert!(page.url.is_none());
        let _ = fs::remove_dir_all(root);
    }
}
