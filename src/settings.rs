//! Settings controls injected into the sheet page. The shell owns them, so a
//! plugin does not have to ship a settings screen.
//!
//! Linux draws the border, navigation, and Settings with real widgets. Windows
//! draws them in the page, in the top bar.

use crate::model::{Bindings, Plugin};

/// Document-start script. Linux draws the border and Settings with real widgets.
/// Windows draws both in the page, in the top bar.
pub(crate) fn hint(system_confirms: bool) -> String {
    if system_confirms {
        "Save applies this shortcut. The system asks only the first time.".to_string()
    } else {
        "This shortcut shows and hides the sheet. Quit from the tray icon.".to_string()
    }
}

pub(crate) fn script(
    hotkey: &str,
    system_confirms: bool,
    overlay: bool,
    plugins: &[Plugin],
    active: &str,
    bindings: &Bindings,
) -> String {
    let hint = hint(system_confirms);
    TEMPLATE
        .replace("__HOTKEY__", &js_quote(hotkey))
        .replace("__HINT__", &js_quote(&hint))
        .replace("__OVERLAY__", if overlay { "true" } else { "false" })
        .replace("__MARGIN__", &crate::chrome::MARGIN.to_string())
        .replace("__BORDER__", &crate::chrome::BORDER.to_string())
        .replace("__TOP__", &crate::chrome::TOP.to_string())
        .replace("__PLUGINS__", &plugins_js(plugins))
        .replace("__BINDINGS__", &bindings_js(bindings))
        .replace("__ACTIVE__", &js_quote(active))
        .replace(
            "__NATIVE__",
            if cfg!(target_os = "linux") {
                "true"
            } else {
                "false"
            },
        )
}

fn bindings_js(bindings: &Bindings) -> String {
    format!(
        "{{back:{},forward:{},reload:{},zoomIn:{},zoomOut:{},zoomReset:{},fill:{}}}",
        js_quote(&bindings.back),
        js_quote(&bindings.forward),
        js_quote(&bindings.reload),
        js_quote(&bindings.zoom_in),
        js_quote(&bindings.zoom_out),
        js_quote(&bindings.zoom_reset),
        js_quote(&bindings.fill),
    )
}

fn plugins_js(plugins: &[Plugin]) -> String {
    let mut out = String::from("[");
    for (index, plugin) in plugins.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{id:{},name:{},game:{},gameName:{}}}",
            js_quote(&plugin.id),
            js_quote(&plugin.name),
            js_quote(&plugin.game_id),
            js_quote(&plugin.game_name),
        ));
    }
    out.push(']');
    out
}

/// Highlight the open plugin after the page builds the bar again.
pub(crate) fn select_call(plugin_id: &str) -> String {
    format!(
        "if(window.__gc&&window.__gc.select)window.__gc.select({});",
        js_quote(plugin_id)
    )
}

