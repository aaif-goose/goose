use axum::{
    extract::{Path, Query, Request, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::acp::transport::auth::token_matches;
use crate::config::paths::Paths;
use crate::plugins::formats::goose_native::{read_manifest, WebUiContributionSlot, WebUiPage};
use crate::plugins::is_plugin_enabled;

fn host_bridge_script(base_url: &str, token: &str, plugin_id: &str) -> String {
    let b = base_url.replace('\'', "\\'");
    let t = token.replace('\'', "\\'");
    let p = plugin_id.replace('\'', "\\'");
    format!(
        r#"(function(){{
var B='{b}',T='{t}',P='{p}';
var pnd={{}};
var _state={{}};
var _stateSubs=[];
window.addEventListener('message',function(e){{
  var d=e.data;
  if(!d)return;
  if(d.type==='goose:response'){{
    var x=pnd[d.id];if(!x)return;delete pnd[d.id];
    d.error?x[1](new Error(d.error)):x[0](d.result);
    return;
  }}
  if(d.type==='goose:state-update'&&d.pluginId===P){{
    _state=d.snapshot||{{}};
    _stateSubs.forEach(function(fn){{try{{fn(_state);}}catch(e){{}}}});
  }}
}});
function pmsg(m,a){{return new Promise(function(rs,rj){{
  var id=Math.random().toString(36).slice(2)+Date.now().toString(36);
  pnd[id]=[rs,rj];
  window.parent.postMessage({{type:'goose:request',id:id,pluginId:P,method:m,params:a||{{}}}},'*');
  setTimeout(function(){{if(pnd[id]){{delete pnd[id];rj(new Error('goose:timeout'));}}}},8000);
}})}}
function hf(path,opts){{
  var h=Object.assign({{'X-Secret-Key':T}},opts&&opts.headers||{{}});
  return fetch(B+path,Object.assign({{}},opts,{{headers:h}}));
}}
function lcfg(){{return hf('/plugin-config/'+P).then(function(r){{return r.ok?r.json():{{}}}}).catch(function(){{return{{}}}});}}
window.host={{
  plugin:{{name:P}},
  config:{{
    visible:{{settings:{{}}}},
    get:function(k){{return lcfg().then(function(a){{return a[k]&&a[k].value;}});}},
    set:function(k,v){{var b={{}};b[k]=v;return hf('/plugin-config/'+P,{{method:'PATCH',headers:{{'Content-Type':'application/json'}},body:JSON.stringify(b)}}).then(lcfg).then(function(s){{window.host.config.visible.settings=s;}});}},
    requestMutation:function(c){{return Promise.all(Object.entries(c).map(function(e){{return window.host.config.set(e[0],e[1]);}}))[0];}}
  }},
  notifications:{{
    show:function(m,o){{return pmsg('notifications.show',typeof m==='string'?Object.assign({{message:m}},o||{{}}):m);}}
  }},
  network:{{
    fetchPlugin:function(p,o){{return hf('/plugin-http/'+P+'/'+p.replace(/^\//,''),o||{{}})}},
    json:function(p,o){{return window.host.network.fetchPlugin(p,o).then(function(r){{return r.json()}})}}
  }},
  appearance:{{
    getTokens:function(){{return pmsg('appearance.getTokens',{{}})}},
    subscribe:function(fn){{
      var mq=window.matchMedia('(prefers-color-scheme: dark)');
      var run=function(){{window.host.appearance.getTokens().then(fn).catch(function(){{}});}};
      mq.addEventListener('change',run);
      return run;
    }}
  }},
  navigation:{{
    navigateTo:function(path){{return pmsg('navigation.navigateTo',{{path:path}})}},
    openPluginPage:function(pageId){{return pmsg('navigation.openPluginPage',{{pageId:pageId}})}}
  }},
  state:{{
    getSnapshot:function(){{return pmsg('state.getSnapshot',{{}})}},
    update:function(patch){{return pmsg('state.update',{{patch:patch}})}},
    subscribe:function(fn){{
      _stateSubs.push(fn);
      return function(){{_stateSubs=_stateSubs.filter(function(f){{return f!==fn;}});}};
    }}
  }}
}};
Promise.all([lcfg(),pmsg('appearance.getTokens',{{}})]).then(function(r){{
  window.host.config.visible.settings=r[0];
  window.host.appearance.tokens=r[1];
  window.dispatchEvent(new CustomEvent('host:ready',{{detail:{{config:r[0],appearance:r[1]}}}}));
}}).catch(function(){{window.dispatchEvent(new Event('host:ready'));}});
}})();"#,
        b = b,
        t = t,
        p = p
    )
}

fn inject_bridge(html: &[u8], script: &str) -> Vec<u8> {
    let src = match std::str::from_utf8(html) {
        Ok(s) => s,
        Err(_) => return html.to_vec(),
    };
    let tag = format!("<script>{}</script>", script);
    let injected = if let Some(pos) = src.find("</head>") {
        format!("{}{}{}", &src[..pos], tag, &src[pos..])
    } else if let Some(pos) = src.find("<body") {
        let end = src[pos..].find('>').map(|i| pos + i + 1).unwrap_or(pos);
        format!("{}{}{}", &src[..end], tag, &src[end..])
    } else {
        format!("{}{}", tag, src)
    };
    injected.into_bytes()
}

#[derive(Debug, Serialize)]
pub struct PluginUiPageInfo {
    pub id: String,
    pub label: String,
    pub main: String,
}

#[derive(Debug, Serialize)]
pub struct PluginUiContributionSlotInfo {
    pub id: String,
    pub main: String,
}

#[derive(Debug, Serialize)]
pub struct PluginUiEntry {
    pub plugin_id: String,
    pub plugin_name: String,
    pub pages: Vec<PluginUiPageInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_suffix: Option<PluginUiContributionSlotInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_section: Option<PluginUiContributionSlotInfo>,
}

#[derive(Clone)]
struct PluginUiState {
    secret_key: String,
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

fn check_auth(
    secret_key: &str,
    headers: &axum::http::HeaderMap,
    query_token: Option<&str>,
) -> bool {
    let header_token = headers.get("X-Secret-Key").and_then(|v| v.to_str().ok());
    token_matches(header_token, secret_key) || token_matches(query_token, secret_key)
}

fn mime_from_path(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "application/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

async fn list_plugin_ui(
    State(state): State<PluginUiState>,
    Query(query): Query<TokenQuery>,
    req: Request,
) -> Result<Json<Vec<PluginUiEntry>>, StatusCode> {
    if !check_auth(&state.secret_key, req.headers(), query.token.as_deref()) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let plugins_dir = Paths::plugins_dir();
    let Ok(entries) = std::fs::read_dir(&plugins_dir) else {
        return Ok(Json(vec![]));
    };

    let mut result = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(Some(manifest)) = read_manifest(&dir) else {
            continue;
        };
        let Some(web_ui) = manifest.web_ui else {
            continue;
        };
        if web_ui.pages.is_empty()
            && web_ui.chat_suffix.is_none()
            && web_ui.config_section.is_none()
        {
            continue;
        }
        if !is_plugin_enabled(&dir) {
            continue;
        }
        let chat_suffix =
            web_ui
                .chat_suffix
                .map(|s: WebUiContributionSlot| PluginUiContributionSlotInfo {
                    id: s.id,
                    main: s.main,
                });
        let config_section =
            web_ui
                .config_section
                .map(|s: WebUiContributionSlot| PluginUiContributionSlotInfo {
                    id: s.id,
                    main: s.main,
                });
        result.push(PluginUiEntry {
            plugin_id: manifest.id.clone(),
            plugin_name: manifest.name.clone(),
            pages: web_ui
                .pages
                .into_iter()
                .map(|p: WebUiPage| PluginUiPageInfo {
                    id: p.id,
                    label: p.label,
                    main: p.main,
                })
                .collect(),
            chat_suffix,
            config_section,
        });
    }

    Ok(Json(result))
}

async fn serve_plugin_ui_file(
    State(state): State<PluginUiState>,
    Path((plugin_id, file_path)): Path<(String, String)>,
    Query(query): Query<TokenQuery>,
    req: Request,
) -> Result<Response, StatusCode> {
    let base_url = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|h| format!("http://{}", h))
        .unwrap_or_else(|| "http://127.0.0.1".to_string());

    if !check_auth(&state.secret_key, req.headers(), query.token.as_deref()) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    if plugin_id.contains("..") || file_path.contains("..") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let plugins_dir = Paths::plugins_dir();
    let plugin_dir = plugins_dir.join(&plugin_id);
    if !plugin_dir.is_dir() {
        return Err(StatusCode::NOT_FOUND);
    }

    let Ok(Some(manifest)) = read_manifest(&plugin_dir) else {
        return Err(StatusCode::NOT_FOUND);
    };
    if manifest.web_ui.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    if !is_plugin_enabled(&plugin_dir) {
        return Err(StatusCode::FORBIDDEN);
    }

    let safe_path = file_path.trim_start_matches('/');
    let file = plugin_dir.join("web_ui").join(safe_path);
    let canonical = file.canonicalize().map_err(|_| StatusCode::NOT_FOUND)?;
    let ui_root = plugin_dir
        .join("web_ui")
        .canonicalize()
        .map_err(|_| StatusCode::NOT_FOUND)?;
    if !canonical.starts_with(&ui_root) {
        return Err(StatusCode::FORBIDDEN);
    }

    let bytes = std::fs::read(&canonical).map_err(|_| StatusCode::NOT_FOUND)?;
    let mime = mime_from_path(&canonical);

    if mime == "text/html; charset=utf-8" {
        let script = host_bridge_script(&base_url, &state.secret_key, &manifest.id);
        let injected = inject_bridge(&bytes, &script);
        return Ok(([(header::CONTENT_TYPE, mime)], injected).into_response());
    }

    Ok(([(header::CONTENT_TYPE, mime)], bytes).into_response())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallPluginLocalRequest {
    source_path: String,
}

#[derive(Serialize)]
struct InstallPluginLocalResponse {
    id: String,
}

async fn install_plugin_local(
    State(state): State<PluginUiState>,
    Query(query): Query<TokenQuery>,
    headers: HeaderMap,
    Json(body): Json<InstallPluginLocalRequest>,
) -> impl IntoResponse {
    if !check_auth(&state.secret_key, &headers, query.token.as_deref()) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match crate::plugins::install_goose_plugin_from_local(std::path::Path::new(&body.source_path)) {
        Ok(install) => Json(InstallPluginLocalResponse { id: install.name }).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": format!("{e:#}") })),
        )
            .into_response(),
    }
}

pub fn routes(secret_key: String) -> Router {
    let state = PluginUiState { secret_key };
    Router::new()
        .route("/plugin-ui", get(list_plugin_ui))
        .route(
            "/plugin-ui/{plugin_id}/{*file_path}",
            get(serve_plugin_ui_file),
        )
        .route("/plugin-install", post(install_plugin_local))
        .with_state(state)
}
