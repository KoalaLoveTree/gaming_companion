//! Settings controls injected into the sheet page. The shell owns them, so a
//! plugin does not have to ship a settings screen.

/// Document-start script: a Settings button and a shortcut field.
pub(crate) fn script(hotkey: &str, system_confirms: bool, overlay: bool) -> String {
    let hint = if system_confirms {
        "Save applies this shortcut. The system asks only the first time."
    } else {
        "This shortcut shows and hides the sheet. Quit from the tray icon."
    };
    TEMPLATE
        .replace("__HOTKEY__", &js_quote(hotkey))
        .replace("__HINT__", &js_quote(hint))
        .replace("__OVERLAY__", if overlay { "true" } else { "false" })
}

/// Tell the open page which shortcut is saved, and a short status line.
pub(crate) fn apply_call(hotkey: &str, status: &str, overlay: bool) -> String {
    format!(
        "if(window.__gc)window.__gc.apply({},{},{});",
        js_quote(hotkey),
        js_quote(status),
        if overlay { "true" } else { "false" }
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

const TEMPLATE: &str = r##"(function(){
if(window.__gc)return;
var current=__HOTKEY__;
var hint=__HINT__;
var statusText=hint;
var draft=null;
var recording=false;
var opened=false;
var overlayOn=__OVERLAY__;
var root=null, bar=null, field=null, change=null, status=null, opener=null, mode=null, moveBtn=null, sizeBtn=null;
var lock={
  position:"relative", display:"inline-block", width:"auto", height:"auto",
  "min-width":"0", "max-width":"none", "min-height":"0", "max-height":"none",
  float:"none", clear:"none", top:"auto", right:"auto", bottom:"auto", left:"auto",
  margin:"0", padding:"0", flex:"0 0 auto", "align-self":"center", transform:"none",
  "box-sizing":"border-box", "pointer-events":"auto", "vertical-align":"middle",
  "white-space":"nowrap", overflow:"hidden", "text-overflow":"ellipsis"
};
function pin(node, styles){
  node.style.cssText="all:initial";
  var merged={};
  var key;
  for(key in lock)merged[key]=lock[key];
  for(key in styles)merged[key]=styles[key];
  for(key in merged)node.style.setProperty(key, merged[key], "important");
}
function paint(){
  if(!field)return;
  field.textContent=recording?"Press a shortcut":(draft||current);
  change.textContent=recording?"Cancel":"Change";
  status.textContent=statusText;
}
window.__gc={
  apply:function(next, message, overlay){
    statusText=message||"";
    if(!recording){
      if(next)current=next;
      draft=null;
    }
    if(typeof overlay==="boolean")setOverlay(overlay);
    paint();
  }
};
function setOverlay(on){
  overlayOn=!!on;
  var style=document.getElementById("gc-overlay-style");
  if(!overlayOn){
    if(style)style.remove();
  }else{
    if(!style){
      style=document.createElement("style");
      style.id="gc-overlay-style";
      (document.head||document.documentElement).appendChild(style);
    }
    style.textContent="html{background:transparent !important;background-image:none !important}body{background-color:rgba(12, 12, 14, 0.78) !important;background-image:none !important;margin:16px !important}";
  }
  if(root)root.style.setProperty("bottom", overlayOn?"16px":"0", "important");
  if(mode)mode.textContent=overlayOn?"Window":"Overlay";
  if(moveBtn)moveBtn.style.setProperty("display", "inline-block", "important");
  if(sizeBtn)sizeBtn.style.setProperty("display", "inline-block", "important");
}
function mapCode(code){
  if(code==="Space")return "space";
  if(code==="Tab")return "tab";
  if(code==="Escape")return "escape";
  if(code==="Enter")return "enter";
  if(code==="Backspace")return "backspace";
  if(code==="Delete")return "delete";
  if(code==="Insert")return "insert";
  if(code==="Home")return "home";
  if(code==="End")return "end";
  if(code==="PageUp")return "pageup";
  if(code==="PageDown")return "pagedown";
  if(code==="ArrowUp")return "up";
  if(code==="ArrowDown")return "down";
  if(code==="ArrowLeft")return "left";
  if(code==="ArrowRight")return "right";
  if(code==="Minus")return "minus";
  if(code==="Equal")return "equal";
  if(code==="Comma")return "comma";
  if(code==="Period")return "period";
  if(code==="Slash")return "slash";
  if(code==="Backslash")return "backslash";
  if(code==="Backquote")return "grave";
  if(code==="BracketLeft")return "bracketleft";
  if(code==="BracketRight")return "bracketright";
  if(code==="Semicolon")return "semicolon";
  if(code==="Quote")return "quote";
  if(/^Key[A-Z]$/.test(code))return code.slice(3).toLowerCase();
  if(/^Digit[0-9]$/.test(code))return code.slice(5);
  if(/^F([1-9]|1[0-2])$/.test(code))return code.toLowerCase();
  return null;
}
function eat(event){
  event.preventDefault();
  event.stopPropagation();
  if(event.stopImmediatePropagation)event.stopImmediatePropagation();
}
function setOpen(open){
  opened=open;
  if(!root)return;
  if(open){
    root.style.setProperty("left","0","important");
    root.style.setProperty("right","0","important");
    root.style.setProperty("background","#1c1c1c","important");
    root.style.setProperty("border-top","1px solid #3a3a3a","important");
    opener.style.setProperty("display","none","important");
    bar.style.setProperty("display","flex","important");
    try{window.focus();}catch(err){}
  }else{
    recording=false;
    root.style.setProperty("left","auto","important");
    root.style.setProperty("right","0","important");
    root.style.setProperty("background","transparent","important");
    root.style.setProperty("border-top","0","important");
    opener.style.setProperty("display","inline-block","important");
    bar.style.setProperty("display","none","important");
    paint();
  }
}
function mount(){
  if(!document.documentElement)return;
  if(document.getElementById("gc-settings-root"))return;
  root=document.createElement("div");
  root.id="gc-settings-root";
  pin(root,{
    position:"fixed", bottom:"0", left:"auto", right:"0", "z-index":"2147483647",
    display:"flex", "flex-direction":"row", "flex-wrap":"nowrap",
    "align-items":"center", "justify-content":"flex-end", gap:"8px",
    width:"auto", "max-height":"48px", padding:"6px 10px", margin:"0",
    background:"transparent", "pointer-events":"none",
    "font":"14px/1.2 system-ui, sans-serif", "overflow-x":"auto", "overflow-y":"hidden"
  });
  opener=document.createElement("button");
  opener.type="button";
  opener.textContent="Settings";
  pin(opener,{
    display:"inline-block", padding:"6px 10px", "border-radius":"8px",
    border:"1px solid #4a4a4a", background:"#1c1c1c", color:"#f2f2f2",
    "font":"14px/1.2 system-ui, sans-serif", cursor:"pointer",
    appearance:"none", "-webkit-appearance":"none"
  });
  bar=document.createElement("div");
  pin(bar,{
    display:"none", "flex-direction":"row", "flex-wrap":"nowrap",
    "align-items":"center", "justify-content":"flex-start", gap:"8px",
    flex:"1 1 auto", width:"auto", "min-width":"0", "pointer-events":"auto"
  });
  field=document.createElement("div");
  pin(field,{
    flex:"0 1 auto", "min-width":"8em", "max-width":"16em", padding:"6px 10px",
    "border-radius":"8px", border:"1px solid #4a4a4a", background:"#111",
    color:"#f2f2f2", "font":"14px/1.3 ui-monospace, monospace"
  });
  var buttonStyle={
    display:"inline-block", padding:"6px 12px", "border-radius":"8px",
    border:"1px solid #4a4a4a", background:"#2a2a2a", color:"#f2f2f2",
    "font":"14px/1.2 system-ui, sans-serif", cursor:"pointer",
    appearance:"none", "-webkit-appearance":"none"
  };
  change=document.createElement("button");
  change.type="button";
  change.textContent="Change";
  pin(change, buttonStyle);
  var save=document.createElement("button");
  save.type="button";
  save.textContent="Save";
  pin(save, buttonStyle);
  status=document.createElement("div");
  pin(status,{
    flex:"1 1 auto", "min-width":"0", color:"#d7d7d7",
    "font":"13px/1.3 system-ui, sans-serif"
  });
  mode=document.createElement("button");
  mode.type="button";
  mode.textContent="Overlay";
  pin(mode, buttonStyle);
  moveBtn=document.createElement("button");
  moveBtn.type="button";
  moveBtn.textContent="Move";
  pin(moveBtn, buttonStyle);
  sizeBtn=document.createElement("button");
  sizeBtn.type="button";
  sizeBtn.textContent="Size";
  pin(sizeBtn, buttonStyle);
  var close=document.createElement("button");
  close.type="button";
  close.textContent="Close";
  pin(close, buttonStyle);
  bar.appendChild(field);
  bar.appendChild(change);
  bar.appendChild(save);
  bar.appendChild(mode);
  bar.appendChild(status);
  bar.appendChild(close);
  root.appendChild(bar);
  root.appendChild(moveBtn);
  root.appendChild(sizeBtn);
  root.appendChild(opener);
  document.documentElement.appendChild(root);
  opener.addEventListener("click", function(event){
    eat(event);
    setOpen(true);
  }, true);
  close.addEventListener("click", function(event){
    eat(event);
    setOpen(false);
  }, true);
  change.addEventListener("click", function(event){
    eat(event);
    recording=!recording;
    if(recording){
      draft=null;
      statusText="Press the keys";
      try{window.focus();}catch(err){}
    }else{
      draft=null;
      statusText=hint;
    }
    paint();
  }, true);
  save.addEventListener("click", function(event){
    eat(event);
    if(!draft){
      statusText="Press Change, then the keys";
      paint();
      return;
    }
    var value=draft;
    recording=false;
    if(window.ipc&&window.ipc.postMessage)window.ipc.postMessage("set:"+value);
    paint();
  }, true);
  mode.addEventListener("click", function(event){
    eat(event);
    if(window.ipc&&window.ipc.postMessage)window.ipc.postMessage("overlay");
  }, true);
  moveBtn.addEventListener("pointerdown", function(event){
    eat(event);
    if(window.ipc&&window.ipc.postMessage)window.ipc.postMessage("drag");
  }, true);
  sizeBtn.addEventListener("pointerdown", function(event){
    eat(event);
    if(window.ipc&&window.ipc.postMessage)window.ipc.postMessage("resize");
  }, true);
  paint();
  setOverlay(overlayOn);
  if(opened)setOpen(true);
  if(window.ipc&&window.ipc.postMessage)window.ipc.postMessage("ready");
  if(!window.__gcObs && window.MutationObserver){
    window.__gcObs=new MutationObserver(function(){
      if(!document.getElementById("gc-settings-root"))mount();
    });
    window.__gcObs.observe(document.documentElement,{childList:true,subtree:true});
  }
}
window.addEventListener("keydown", function(event){
  if(!recording)return;
  eat(event);
  if(event.repeat)return;
  var bare=!(event.ctrlKey||event.altKey||event.shiftKey||event.metaKey);
  if(event.code==="Escape"&&bare){
    recording=false;
    draft=null;
    statusText=hint;
    paint();
    return;
  }
  var key=mapCode(event.code);
  if(!key)return;
  var parts=[];
  if(event.ctrlKey)parts.push("ctrl");
  if(event.altKey)parts.push("alt");
  if(event.shiftKey)parts.push("shift");
  if(event.metaKey)parts.push("super");
  parts.push(key);
  draft=parts.join("+");
  recording=false;
  statusText="Press Save";
  paint();
}, true);
mount();
document.addEventListener("DOMContentLoaded", mount);
window.addEventListener("load", mount);
})();
"##;

#[cfg(test)]
mod tests {
    use super::script;

    #[test]
    fn the_settings_bar_is_one_row_at_the_bottom() {
        let source = script("ctrl+space", true, true);
        assert!(source.contains("bottom:\"0\""));
        assert!(source.contains("\"flex-direction\":\"row\""));
        assert!(source.contains("stopImmediatePropagation"));
        assert!(source.contains("Press Change, then the keys"));
        assert!(source.contains("postMessage(\"overlay\")"));
        assert!(source.contains("rgba(12, 12, 14, 0.78)"));
        assert!(source.contains("margin:16px"));
        assert!(!source.contains("top:\"10px\""));
        assert!(!source.contains("top:10px"));
    }
}