/// Tell the open page the saved page shortcuts.
pub(crate) fn bindings_call(bindings: &Bindings) -> String {
    format!(
        "if(window.__gc&&window.__gc.bindings)window.__gc.bindings({});",
        bindings_js(bindings)
    )
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
var drafts={};
var recordingId=null;
var overlayOn=__OVERLAY__;
var MARGIN=__MARGIN__, BORDER=__BORDER__, TOP=__TOP__, NATIVE=__NATIVE__;
var plugins=__PLUGINS__;
var bindings=__BINDINGS__;
var activePlugin=__ACTIVE__;
var pluginsOpen=false, settingsOpen=false;
var pluginsButton=null, settingsButton=null, pluginPanel=null, settingsPanel=null, mode=null, status=null;
var rows={};
var chrome=null, strip=null;
var CURSORS={
  north:"ns-resize", south:"ns-resize", east:"ew-resize", west:"ew-resize",
  nw:"nwse-resize", se:"nwse-resize", ne:"nesw-resize", sw:"nesw-resize"
};
var lock={
  position:"relative", display:"inline-block", width:"auto", height:"auto",
  "min-width":"0", "max-width":"none", "min-height":"0", "max-height":"none",
  float:"none", clear:"none", top:"auto", right:"auto", bottom:"auto", left:"auto",
  margin:"0", padding:"0", flex:"0 0 auto", "align-self":"center", transform:"none",
  "box-sizing":"border-box", "pointer-events":"auto", "vertical-align":"middle",
  "white-space":"nowrap", overflow:"hidden", "text-overflow":"ellipsis"
};
var ACTIONS=[
  ["show","Show or hide"],
  ["back","Back"],
  ["forward","Forward"],
  ["reload","Reload"],
  ["zoom-in","Zoom in"],
  ["zoom-out","Zoom out"],
  ["zoom-reset","Reset zoom"],
  ["fill","Fill screen"]
];
function pin(node, styles){
  node.style.cssText="all:initial";
  var merged={};
  var key;
  for(key in lock)merged[key]=lock[key];
  for(key in styles)merged[key]=styles[key];
  for(key in merged)node.style.setProperty(key, merged[key], "important");
}
function chordFor(id){
  if(id==="show")return current;
  if(id==="back")return bindings.back;
  if(id==="forward")return bindings.forward;
  if(id==="reload")return bindings.reload;
  if(id==="zoom-in")return bindings.zoomIn;
  if(id==="zoom-out")return bindings.zoomOut;
  if(id==="zoom-reset")return bindings.zoomReset;
  if(id==="fill")return bindings.fill;
  return "";
}
function displayChord(value){
  return String(value||"").split("equal").join("plus");
}
function normalizeChord(chord){
  var parts=String(chord||"").split("+");
  var key=parts[parts.length-1];
  if(key==="equal"||key==="plus"||key==="kpadd"||key==="add"){
    var kept=[];
    for(var i=0;i<parts.length-1;i++) if(parts[i]!=="shift") kept.push(parts[i]);
    kept.push("equal");
    return kept.join("+");
  }
  return parts.join("+");
}
function paintRows(){
  if(status)status.textContent=statusText;
  if(mode)mode.textContent=overlayOn?"Window":"Overlay";
  for(var i=0;i<ACTIONS.length;i++){
    var id=ACTIONS[i][0];
    var row=rows[id];
    if(!row)continue;
    if(recordingId===id){
      row.field.textContent="Press a shortcut";
      row.change.textContent="Cancel";
    }else if(drafts[id]){
      row.field.textContent=displayChord(drafts[id]);
      row.change.textContent="Change";
    }else{
      row.field.textContent=displayChord(chordFor(id));
      row.change.textContent="Change";
    }
  }
}
window.__gc={
  apply:function(next, message, overlay){
    statusText=message||"";
    if(recordingId!=="show"){
      if(next)current=next;
      delete drafts.show;
    }
    if(typeof overlay==="boolean")setOverlay(overlay);
    paintRows();
  },
  bindings:function(next){
    if(next)bindings=next;
    var ids=["back","forward","reload","zoom-in","zoom-out","zoom-reset","fill"];
    for(var i=0;i<ids.length;i++) if(recordingId!==ids[i]) delete drafts[ids[i]];
    paintRows();
  },
  select:function(id){
    activePlugin=id;
    if(!NATIVE)rebuildTree();
  }
};
function post(message){
  if(window.ipc&&window.ipc.postMessage)window.ipc.postMessage(message);
}
function chromeButton(label, extra){
  var button=document.createElement("button");
  button.type="button";
  button.textContent=label;
  var styles={
    display:"inline-block", padding:"2px 8px", "border-radius":"6px",
    border:"1px solid #4a4a4a", background:"#2a2a2a", color:"#f2f2f2",
    "font":"13px/1.2 system-ui, sans-serif", cursor:"pointer",
    appearance:"none", "-webkit-appearance":"none", "text-align":"left"
  };
  if(extra){ var key; for(key in extra)styles[key]=extra[key]; }
  pin(button, styles);
  return button;
}
function show(panel, on){
  if(!panel)return;
  panel.style.setProperty("display", on?"flex":"none", "important");
}
function setPlugins(on){
  pluginsOpen=!!on;
  if(pluginsOpen)settingsOpen=false;
  show(pluginPanel, pluginsOpen);
  show(settingsPanel, settingsOpen);
}
function setSettings(on){
  settingsOpen=!!on;
  if(settingsOpen)pluginsOpen=false;
  show(settingsPanel, settingsOpen);
  show(pluginPanel, pluginsOpen);
  if(!settingsOpen){
    recordingId=null;
    drafts={};
    paintRows();
  }
}
function closePanels(){
  pluginsOpen=false;
  settingsOpen=false;
  show(pluginPanel, false);
  show(settingsPanel, false);
  recordingId=null;
  drafts={};
  paintRows();
}
function panelsOpen(){ return pluginsOpen||settingsOpen; }
function rebuildTree(){
  if(!pluginPanel)return;
  while(pluginPanel.firstChild)pluginPanel.removeChild(pluginPanel.firstChild);
  var last="";
  for(var i=0;i<plugins.length;i++){
    var plugin=plugins[i];
    if(plugin.game!==last){
      last=plugin.game;
      var title=document.createElement("div");
      title.textContent=plugin.gameName;
      pin(title,{
        display:"block", color:"#d7d7d7", "font":"12px/1.3 system-ui, sans-serif",
        padding:"6px 4px 2px", "pointer-events":"none", "font-weight":"bold"
      });
      pluginPanel.appendChild(title);
    }
    (function(id, name, on){
      var button=chromeButton(name);
      button.style.setProperty("display","block","important");
      button.style.setProperty("margin","2px 0 2px 12px","important");
      button.style.setProperty("width","calc(100% - 12px)","important");
      if(on){
        button.style.setProperty("border-color","#f2f2f2","important");
        button.style.setProperty("background","#111111","important");
      }
      button.addEventListener("click", function(event){
        eat(event);
        post("open:"+id);
        setPlugins(false);
      }, true);
      pluginPanel.appendChild(button);
    })(plugin.id, plugin.name, plugin.id===activePlugin);
  }
}
function buildSettings(){
  if(!settingsPanel)return;
  while(settingsPanel.firstChild)settingsPanel.removeChild(settingsPanel.firstChild);
  rows={};
  var buttonStyle={
    display:"inline-block", padding:"2px 8px", "border-radius":"6px",
    border:"1px solid #4a4a4a", background:"#2a2a2a", color:"#f2f2f2",
    "font":"13px/1.2 system-ui, sans-serif", cursor:"pointer",
    appearance:"none", "-webkit-appearance":"none"
  };
  for(var i=0;i<ACTIONS.length;i++){
    (function(id, label){
      var row=document.createElement("div");
      pin(row,{
        display:"flex", "flex-direction":"row", "align-items":"center", gap:"6px",
        width:"100%", "pointer-events":"auto"
      });
      var name=document.createElement("div");
      name.textContent=label;
      pin(name,{
        display:"block", width:"7.5em", color:"#f2f2f2",
        "font":"13px/1.2 system-ui, sans-serif", "pointer-events":"none"
      });
      var field=document.createElement("div");
      pin(field,{
        flex:"1 1 auto", "min-width":"6em", padding:"2px 8px",
        "border-radius":"6px", border:"1px solid #4a4a4a", background:"#111",
        color:"#f2f2f2", "font":"13px/1.3 ui-monospace, monospace"
      });
      var change=chromeButton("Change");
      var save=chromeButton("Save");
      change.addEventListener("click", function(event){
        eat(event);
        if(recordingId===id){
          recordingId=null;
          delete drafts[id];
          statusText=hint;
        }else{
          recordingId=id;
          delete drafts[id];
          statusText="Press the keys";
          try{window.focus();}catch(err){}
        }
        paintRows();
      }, true);
      save.addEventListener("click", function(event){
        eat(event);
        if(!drafts[id]){
          statusText="Press Change, then the keys";
          paintRows();
          return;
        }
        var value=drafts[id];
        delete drafts[id];
        recordingId=null;
        post(id==="show"?"set:"+value:"bind:"+id+":"+value);
        paintRows();
      }, true);
      row.appendChild(name);
      row.appendChild(field);
      row.appendChild(change);
      row.appendChild(save);
      settingsPanel.appendChild(row);
      rows[id]={field:field, change:change};
    })(ACTIONS[i][0], ACTIONS[i][1]);
  }
  var note=document.createElement("div");
  note.textContent="Only Show or hide works while the game is focused.";
  pin(note,{
    display:"block", color:"#d7d7d7", "font":"12px/1.4 system-ui, sans-serif",
    "white-space":"normal", "max-width":"28em", "pointer-events":"none"
  });
  status=document.createElement("div");
  pin(status,{
    display:"block", color:"#d7d7d7", "font":"12px/1.4 system-ui, sans-serif",
    "white-space":"normal", "max-width":"28em"
  });
  var footer=document.createElement("div");
  pin(footer,{
    display:"flex", "flex-direction":"row", "align-items":"center", gap:"6px",
    width:"100%"
  });
  mode=document.createElement("button");
  mode.type="button";
  mode.textContent=overlayOn?"Window":"Overlay";
  pin(mode, buttonStyle);
  var close=chromeButton("Close");
  mode.addEventListener("click", function(event){
    eat(event);
    post("overlay");
  }, true);
  close.addEventListener("click", function(event){
    eat(event);
    setSettings(false);
  }, true);
  footer.appendChild(mode);
  footer.appendChild(close);
  settingsPanel.appendChild(note);
  settingsPanel.appendChild(status);
  settingsPanel.appendChild(footer);
  paintRows();
}
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
    style.textContent="html{background:transparent !important;background-image:none !important}body{background-color:rgba(12, 12, 14, 0.78) !important;background-image:none !important}";
  }
  if(mode)mode.textContent=overlayOn?"Window":"Overlay";
  placeChrome();
}
function panelStyle(top, side, offset, open, minWidth){
  var style={
    position:"absolute", top:top, display:open?"flex":"none",
    "flex-direction":"column", gap:"4px", padding:"8px",
    background:"#242424", border:"1px solid #4a4a4a", "border-radius":"8px",
    "z-index":"6", "pointer-events":"auto", "max-height":"360px",
    overflow:"auto", "white-space":"normal",
    "font":"13px/1.3 system-ui, sans-serif", color:"#f2f2f2",
    "min-width":minWidth, "max-width":"calc(100vw - 24px)"
  };
  style[side]=offset;
  return style;
}
function placeChrome(){
  var margin=NATIVE?0:(overlayOn?MARGIN:0);
  if(strip){
    pin(strip,{
      position:"absolute", top:(margin+BORDER)+"px", left:margin+"px", right:margin+"px",
      height:(TOP-BORDER)+"px", display:"flex", "align-items":"center", "z-index":"1",
      "pointer-events":"auto", cursor:"grab", padding:"0 8px 0 12px",
      background:"#242424", "border-bottom":"1px solid #f2f2f2",
      "user-select":"none", "-webkit-user-select":"none",
      "flex-direction":"row"
    });
  }
  var top=(margin+TOP)+"px";
  if(pluginPanel)pin(pluginPanel, panelStyle(top, "left", (margin+8)+"px", pluginsOpen, "180px"));
  if(settingsPanel)pin(settingsPanel, panelStyle(top, "right", (margin+8)+"px", settingsOpen, "360px"));
  if(!chrome)return;
  var edges=chrome.querySelectorAll("[data-edge]");
  for(var i=0;i<edges.length;i++){
    var kind=edges[i].getAttribute("data-edge");
    var common={
      position:"absolute", "pointer-events":"auto", background:"#4a4a4a",
      "z-index":"2", cursor:CURSORS[kind]||"default"
    };
    if(kind==="north")pin(edges[i], mix(common,{top:margin+"px", left:(margin+BORDER)+"px", right:(margin+BORDER)+"px", height:BORDER+"px", "border-bottom":"1px solid #f2f2f2"}));
    else if(kind==="south")pin(edges[i], mix(common,{bottom:margin+"px", left:(margin+BORDER)+"px", right:(margin+BORDER)+"px", height:BORDER+"px", "border-top":"1px solid #f2f2f2"}));
    else if(kind==="west")pin(edges[i], mix(common,{left:margin+"px", top:(margin+TOP)+"px", bottom:(margin+BORDER)+"px", width:BORDER+"px", "border-right":"1px solid #f2f2f2"}));
    else if(kind==="east")pin(edges[i], mix(common,{right:margin+"px", top:(margin+TOP)+"px", bottom:(margin+BORDER)+"px", width:BORDER+"px", "border-left":"1px solid #f2f2f2"}));
    else if(kind==="nw")pin(edges[i], mix(common,{top:margin+"px", left:margin+"px", width:BORDER+"px", height:BORDER+"px", "z-index":"3", "border-bottom":"1px solid #f2f2f2", "border-right":"1px solid #f2f2f2"}));
    else if(kind==="ne")pin(edges[i], mix(common,{top:margin+"px", right:margin+"px", width:BORDER+"px", height:BORDER+"px", "z-index":"3", "border-bottom":"1px solid #f2f2f2", "border-left":"1px solid #f2f2f2"}));
    else if(kind==="sw")pin(edges[i], mix(common,{bottom:margin+"px", left:margin+"px", width:BORDER+"px", height:BORDER+"px", "z-index":"3", "border-top":"1px solid #f2f2f2", "border-right":"1px solid #f2f2f2"}));
    else if(kind==="se")pin(edges[i], mix(common,{bottom:margin+"px", right:margin+"px", width:BORDER+"px", height:BORDER+"px", "z-index":"3", "border-top":"1px solid #f2f2f2", "border-left":"1px solid #f2f2f2"}));
  }
}
function mix(base, extra){
  var out={};
  var key;
  for(key in base)out[key]=base[key];
  for(key in extra)out[key]=extra[key];
  return out;
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
  if(code==="Minus"||code==="NumpadSubtract")return "minus";
  if(code==="Equal"||code==="NumpadAdd")return "equal";
  if(code==="Numpad0")return "0";
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
function matchedAction(chord){
  if(chord==="f5")return "reload";
  if(normalizeChord(bindings.back)===chord)return "back";
  if(normalizeChord(bindings.forward)===chord)return "forward";
  if(normalizeChord(bindings.reload)===chord)return "reload";
  if(normalizeChord(bindings.zoomIn)===chord)return "zoom-in";
  if(normalizeChord(bindings.zoomOut)===chord)return "zoom-out";
  if(normalizeChord(bindings.zoomReset)===chord)return "zoom-reset";
  if(normalizeChord(bindings.fill)===chord)return "fill";
  return null;
}
function buildChrome(){
  if(document.getElementById("gc-chrome"))return;
  chrome=document.createElement("div");
  chrome.id="gc-chrome";
  pin(chrome,{
    position:"fixed", top:"0", right:"0", bottom:"0", left:"0",
    width:"auto", height:"auto", "z-index":"2147483646",
    "pointer-events":"none", background:"transparent"
  });
  strip=document.createElement("div");
  strip.id="gc-strip";
  var emblem=document.createElement("div");
  pin(emblem,{
    width:"14px", height:"14px", display:"grid",
    "grid-template-columns":"4px 4px", "grid-template-rows":"4px 4px",
    gap:"3px", "pointer-events":"none"
  });
  for(var dot=0; dot<4; dot++){
    var mark=document.createElement("div");
    pin(mark,{
      width:"4px", height:"4px", "border-radius":"50%", background:"#d5d5d5",
      "pointer-events":"none"
    });
    emblem.appendChild(mark);
  }
  strip.appendChild(emblem);
  pluginsButton=chromeButton("Plugins");
  pluginsButton.style.setProperty("margin-left","10px","important");
  pluginsButton.addEventListener("click", function(event){
    eat(event);
    setPlugins(!pluginsOpen);
  }, true);
  settingsButton=chromeButton("Settings", {"margin-left":"auto"});
  settingsButton.addEventListener("click", function(event){
    eat(event);
    setSettings(!settingsOpen);
  }, true);
  strip.appendChild(pluginsButton);
  strip.appendChild(settingsButton);
  chrome.appendChild(strip);
  pluginPanel=document.createElement("div");
  pluginPanel.id="gc-plugins";
  settingsPanel=document.createElement("div");
  settingsPanel.id="gc-settings-root";
  chrome.appendChild(pluginPanel);
  chrome.appendChild(settingsPanel);
  buildSettings();
  rebuildTree();
  var kinds=["north","south","east","west","nw","ne","sw","se"];
  for(var i=0;i<kinds.length;i++){
    var edge=document.createElement("div");
    edge.setAttribute("data-edge", kinds[i]);
    chrome.appendChild(edge);
  }
  document.documentElement.appendChild(chrome);
  placeChrome();
}
function mount(){
  if(!document.documentElement)return;
  if(NATIVE){
    setOverlay(overlayOn);
    if(!window.__gcObs && window.MutationObserver){
      window.__gcObs=new MutationObserver(function(){
        if(overlayOn && !document.getElementById("gc-overlay-style"))setOverlay(true);
      });
      window.__gcObs.observe(document.documentElement,{childList:true,subtree:true});
    }
    if(!window.__gcReady && window.ipc&&window.ipc.postMessage){
      window.__gcReady=true;
      window.ipc.postMessage("ready");
    }
    return;
  }
  if(!document.getElementById("gc-chrome"))buildChrome();
  setOverlay(overlayOn);
  if(window.ipc&&window.ipc.postMessage)window.ipc.postMessage("ready");
  if(!window.__gcObs && window.MutationObserver){
    window.__gcObs=new MutationObserver(function(){
      if(!document.getElementById("gc-settings-root")||!document.getElementById("gc-chrome"))mount();
    });
    window.__gcObs.observe(document.documentElement,{childList:true,subtree:true});
  }
}
window.addEventListener("keydown", function(event){
  if(!recordingId && event.ctrlKey)document.documentElement.style.cursor="grab";
  if(NATIVE)return;
  if(recordingId){
    eat(event);
    if(event.repeat)return;
    var bare=!(event.ctrlKey||event.altKey||event.shiftKey||event.metaKey);
    if(event.code==="Escape"&&bare){
      recordingId=null;
      statusText=hint;
      paintRows();
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
    drafts[recordingId]=parts.join("+");
    recordingId=null;
    statusText="Press Save";
    paintRows();
    return;
  }
  var bareEsc=event.code==="Escape"&&!(event.ctrlKey||event.altKey||event.shiftKey||event.metaKey);
  if(bareEsc){
    if(panelsOpen()){
      eat(event);
      closePanels();
      return;
    }
    post("escape");
    return;
  }
  var typed=mapCode(event.code);
  if(!typed)return;
  var chordParts=[];
  if(event.ctrlKey)chordParts.push("ctrl");
  if(event.altKey)chordParts.push("alt");
  if(event.shiftKey)chordParts.push("shift");
  if(event.metaKey)chordParts.push("super");
  chordParts.push(typed);
  var action=matchedAction(normalizeChord(chordParts.join("+")));
  if(!action)return;
  eat(event);
  post(action);
}, true);
window.addEventListener("pointerdown", function(event){
  if(NATIVE)return;
  if(event.button===3){ eat(event); post("back"); return; }
  if(event.button===4){ eat(event); post("forward"); return; }
  if(event.button!==0||!panelsOpen())return;
  var target=event.target;
  if(pluginPanel&&pluginPanel.contains(target))return;
  if(settingsPanel&&settingsPanel.contains(target))return;
  if(pluginsButton&&pluginsButton.contains(target))return;
  if(settingsButton&&settingsButton.contains(target))return;
  closePanels();
}, true);
window.addEventListener("keyup", function(event){
  if(!event.ctrlKey)document.documentElement.style.cursor="";
}, true);
window.addEventListener("blur", function(){
  document.documentElement.style.cursor="";
});
mount();
document.addEventListener("DOMContentLoaded", mount);
window.addEventListener("load", mount);
})();
"##;

