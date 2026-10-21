use pulldown_cmark::{Options, Parser, html};

use super::plugins::{read_limited, resolve_inside};
use crate::Result;
use crate::js;
use crate::model::{Page, Plugin, Resource, ResourceKind};

pub fn prepare_page(plugin: &Plugin, resource: &Resource) -> Result<Page> {
    let init_script = init_script(plugin, resource)?;
    let title = format!("{} - {}", plugin.name, resource.title);
    let css = css_text(plugin, resource)?;
    match resource.kind {
        ResourceKind::Url => Ok(Page::Url {
            title,
            url: resource.src.clone(),
            init_script,
        }),
        ResourceKind::Html => {
            let path = resolve_inside(&plugin.root, &resource.src)?;
            let mut html = read_limited(&path)?;
            if let Some(css) = css {
                html = embed_style(&html, &css);
            }
            Ok(Page::Html {
                title,
                html,
                init_script,
            })
        }
        ResourceKind::Markdown => {
            let path = resolve_inside(&plugin.root, &resource.src)?;
            let markdown = read_limited(&path)?;
            let html = markdown_document(&title, &markdown, css.as_deref().unwrap_or(""));
            Ok(Page::Html {
                title,
                html,
                init_script,
            })
        }
    }
}

/// Document-start script for every URL resource that injects CSS.
/// It runs on each navigation and applies the sheet only when the address matches.
pub fn catalog_script(plugins: &[Plugin]) -> Result<Option<String>> {
    let mut rules = Vec::new();
    for plugin in plugins {
        for resource in &plugin.resources {
            if resource.kind != ResourceKind::Url {
                continue;
            }
            let Some(css) = css_text(plugin, resource)? else {
                continue;
            };
            rules.push(format!(
                "{{src:{},css:{}}}",
                js::quote(&resource.src),
                js::quote(&css)
            ));
        }
    }
    if rules.is_empty() {
        return Ok(None);
    }
    Ok(Some(format!(
        "(function(){{var rules=[{}];var href=String(location.href||'');for(var i=0;i<rules.length;i++){{var src=rules[i].src;if(href!==src&&href.indexOf(src+'/')!==0&&href.indexOf(src+'?')!==0&&href.indexOf(src+'#')!==0)continue;var s=document.createElement('style');s.setAttribute('data-gaming-companion','inject');s.textContent=rules[i].css;(document.head||document.documentElement).appendChild(s);return;}}}})();",
        rules.join(",")
    )))
}

/// The URL resource whose address is the longest match for `url`.
pub fn matching_resource<'a>(
    plugins: &'a [Plugin],
    url: &str,
) -> Option<(&'a Plugin, &'a Resource)> {
    let mut best: Option<(&Plugin, &Resource, usize)> = None;
    for plugin in plugins {
        for resource in &plugin.resources {
            if resource.kind != ResourceKind::Url || !url_matches(&resource.src, url) {
                continue;
            }
            let len = resource.src.len();
            if best.as_ref().is_none_or(|(_, _, known)| len > *known) {
                best = Some((plugin, resource, len));
            }
        }
    }
    best.map(|(plugin, resource, _)| (plugin, resource))
}

fn url_matches(src: &str, url: &str) -> bool {
    url == src
        || url.starts_with(&format!("{src}/"))
        || url.starts_with(&format!("{src}?"))
        || url.starts_with(&format!("{src}#"))
}

fn css_text(plugin: &Plugin, resource: &Resource) -> Result<Option<String>> {
    if resource.inject.is_empty() {
        return Ok(None);
    }
    let mut css = String::new();
    for inject in &resource.inject {
        let path = resolve_inside(&plugin.root, inject)?;
        css.push_str(&read_limited(&path)?);
        css.push('\n');
    }
    Ok(Some(css))
}

fn init_script(plugin: &Plugin, resource: &Resource) -> Result<Option<String>> {
    let Some(css) = css_text(plugin, resource)? else {
        return Ok(None);
    };
    Ok(Some(format!(
        "(function(){{var s=document.createElement('style');s.setAttribute('data-gaming-companion','inject');s.textContent={};(document.head||document.documentElement).appendChild(s);}})();",
        js::quote(&css)
    )))
}

fn embed_style(html: &str, css: &str) -> String {
    let tag = format!(
        "<style data-gaming-companion=\"inject\">{}</style>",
        css.replace("</style", "<\\/style")
    );
    let lower = html.to_ascii_lowercase();
    if let Some(index) = lower.find("</head>") {
        let mut out = String::new();
        out.push_str(&html[..index]);
        out.push_str(&tag);
        out.push_str(&html[index..]);
        return out;
    }
    format!("{tag}{html}")
}

fn markdown_document(title: &str, markdown: &str, css: &str) -> String {
    let mut body = String::new();
    let parser = Parser::new_ext(
        markdown,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH,
    );
    html::push_html(&mut body, parser);
    let extra = if css.is_empty() {
        String::new()
    } else {
        format!(
            "<style data-gaming-companion=\"inject\">{}</style>",
            css.replace("</style", "<\\/style")
        )
    };
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>{}</title>\
{extra}<style>body{{font:16px/1.45 sans-serif;margin:24px;background:#161616;color:#eee}}a{{color:#8cf}}</style>\
</head><body>{}</body></html>",
        html_escape(title),
        body
    )
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
        let Page::Url {
            url, init_script, ..
        } = page
        else {
            panic!("url resource should open as a url");
        };
        assert_eq!(url, "https://example.com/sheet");
        let script = init_script.unwrap();
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
        let Page::Html { html, .. } = page else {
            panic!("markdown should open as a document");
        };
        assert!(html.contains("<strong>there</strong>"));
        assert!(html.contains("Pad &amp; &lt;co&gt;"));
        let _ = fs::remove_dir_all(root);
    }
}
