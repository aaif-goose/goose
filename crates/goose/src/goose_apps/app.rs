use crate::agents::ExtensionManager;
use rmcp::model::ErrorData;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::resource::McpAppResource;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowProps {
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GooseApp {
    #[serde(flatten)]
    pub resource: McpAppResource,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<String>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub window_props: Option<WindowProps>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prd: Option<String>,
    /// Whether this app can be deleted by the user (i.e. not a bundled default).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deletable: bool,
}

impl GooseApp {
    const METADATA_SCRIPT_TYPE: &'static str = "application/ld+json";
    const PRD_SCRIPT_TYPE: &'static str = "application/x-goose-prd";
    const GOOSE_APP_TYPE: &'static str = "GooseApp";
    const GOOSE_SCHEMA_CONTEXT: &'static str = "urn:goose.ai:schema";

    pub fn from_html(html: &str) -> Result<Self, String> {
        use regex::Regex;

        let metadata_re = Regex::new(&format!(
            r#"(?s)<script type="{}"[^>]*>\s*(.*?)\s*</script>"#,
            regex::escape(Self::METADATA_SCRIPT_TYPE)
        ))
        .map_err(|e| format!("Regex error: {}", e))?;

        let prd_re = Regex::new(&format!(
            r#"(?s)<script type="{}"[^>]*>\s*(.*?)\s*</script>"#,
            regex::escape(Self::PRD_SCRIPT_TYPE)
        ))
        .map_err(|e| format!("Regex error: {}", e))?;

        let json_str = metadata_re
            .captures(html)
            .and_then(|cap| cap.get(1))
            .ok_or_else(|| "No GooseApp JSON-LD metadata found in HTML".to_string())?
            .as_str();

        let metadata: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| format!("Failed to parse metadata JSON: {}", e))?;

        let name = metadata
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'name' in metadata")?
            .to_string();

        let description = metadata
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from);

        let width = metadata
            .get("width")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        let height = metadata
            .get("height")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        let resizable = metadata.get("resizable").and_then(|v| v.as_bool());

        let window_props = if width.is_some() || height.is_some() || resizable.is_some() {
            Some(WindowProps {
                width: width.unwrap_or(800),
                height: height.unwrap_or(600),
                resizable: resizable.unwrap_or(true),
            })
        } else {
            None
        };

        let mcp_servers = metadata
            .get("mcpServers")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let prd = prd_re
            .captures(html)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().trim().to_string());

        let clean_html = metadata_re.replace(html, "");
        let clean_html = prd_re.replace(&clean_html, "").to_string();

        Ok(GooseApp {
            resource: McpAppResource {
                uri: format!("ui://apps/{}", name),
                name,
                description,
                mime_type: "text/html;profile=mcp-app".to_string(),
                text: Some(clean_html),
                blob: None,
                meta: None,
            },
            mcp_servers,
            window_props,
            prd,
            deletable: false,
        })
    }

    pub fn to_html(&self) -> Result<String, String> {
        let html = self
            .resource
            .text
            .as_ref()
            .ok_or("App has no HTML content")?;

        let mut metadata = serde_json::json!({
            "@context": Self::GOOSE_SCHEMA_CONTEXT,
            "@type": Self::GOOSE_APP_TYPE,
            "name": self.resource.name,
        });

        if let Some(ref desc) = self.resource.description {
            metadata["description"] = serde_json::json!(desc);
        }

        if let Some(ref props) = self.window_props {
            metadata["width"] = serde_json::json!(props.width);
            metadata["height"] = serde_json::json!(props.height);
            metadata["resizable"] = serde_json::json!(props.resizable);
        }

        if !self.mcp_servers.is_empty() {
            metadata["mcpServers"] = serde_json::json!(self.mcp_servers);
        }

        let metadata_json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| format!("Failed to serialize metadata: {}", e))?;

        let metadata_script = format!(
            "  <script type=\"{}\">\n{}\n  </script>",
            Self::METADATA_SCRIPT_TYPE,
            metadata_json
        );

        let prd_script = if let Some(ref prd) = self.prd {
            if !prd.is_empty() {
                format!(
                    "  <script type=\"{}\">\n{}\n  </script>",
                    Self::PRD_SCRIPT_TYPE,
                    prd
                )
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let scripts = if prd_script.is_empty() {
            format!("{}\n", metadata_script)
        } else {
            format!("{}\n{}\n", metadata_script, prd_script)
        };

        let result = if let Some(head_pos) = html.find("</head>") {
            let mut result = html.clone();
            result.insert_str(head_pos, &scripts);
            result
        } else if let Some(html_pos) = html.find("<html") {
            let after_html = html
                .get(html_pos..)
                .and_then(|s| s.find('>'))
                .map(|p| html_pos + p + 1);
            if let Some(pos) = after_html {
                let mut result = html.clone();
                result.insert_str(pos, &format!("\n<head>\n{}</head>", scripts));
                result
            } else {
                format!("<head>\n{}</head>\n{}", scripts, html)
            }
        } else {
            format!(
                "<html>\n<head>\n{}</head>\n<body>\n{}\n</body>\n</html>",
                scripts, html
            )
        };

        Ok(result)
    }
}

pub async fn fetch_mcp_apps(
    extension_manager: &ExtensionManager,
    session_id: &str,
) -> Result<Vec<GooseApp>, ErrorData> {
    let mut apps = Vec::new();

    let ui_resources = extension_manager.get_ui_resources(session_id).await?;

    for (extension_name, resource) in ui_resources {
        match extension_manager
            .read_resource(
                session_id,
                &resource.uri,
                &extension_name,
                CancellationToken::default(),
            )
            .await
        {
            Ok(read_result) => {
                let mut html = String::new();
                for content in read_result.contents {
                    if let rmcp::model::ResourceContents::TextResourceContents { text, .. } =
                        content
                    {
                        html = text;
                        break;
                    }
                }

                if !html.is_empty() {
                    let mcp_resource = McpAppResource {
                        uri: resource.uri.clone(),
                        name: resource.name.clone(),
                        description: resource.description.clone(),
                        mime_type: "text/html;profile=mcp-app".to_string(),
                        text: Some(html),
                        blob: None,
                        meta: None,
                    };

                    let window_props = if let Some(ref meta) = resource.meta {
                        if let Some(window_obj) = meta.get("window").and_then(|v| v.as_object()) {
                            if let (Some(width), Some(height), Some(resizable)) = (
                                window_obj
                                    .get("width")
                                    .and_then(|v| v.as_u64())
                                    .map(|v| v as u32),
                                window_obj
                                    .get("height")
                                    .and_then(|v| v.as_u64())
                                    .map(|v| v as u32),
                                window_obj.get("resizable").and_then(|v| v.as_bool()),
                            ) {
                                Some(WindowProps {
                                    width,
                                    height,
                                    resizable,
                                })
                            } else {
                                Some(WindowProps {
                                    width: 800,
                                    height: 600,
                                    resizable: true,
                                })
                            }
                        } else {
                            Some(WindowProps {
                                width: 800,
                                height: 600,
                                resizable: true,
                            })
                        }
                    } else {
                        Some(WindowProps {
                            width: 800,
                            height: 600,
                            resizable: true,
                        })
                    };

                    let app = GooseApp {
                        resource: mcp_resource,
                        mcp_servers: vec![extension_name],
                        window_props,
                        prd: None,
                        deletable: false,
                    };

                    apps.push(app);
                }
            }
            Err(e) => {
                warn!(
                    "Failed to read resource {} from {}: {}",
                    resource.uri, extension_name, e
                );
            }
        }
    }

    Ok(apps)
}