#[cfg(test)]
mod tests {
    use super::script;
    use crate::model::Bindings;

    #[test]
    fn the_settings_panel_lists_shortcuts_and_plugins() {
        let source = script(
            "ctrl+space",
            true,
            true,
            &[],
            "sample",
            &Bindings::default(),
        );
        assert!(source.contains("gc-settings-root"));
        assert!(source.contains("gc-strip"));
        assert!(source.contains("data-edge"));
        assert!(source.contains("\"flex-direction\":\"row\""));
        assert!(source.contains("stopImmediatePropagation"));
        assert!(source.contains("Press Change, then the keys"));
        assert!(source.contains("post(\"overlay\")"));
        assert!(!source.contains("postMessage(\"drag\")"));
        assert!(!source.contains("postMessage(\"resize\")"));
        assert!(!source.contains("textContent=\"Move\""));
        assert!(!source.contains("textContent=\"Size\""));
        assert!(source.contains("rgba(12, 12, 14, 0.78)"));
        assert!(source.contains("background:\"#4a4a4a\""));
        assert!(source.contains("background:\"#242424\""));
        assert!(source.contains("#f2f2f2"));
        assert!(source.contains("\"margin-left\":\"auto\""));
        assert!(source.contains("Plugins"));
        assert!(source.contains("Show or hide"));
        assert!(source.contains("Fill screen"));
        assert!(source.contains("Only Show or hide works while the game is focused."));
        assert!(source.contains("zoom-in"));
        assert!(source.contains("button===3"));
        assert!(source.contains("button===4"));
        assert!(source.contains("\"open:\"+id"));
        assert!(source.contains("alt+left"));
        assert!(!source.contains("settings-open"));
        assert!(!source.contains("barButton(\"Back\""));
        assert!(!source.contains("position:\"fixed\", bottom:\"0\", left:\"auto\""));
        assert!(!source.contains("top:\"10px\""));
        assert!(!source.contains("top:10px"));
    }
}
