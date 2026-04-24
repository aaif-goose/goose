use etcetera::{choose_app_strategy, AppStrategy};
use indoc::formatdoc;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, ErrorCode, ErrorData, Implementation, InitializeResult,
        ListResourcesResult, Meta, PaginatedRequestParams, RawResource, ReadResourceRequestParams,
        ReadResourceResult, Resource, ResourceContents, ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

/// MIME type for MCP Apps (SEP-1865)
const MCP_APPS_MIME_TYPE: &str = "text/html;profile=mcp-app";

/// Build a Meta object with `_meta.ui.resourceUri` for linking a tool to a UI resource.
fn ui_resource_meta(uri: &str) -> Meta {
    let mut meta = Meta::new();
    meta.0
        .insert("ui".to_string(), json!({ "resourceUri": uri }));
    meta
}

/// Struct representing the UI resource definitions for autovisualiser chart types.
struct UIResourceDef {
    uri: &'static str,
    name: &'static str,
    description: &'static str,
}

const UI_RESOURCES: &[UIResourceDef] = &[
    UIResourceDef {
        uri: "ui://autovisualiser/chart",
        name: "Chart",
        description: "Interactive line, bar, and scatter chart visualization",
    },
    UIResourceDef {
        uri: "ui://autovisualiser/sankey",
        name: "Sankey Diagram",
        description: "Flow diagram showing relationships between nodes",
    },
    UIResourceDef {
        uri: "ui://autovisualiser/radar",
        name: "Radar Chart",
        description: "Multi-dimensional data comparison spider chart",
    },
    UIResourceDef {
        uri: "ui://autovisualiser/donut",
        name: "Donut/Pie Chart",
        description: "Categorical data visualization as donut or pie chart",
    },
    UIResourceDef {
        uri: "ui://autovisualiser/treemap",
        name: "Treemap",
        description: "Hierarchical data visualization with proportional areas",
    },
    UIResourceDef {
        uri: "ui://autovisualiser/chord",
        name: "Chord Diagram",
        description: "Relationship and flow visualization between entities",
    },
    UIResourceDef {
        uri: "ui://autovisualiser/map",
        name: "Interactive Map",
        description: "Geographic data visualization with location markers",
    },
    UIResourceDef {
        uri: "ui://autovisualiser/mermaid",
        name: "Mermaid Diagram",
        description: "Diagram visualization from Mermaid syntax",
    },
];

/// Validates that the data parameter is a proper JSON value and not a string
fn validate_data_param(params: &Value, allow_array: bool) -> Result<Value, ErrorData> {
    let data_value = params.get("data").ok_or_else(|| {
        ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            "Missing 'data' parameter".to_string(),
            None,
        )
    })?;

    if data_value.is_string() {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            "The 'data' parameter must be a JSON object, not a JSON string. Please provide valid JSON without comments.".to_string(),
            None,
        ));
    }

    if allow_array {
        if !data_value.is_object() && !data_value.is_array() {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "The 'data' parameter must be a JSON object or array.".to_string(),
                None,
            ));
        }
    } else if !data_value.is_object() {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            "The 'data' parameter must be a JSON object.".to_string(),
            None,
        ));
    }

    Ok(data_value.clone())
}

fn validation_err(msg: impl Into<String>) -> ErrorData {
    ErrorData::new(ErrorCode::INVALID_PARAMS, msg.into(), None)
}

/// Sankey node structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SankeyNode {
    /// The name of the node
    pub name: String,
    /// Optional category for the node
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// Sankey link structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SankeyLink {
    /// Source node name
    pub source: String,
    /// Target node name
    pub target: String,
    /// Flow value
    pub value: f64,
}

/// Sankey data structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SankeyData {
    /// Array of nodes
    pub nodes: Vec<SankeyNode>,
    /// Array of links between nodes
    pub links: Vec<SankeyLink>,
}

impl SankeyData {
    fn validate(&self) -> Result<(), ErrorData> {
        if self.nodes.is_empty() {
            return Err(validation_err("nodes array must not be empty"));
        }
        if self.links.is_empty() {
            return Err(validation_err("links array must not be empty"));
        }
        let names: std::collections::HashSet<&str> =
            self.nodes.iter().map(|n| n.name.as_str()).collect();
        for link in &self.links {
            if !names.contains(link.source.as_str()) {
                return Err(validation_err(format!(
                    "link source '{}' not found in nodes",
                    link.source
                )));
            }
            if !names.contains(link.target.as_str()) {
                return Err(validation_err(format!(
                    "link target '{}' not found in nodes",
                    link.target
                )));
            }
            if link.value <= 0.0 {
                return Err(validation_err(format!(
                    "link value must be positive, got {} for '{}' → '{}'",
                    link.value, link.source, link.target
                )));
            }
        }
        Ok(())
    }
}

/// Parameters for render_sankey tool
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderSankeyParams {
    /// The data for the Sankey diagram
    pub data: SankeyData,
}

/// Radar dataset structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RadarDataset {
    /// Label for this dataset
    pub label: String,
    /// Data values for each category
    pub data: Vec<f64>,
}

/// Radar chart data structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RadarData {
    /// Category labels
    pub labels: Vec<String>,
    /// Datasets to compare
    pub datasets: Vec<RadarDataset>,
}

impl RadarData {
    fn validate(&self) -> Result<(), ErrorData> {
        if self.labels.is_empty() {
            return Err(validation_err("labels array must not be empty"));
        }
        if self.datasets.is_empty() {
            return Err(validation_err("datasets array must not be empty"));
        }
        let expected = self.labels.len();
        for ds in &self.datasets {
            if ds.data.len() != expected {
                return Err(validation_err(format!(
                    "dataset '{}' has {} values but there are {} labels",
                    ds.label,
                    ds.data.len(),
                    expected
                )));
            }
        }
        Ok(())
    }
}

/// Parameters for render_radar tool
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderRadarParams {
    /// The data for the radar chart
    pub data: RadarData,
}

/// Data item for donut/pie charts - can be a number or labeled value
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
#[serde(untagged)]
pub enum DonutDataItem {
    /// Simple numeric value
    Number(f64),
    /// Labeled value with explicit label
    LabeledValue {
        /// Label for this data point
        label: String,
        /// Numeric value
        value: f64,
    },
}

/// Chart type for donut/pie charts
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DonutChartType {
    /// Doughnut chart (with hole in center)
    Doughnut,
    /// Pie chart (no hole)
    Pie,
}

/// Single donut/pie chart data
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct SingleDonutChart {
    /// Array of values — numbers (e.g. [10, 20]) or objects (e.g. [{"label": "A", "value": 10}])
    pub values: Vec<DonutDataItem>,
    /// Optional chart title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional chart type (doughnut or pie)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub chart_type: Option<DonutChartType>,
    /// Optional labels array (used when values are just numbers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
}

impl SingleDonutChart {
    fn validate(&self) -> Result<(), ErrorData> {
        if self.values.is_empty() {
            return Err(validation_err("values array must not be empty"));
        }
        if let Some(labels) = &self.labels {
            if labels.len() != self.values.len() {
                return Err(validation_err(format!(
                    "labels array length ({}) must match values array length ({})",
                    labels.len(),
                    self.values.len()
                )));
            }
        }
        Ok(())
    }
}

fn validate_donut_charts(charts: &[SingleDonutChart]) -> Result<(), ErrorData> {
    if charts.is_empty() {
        return Err(validation_err("charts array must not be empty"));
    }
    for chart in charts {
        chart.validate()?;
    }
    Ok(())
}

/// Parameters for render_donut tool
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderDonutParams {
    /// The chart data as an array of chart objects. Use a single-element array for one chart.
    pub data: Vec<SingleDonutChart>,
}

/// Treemap node structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct TreemapNode {
    /// Name of the node
    pub name: String,
    /// Value for leaf nodes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// Category for coloring
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Children nodes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<TreemapNode>>,
}

impl TreemapNode {
    fn validate(&self) -> Result<(), ErrorData> {
        // Must have either a value or children
        if self.value.is_none() && self.children.as_ref().is_none_or(|c| c.is_empty()) {
            return Err(validation_err(format!(
                "node '{}' must have either a value or non-empty children",
                self.name
            )));
        }
        if let Some(children) = &self.children {
            for child in children {
                child.validate()?;
            }
        }
        Ok(())
    }
}

/// Parameters for render_treemap tool
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderTreemapParams {
    /// The hierarchical data for the treemap
    pub data: TreemapNode,
}

/// Chord diagram data structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ChordData {
    /// Labels for each entity
    pub labels: Vec<String>,
    /// 2D matrix of flows (matrix[i][j] = flow from i to j)
    pub matrix: Vec<Vec<f64>>,
}

impl ChordData {
    fn validate(&self) -> Result<(), ErrorData> {
        if self.labels.is_empty() {
            return Err(validation_err("labels array must not be empty"));
        }
        let n = self.labels.len();
        if self.matrix.len() != n {
            return Err(validation_err(format!(
                "matrix has {} rows but there are {} labels",
                self.matrix.len(),
                n
            )));
        }
        for (i, row) in self.matrix.iter().enumerate() {
            if row.len() != n {
                return Err(validation_err(format!(
                    "matrix row {} has {} columns but expected {}",
                    i,
                    row.len(),
                    n
                )));
            }
        }
        Ok(())
    }
}

/// Parameters for render_chord tool
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderChordParams {
    /// The data for the chord diagram
    pub data: ChordData,
}

/// Map marker structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MapMarker {
    /// Latitude (required)
    pub lat: f64,
    /// Longitude (required)
    pub lng: f64,
    /// Location name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Numeric value for sizing/coloring
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// Description text
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Custom popup HTML
    #[serde(skip_serializing_if = "Option::is_none")]
    pub popup: Option<String>,
    /// Custom marker color
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Custom marker label
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Use default Leaflet icon
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "useDefaultIcon")]
    pub use_default_icon: Option<bool>,
}

/// Map center point
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MapCenter {
    /// Latitude
    pub lat: f64,
    /// Longitude
    pub lng: f64,
}

/// Map data structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct MapData {
    /// Array of markers
    pub markers: Vec<MapMarker>,
    /// Optional title for the map
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional subtitle
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Optional center point
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center: Option<MapCenter>,
    /// Optional initial zoom level
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom: Option<f64>,
    /// Optional boolean to enable/disable clustering
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clustering: Option<bool>,
    /// Optional cluster radius
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "clusterRadius")]
    pub cluster_radius: Option<f64>,
    /// Optional boolean to auto-fit map to markers
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "autoFit")]
    pub auto_fit: Option<bool>,
}

impl MapData {
    fn validate(&self) -> Result<(), ErrorData> {
        if self.markers.is_empty() {
            return Err(validation_err("markers array must not be empty"));
        }
        for (i, m) in self.markers.iter().enumerate() {
            if !(-90.0..=90.0).contains(&m.lat) {
                return Err(validation_err(format!(
                    "marker {} has invalid latitude {} (must be -90 to 90)",
                    i, m.lat
                )));
            }
            if !(-180.0..=180.0).contains(&m.lng) {
                return Err(validation_err(format!(
                    "marker {} has invalid longitude {} (must be -180 to 180)",
                    i, m.lng
                )));
            }
        }
        Ok(())
    }
}

/// Parameters for render_map tool
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderMapParams {
    /// The data for the map visualization
    pub data: MapData,
}

/// Chart data point for scatter charts
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ChartPoint {
    /// X coordinate
    pub x: f64,
    /// Y coordinate
    pub y: f64,
}

/// Chart dataset structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ChartDataset {
    /// Label for this dataset
    pub label: String,
    /// Data points - can be numbers or x/y points
    pub data: ChartDataValues,
    /// Optional background color for the dataset
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "backgroundColor")]
    pub background_color: Option<String>,
    /// Optional border color for the dataset
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "borderColor")]
    pub border_color: Option<String>,
    /// Optional border width for the dataset
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "borderWidth")]
    pub border_width: Option<f64>,
    /// Optional tension for line curves (0 = straight lines, higher = more curved)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tension: Option<f64>,
    /// Optional fill setting for area under the line
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<bool>,
}

/// Chart data values - can be simple numbers or x/y points
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
#[serde(untagged)]
pub enum ChartDataValues {
    /// Simple numeric values (for line/bar charts with labels)
    Numbers(Vec<f64>),
    /// X/Y points (for scatter charts or line charts without labels)
    Points(Vec<ChartPoint>),
}

/// Chart type enumeration
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChartType {
    /// Line chart
    Line,
    /// Scatter chart
    Scatter,
    /// Bar chart
    Bar,
}

/// Chart data structure
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ChartData {
    /// Chart type
    #[serde(rename = "type")]
    pub chart_type: ChartType,
    /// Datasets to display
    pub datasets: Vec<ChartDataset>,
    /// Optional labels for x-axis (for line/bar charts)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    /// Optional chart title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional subtitle
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Optional x-axis label
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "xAxisLabel")]
    pub x_axis_label: Option<String>,
    /// Optional y-axis label
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "yAxisLabel")]
    pub y_axis_label: Option<String>,
}

impl ChartData {
    fn validate(&self) -> Result<(), ErrorData> {
        if self.datasets.is_empty() {
            return Err(validation_err("datasets array must not be empty"));
        }
        if let Some(labels) = &self.labels {
            for ds in &self.datasets {
                if let ChartDataValues::Numbers(nums) = &ds.data {
                    if nums.len() != labels.len() {
                        return Err(validation_err(format!(
                            "dataset '{}' has {} values but there are {} labels",
                            ds.label,
                            nums.len(),
                            labels.len()
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

/// Parameters for show_chart tool
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct ShowChartParams {
    /// The data for the chart
    pub data: ChartData,
}

/// Parameters for render_mermaid tool
#[derive(Debug, Serialize, Deserialize, rmcp::schemars::JsonSchema)]
pub struct RenderMermaidParams {
    /// The Mermaid diagram code to render
    pub mermaid_code: String,
}

/// An extension for automatic data visualization and UI generation
#[derive(Clone)]
pub struct AutoVisualiserRouter {
    tool_router: ToolRouter<Self>,
    #[allow(dead_code)]
    cache_dir: PathBuf,
    instructions: String,
}

impl Default for AutoVisualiserRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for AutoVisualiserRouter {
    fn get_info(&self) -> ServerInfo {
        InitializeResult::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new(
            "goose-autovisualiser",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(self.instructions.clone())
    }

    async fn list_resources(
        &self,
        _pagination: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let resources = UI_RESOURCES
            .iter()
            .map(|def| Resource {
                raw: RawResource {
                    uri: def.uri.to_string(),
                    name: def.name.to_string(),
                    title: Some(def.name.to_string()),
                    description: Some(def.description.to_string()),
                    mime_type: Some(MCP_APPS_MIME_TYPE.to_string()),
                    size: None,
                    icons: None,
                    meta: None,
                },
                annotations: None,
            })
            .collect();

        Ok(ListResourcesResult {
            resources,
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        params: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        let html = self.get_template_html(&params.uri)?;

        let mut meta = Meta::new();
        meta.0
            .insert("ui".to_string(), json!({ "prefersBorder": true }));

        let resource_contents = ResourceContents::TextResourceContents {
            uri: params.uri,
            mime_type: Some(MCP_APPS_MIME_TYPE.to_string()),
            text: html,
            meta: Some(meta),
        };

        Ok(ReadResourceResult::new(vec![resource_contents]))
    }
}

#[tool_router(router = tool_router)]
impl AutoVisualiserRouter {
    pub fn new() -> Self {
        // choose_app_strategy().cache_dir()
        // - macOS/Linux: ~/.cache/goose/autovisualiser/
        // - Windows:     ~\AppData\Local\Block\goose\cache\autovisualiser\
        let cache_dir = choose_app_strategy(crate::APP_STRATEGY.clone())
            .unwrap()
            .cache_dir()
            .join("autovisualiser");

        // Create cache directory if it doesn't exist
        let _ = std::fs::create_dir_all(&cache_dir);

        let instructions = formatdoc! {r#"
            This extension provides tools for automatic data visualization
            Use these tools when you are presenting data to the user which could be complemented by a visual expression
            Choose the most appropriate chart type based on the data you have and can provide
            It is important you match the data format as appropriate with the chart type you have chosen
            The user may specify a type of chart or you can pick one of the most appropriate that you can shape the data to

            ## Available Tools:
            - **render_sankey**: Creates interactive Sankey diagrams from flow data
            - **render_radar**: Creates interactive radar charts for multi-dimensional data comparison
            - **render_donut**: Creates interactive donut/pie charts for categorical data (supports multiple charts)
            - **render_treemap**: Creates interactive treemap visualizations for hierarchical data
            - **render_chord**: Creates interactive chord diagrams for relationship/flow visualization
            - **render_map**: Creates interactive map visualizations with location markers
            - **render_mermaid**: Creates interactive Mermaid diagrams from Mermaid syntax
            - **show_chart**: Creates interactive line, scatter, or bar charts for data visualization
        "#};

        Self {
            tool_router: Self::tool_router(),
            cache_dir,
            instructions,
        }
    }

    /// Get the static HTML template for a given `ui://` resource URI.
    /// Templates have JS libs inlined but NO data baked in — data arrives via postMessage.
    fn get_template_html(&self, uri: &str) -> Result<String, ErrorData> {
        match uri {
            "ui://autovisualiser/chart" => {
                const TEMPLATE: &str = include_str!("templates/chart_template.html");
                const CHART_MIN: &str = include_str!("templates/assets/chart.min.js");
                const BASE_CSS: &str = include_str!("templates/assets/mcp-app-base.css");
                const BRIDGE_JS: &str = include_str!("templates/assets/mcp-app-bridge.js");
                Ok(TEMPLATE
                    .replace("{{CHART_MIN}}", CHART_MIN)
                    .replace("{{MCP_APP_BASE_CSS}}", BASE_CSS)
                    .replace("{{MCP_APP_BRIDGE}}", BRIDGE_JS))
            }
            "ui://autovisualiser/sankey" => {
                const TEMPLATE: &str = include_str!("templates/sankey_template.html");
                const D3_MIN: &str = include_str!("templates/assets/d3.min.js");
                const D3_SANKEY: &str = include_str!("templates/assets/d3.sankey.min.js");
                const BASE_CSS: &str = include_str!("templates/assets/mcp-app-base.css");
                const BRIDGE_JS: &str = include_str!("templates/assets/mcp-app-bridge.js");
                Ok(TEMPLATE
                    .replace("{{D3_MIN}}", D3_MIN)
                    .replace("{{D3_SANKY}}", D3_SANKEY)
                    .replace("{{MCP_APP_BASE_CSS}}", BASE_CSS)
                    .replace("{{MCP_APP_BRIDGE}}", BRIDGE_JS))
            }
            "ui://autovisualiser/radar" => {
                const TEMPLATE: &str = include_str!("templates/radar_template.html");
                const CHART_MIN: &str = include_str!("templates/assets/chart.min.js");
                const BASE_CSS: &str = include_str!("templates/assets/mcp-app-base.css");
                const BRIDGE_JS: &str = include_str!("templates/assets/mcp-app-bridge.js");
                Ok(TEMPLATE
                    .replace("{{CHART_MIN}}", CHART_MIN)
                    .replace("{{MCP_APP_BASE_CSS}}", BASE_CSS)
                    .replace("{{MCP_APP_BRIDGE}}", BRIDGE_JS))
            }
            "ui://autovisualiser/donut" => {
                const TEMPLATE: &str = include_str!("templates/donut_template.html");
                const CHART_MIN: &str = include_str!("templates/assets/chart.min.js");
                const BASE_CSS: &str = include_str!("templates/assets/mcp-app-base.css");
                const BRIDGE_JS: &str = include_str!("templates/assets/mcp-app-bridge.js");
                Ok(TEMPLATE
                    .replace("{{CHART_MIN}}", CHART_MIN)
                    .replace("{{MCP_APP_BASE_CSS}}", BASE_CSS)
                    .replace("{{MCP_APP_BRIDGE}}", BRIDGE_JS))
            }
            "ui://autovisualiser/treemap" => {
                const TEMPLATE: &str = include_str!("templates/treemap_template.html");
                const D3_MIN: &str = include_str!("templates/assets/d3.min.js");
                const BASE_CSS: &str = include_str!("templates/assets/mcp-app-base.css");
                const BRIDGE_JS: &str = include_str!("templates/assets/mcp-app-bridge.js");
                Ok(TEMPLATE
                    .replace("{{D3_MIN}}", D3_MIN)
                    .replace("{{MCP_APP_BASE_CSS}}", BASE_CSS)
                    .replace("{{MCP_APP_BRIDGE}}", BRIDGE_JS))
            }
            "ui://autovisualiser/chord" => {
                const TEMPLATE: &str = include_str!("templates/chord_template.html");
                const D3_MIN: &str = include_str!("templates/assets/d3.min.js");
                const BASE_CSS: &str = include_str!("templates/assets/mcp-app-base.css");
                const BRIDGE_JS: &str = include_str!("templates/assets/mcp-app-bridge.js");
                Ok(TEMPLATE
                    .replace("{{D3_MIN}}", D3_MIN)
                    .replace("{{MCP_APP_BASE_CSS}}", BASE_CSS)
                    .replace("{{MCP_APP_BRIDGE}}", BRIDGE_JS))
            }
            "ui://autovisualiser/map" => {
                const TEMPLATE: &str = include_str!("templates/map_template.html");
                const LEAFLET_JS: &str = include_str!("templates/assets/leaflet.min.js");
                const LEAFLET_CSS: &str = include_str!("templates/assets/leaflet.min.css");
                const MARKERCLUSTER_JS: &str =
                    include_str!("templates/assets/leaflet.markercluster.min.js");
                const BASE_CSS: &str = include_str!("templates/assets/mcp-app-base.css");
                const BRIDGE_JS: &str = include_str!("templates/assets/mcp-app-bridge.js");
                Ok(TEMPLATE
                    .replace("{{LEAFLET_JS}}", LEAFLET_JS)
                    .replace("{{LEAFLET_CSS}}", LEAFLET_CSS)
                    .replace("{{MARKERCLUSTER_JS}}", MARKERCLUSTER_JS)
                    .replace("{{MCP_APP_BASE_CSS}}", BASE_CSS)
                    .replace("{{MCP_APP_BRIDGE}}", BRIDGE_JS))
            }
            "ui://autovisualiser/mermaid" => {
                const TEMPLATE: &str = include_str!("templates/mermaid_template.html");
                const MERMAID_MIN: &str = include_str!("templates/assets/mermaid.min.js");
                const BASE_CSS: &str = include_str!("templates/assets/mcp-app-base.css");
                const BRIDGE_JS: &str = include_str!("templates/assets/mcp-app-bridge.js");
                Ok(TEMPLATE
                    .replace("{{MERMAID_MIN}}", MERMAID_MIN)
                    .replace("{{MCP_APP_BASE_CSS}}", BASE_CSS)
                    .replace("{{MCP_APP_BRIDGE}}", BRIDGE_JS))
            }
            _ => Err(ErrorData::new(
                ErrorCode::INVALID_REQUEST,
                format!("Unknown resource URI: {}", uri),
                None,
            )),
        }
    }

    /// show a Sankey diagram from flow data
    #[tool(
        name = "render_sankey",
        description = r#"show a Sankey diagram from flow data
The data must contain:
- nodes: Array of objects with 'name' and optional 'category' properties
- links: Array of objects with 'source', 'target', and 'value' properties

IMPORTANT: Links must NOT form cycles (e.g. A→B and B→A). Sankey diagrams are
directional acyclic flows. If the data has circular relationships, restructure
them so flow moves in one direction (e.g. add a separate node like "Re-signed"
instead of linking back to an earlier node).

Example:
{
  "nodes": [
    {"name": "Source A", "category": "source"},
    {"name": "Target B", "category": "target"}
  ],
  "links": [
    {"source": "Source A", "target": "Target B", "value": 100}
  ]
}"#,
        meta = ui_resource_meta("ui://autovisualiser/sankey")
    )]
    pub async fn render_sankey(
        &self,
        params: Parameters<RenderSankeyParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let inner = params.0;
        inner.data.validate()?;
        let data = validate_data_param(
            &serde_json::to_value(inner).map_err(|e| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("Invalid parameters: {}", e),
                    None,
                )
            })?,
            false,
        )?;

        let node_count = data
            .get("nodes")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let link_count = data
            .get("links")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let text_fallback = format!(
            "sankey diagram: {} node(s), {} link(s)",
            node_count, link_count
        );

        let mut result = CallToolResult::structured(data);
        result.content = vec![Content::text(text_fallback)];
        result = result.with_meta(Some(ui_resource_meta("ui://autovisualiser/sankey")));

        Ok(result)
    }

    /// show a radar chart (spider chart) for multi-dimensional data comparison
    #[tool(
        name = "render_radar",
        description = r#"show a radar chart (spider chart) for multi-dimensional data comparison

The data must contain:
- labels: Array of strings representing the dimensions/axes
- datasets: Array of dataset objects with 'label' and 'data' properties

Example:
{
  "labels": ["Speed", "Strength", "Endurance", "Agility", "Intelligence"],
  "datasets": [
    {
      "label": "Player 1",
      "data": [85, 70, 90, 75, 80]
    },
    {
      "label": "Player 2",
      "data": [75, 85, 80, 90, 70]
    }
  ]
}"#,
        meta = ui_resource_meta("ui://autovisualiser/radar")
    )]
    pub async fn render_radar(
        &self,
        params: Parameters<RenderRadarParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let inner = params.0;
        inner.data.validate()?;
        let data = validate_data_param(
            &serde_json::to_value(inner).map_err(|e| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("Invalid parameters: {}", e),
                    None,
                )
            })?,
            false,
        )?;

        let label_count = data
            .get("labels")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let dataset_count = data
            .get("datasets")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let text_fallback = format!(
            "radar chart: {} dimension(s), {} dataset(s)",
            label_count, dataset_count
        );

        let mut result = CallToolResult::structured(data);
        result.content = vec![Content::text(text_fallback)];
        result = result.with_meta(Some(ui_resource_meta("ui://autovisualiser/radar")));

        Ok(result)
    }

    /// show pie or donut charts for categorical data visualization
    #[tool(
        name = "render_donut",
        description = r#"show pie or donut charts for categorical data visualization.
Supports one or more charts in a grid layout.
The `data` field must always be an array; pass a single-element array for one chart.

Each chart object must contain:
- values: Array of numbers OR objects with 'label' and 'value'
- type: Optional 'doughnut' (default) or 'pie'
- title: Optional chart title
- labels: Optional array of labels (required when values are plain numbers)

Example single chart (labeled values):
[
  {
    "values": [
      {"label": "Marketing", "value": 25000},
      {"label": "Development", "value": 35000}
    ],
    "title": "Budget"
  }
]

Example single chart (parallel arrays):
[
  {
    "values": [45000, 38000],
    "labels": ["Product A", "Product B"],
    "type": "pie"
  }
]

Example multiple charts (array of chart objects):
[
  {"values": [60, 40], "labels": ["Yes", "No"], "title": "Q1"},
  {"values": [75, 25], "labels": ["Yes", "No"], "title": "Q2"}
]"#,
        meta = ui_resource_meta("ui://autovisualiser/donut")
    )]
    pub async fn render_donut(
        &self,
        params: Parameters<RenderDonutParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let inner = params.0;
        validate_donut_charts(&inner.data)?;
        let data = validate_data_param(
            &serde_json::to_value(inner).map_err(|e| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("Invalid parameters: {}", e),
                    None,
                )
            })?,
            true,
        )?;

        let charts = data.as_array().ok_or_else(|| {
            ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                "The 'data' parameter must be an array.".to_string(),
                None,
            )
        })?;
        let text_fallback = if charts.len() == 1 {
            let title = charts[0]
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled");
            format!("donut/pie chart: \"{}\"", title)
        } else {
            format!("donut/pie chart: {} chart(s)", charts.len())
        };

        let mut result = CallToolResult::structured(data);
        result.content = vec![Content::text(text_fallback)];
        result = result.with_meta(Some(ui_resource_meta("ui://autovisualiser/donut")));

        Ok(result)
    }

    /// show a treemap visualization for hierarchical data
    #[tool(
        name = "render_treemap",
        description = r#"show a treemap visualization for hierarchical data with proportional area representation as boxes

The data should be a hierarchical structure with:
- name: Name of the node (required)
- value: Numeric value for leaf nodes (optional for parent nodes)
- children: Array of child nodes (optional)
- category: Category for coloring (optional)

Example:
{
  "name": "Root",
  "children": [
    {
      "name": "Group A",
      "children": [
        {"name": "Item 1", "value": 100, "category": "Type1"},
        {"name": "Item 2", "value": 200, "category": "Type2"}
      ]
    },
    {"name": "Item 3", "value": 150, "category": "Type1"}
  ]
}"#,
        meta = ui_resource_meta("ui://autovisualiser/treemap")
    )]
    pub async fn render_treemap(
        &self,
        params: Parameters<RenderTreemapParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let inner = params.0;
        inner.data.validate()?;
        let data = validate_data_param(
            &serde_json::to_value(inner).map_err(|e| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("Invalid parameters: {}", e),
                    None,
                )
            })?,
            false,
        )?;

        let root_name = data.get("name").and_then(|v| v.as_str()).unwrap_or("Root");
        let child_count = data
            .get("children")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let text_fallback = format!(
            "treemap: \"{}\" with {} top-level children",
            root_name, child_count
        );

        let mut result = CallToolResult::structured(data);
        result.content = vec![Content::text(text_fallback)];
        result = result.with_meta(Some(ui_resource_meta("ui://autovisualiser/treemap")));

        Ok(result)
    }

    /// Show a chord diagram visualization for relationships and flows
    #[tool(
        name = "render_chord",
        description = r#"Show a chord diagram visualization for showing relationships and flows between entities.

The data must contain:
- labels: Array of strings representing the entities
- matrix: 2D array of numbers representing flows (matrix[i][j] = flow from i to j)

Example:
{
  "labels": ["North America", "Europe", "Asia", "Africa"],
  "matrix": [
    [0, 15, 25, 8],
    [18, 0, 20, 12],
    [22, 18, 0, 15],
    [5, 10, 18, 0]
  ]
}"#,
        meta = ui_resource_meta("ui://autovisualiser/chord")
    )]
    pub async fn render_chord(
        &self,
        params: Parameters<RenderChordParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let inner = params.0;
        inner.data.validate()?;
        let data = validate_data_param(
            &serde_json::to_value(inner).map_err(|e| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("Invalid parameters: {}", e),
                    None,
                )
            })?,
            false,
        )?;

        let entity_count = data
            .get("labels")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let text_fallback = format!("chord diagram: {} entities", entity_count);

        let mut result = CallToolResult::structured(data);
        result.content = vec![Content::text(text_fallback)];
        result = result.with_meta(Some(ui_resource_meta("ui://autovisualiser/chord")));

        Ok(result)
    }

    /// show an interactive map visualization with location markers
    #[tool(
        name = "render_map",
        description = r#"show an interactive map visualization with location markers using Leaflet.

The data must contain:
- markers: Array of objects with 'lat', 'lng', and optional properties
- title: Optional title for the map (default: "Interactive Map")
- subtitle: Optional subtitle (default: "Geographic data visualization")
- center: Optional center point {lat, lng} (default: USA center)
- zoom: Optional initial zoom level (default: 4)
- clustering: Optional boolean to enable/disable clustering (default: true)
- autoFit: Optional boolean to auto-fit map to markers (default: true)

Marker properties:
- lat: Latitude (required)
- lng: Longitude (required)
- name: Location name
- value: Numeric value for sizing/coloring
- description: Description text
- popup: Custom popup HTML
- color: Custom marker color
- label: Custom marker label
- useDefaultIcon: Use default Leaflet icon

Example:
{
  "title": "Store Locations",
  "markers": [
    {"lat": 37.7749, "lng": -122.4194, "name": "SF Store", "value": 150000},
    {"lat": 40.7128, "lng": -74.0060, "name": "NYC Store", "value": 200000}
  ]
}"#,
        meta = ui_resource_meta("ui://autovisualiser/map")
    )]
    pub async fn render_map(
        &self,
        params: Parameters<RenderMapParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let inner = params.0;
        inner.data.validate()?;
        let data = validate_data_param(
            &serde_json::to_value(inner).map_err(|e| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("Invalid parameters: {}", e),
                    None,
                )
            })?,
            false,
        )?;

        let title = data
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Interactive Map");
        let marker_count = data
            .get("markers")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let text_fallback = format!("map: \"{}\" with {} marker(s)", title, marker_count);

        let mut result = CallToolResult::structured(data);
        result.content = vec![Content::text(text_fallback)];
        result = result.with_meta(Some(ui_resource_meta("ui://autovisualiser/map")));

        Ok(result)
    }

    /// show a Mermaid diagram from Mermaid syntax
    #[tool(
        name = "render_mermaid",
        description = r#"show a Mermaid diagram from Mermaid syntax

Provide the Mermaid code as a string. Supports flowcharts, sequence diagrams, Gantt charts, etc.

Example:
graph TD;
    A-->B;
    A-->C;
    B-->D;
    C-->D;
"#,
        meta = ui_resource_meta("ui://autovisualiser/mermaid")
    )]
    pub async fn render_mermaid(
        &self,
        params: Parameters<RenderMermaidParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let mermaid_code = &params.0.mermaid_code;

        let first_line = mermaid_code.lines().next().unwrap_or("diagram").trim();
        let text_fallback = format!("mermaid diagram: {}", first_line);

        let data = serde_json::to_value(&params.0).map_err(|e| {
            ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!("Invalid parameters: {}", e),
                None,
            )
        })?;

        let mut result = CallToolResult::structured(data);
        result.content = vec![Content::text(text_fallback)];
        result = result.with_meta(Some(ui_resource_meta("ui://autovisualiser/mermaid")));

        Ok(result)
    }

    /// show interactive line, scatter, or bar charts
    #[tool(
        name = "show_chart",
        description = r#"show interactive line, scatter, or bar charts

Required: type ('line', 'scatter', or 'bar'), datasets array
Optional: labels, title, subtitle, xAxisLabel, yAxisLabel, options

Example:
{
  "type": "line",
  "title": "Monthly Sales",
  "labels": ["Jan", "Feb", "Mar"],
  "datasets": [
    {"label": "Product A", "data": [65, 59, 80]}
  ]
}"#,
        meta = ui_resource_meta("ui://autovisualiser/chart")
    )]
    pub async fn show_chart(
        &self,
        params: Parameters<ShowChartParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let inner = params.0;
        inner.data.validate()?;
        let data = validate_data_param(
            &serde_json::to_value(inner).map_err(|e| {
                ErrorData::new(
                    ErrorCode::INVALID_PARAMS,
                    format!("Invalid parameters: {}", e),
                    None,
                )
            })?,
            false,
        )?;

        // Build a text fallback describing the chart for non-UI hosts
        let chart_type = data.get("type").and_then(|v| v.as_str()).unwrap_or("chart");
        let title = data
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled");
        let dataset_count = data
            .get("datasets")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let text_fallback = format!(
            "{} chart: \"{}\" with {} dataset(s)",
            chart_type, title, dataset_count
        );

        // Return structuredContent (the raw data) + text fallback.
        // The host fetches the template via read_resource and sends this data
        // to the template via the MCP Apps postMessage lifecycle.
        let mut result = CallToolResult::structured(data);
        result.content = vec![Content::text(text_fallback)];
        result = result.with_meta(Some(ui_resource_meta("ui://autovisualiser/chart")));

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::handler::server::wrapper::Parameters;
    use rmcp::model::RawContent;
    use serde_json::json;

    #[test]
    fn test_validate_data_param_rejects_string() {
        // Test that a string value for data is rejected
        let params = json!({
            "data": "{\"labels\": [\"A\", \"B\"], \"matrix\": [[0, 1], [1, 0]]}"
        });

        let result = validate_data_param(&params, false);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(err
            .message
            .contains("must be a JSON object, not a JSON string"));
        assert!(err.message.contains("without comments"));
    }

    #[test]
    fn test_validate_data_param_accepts_object() {
        // Test that a proper object is accepted
        let params = json!({
            "data": {
                "labels": ["A", "B"],
                "matrix": [[0, 1], [1, 0]]
            }
        });

        let result = validate_data_param(&params, false);
        assert!(result.is_ok());

        let data = result.unwrap();
        assert!(data.is_object());
        assert_eq!(data["labels"][0], "A");
    }

    #[test]
    fn test_validate_data_param_rejects_array_when_not_allowed() {
        // Test that an array is rejected when allow_array is false
        let params = json!({
            "data": [
                {"label": "A", "value": 10},
                {"label": "B", "value": 20}
            ]
        });

        let result = validate_data_param(&params, false);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("must be a JSON object"));
    }

    #[test]
    fn test_validate_data_param_accepts_array_when_allowed() {
        // Test that an array is accepted when allow_array is true
        let params = json!({
            "data": [
                {"label": "A", "value": 10},
                {"label": "B", "value": 20}
            ]
        });

        let result = validate_data_param(&params, true);
        assert!(result.is_ok());

        let data = result.unwrap();
        assert!(data.is_array());
        assert_eq!(data[0]["label"], "A");
    }

    #[test]
    fn test_validate_data_param_missing_data() {
        // Test that missing data parameter is rejected
        let params = json!({
            "other": "value"
        });

        let result = validate_data_param(&params, false);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("Missing 'data' parameter"));
    }

    #[test]
    fn test_validate_data_param_rejects_primitive_values() {
        // Test that primitive values (number, boolean) are rejected
        let params_number = json!({
            "data": 42
        });

        let result = validate_data_param(&params_number, false);
        assert!(result.is_err());

        let params_bool = json!({
            "data": true
        });

        let result = validate_data_param(&params_bool, false);
        assert!(result.is_err());

        let params_null = json!({
            "data": null
        });

        let result = validate_data_param(&params_null, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_data_param_with_json_containing_comments_as_string() {
        // Test that JSON with comments passed as a string is rejected
        let params = json!({
            "data": r#"{
                "labels": ["A", "B"],
                "matrix": [
                    [0, 1],  // This is a comment
                    [1, 0]   /* Another comment */
                ]
            }"#
        });

        let result = validate_data_param(&params, false);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("not a JSON string"));
        assert!(err.message.contains("without comments"));
    }

    fn assert_mcp_apps_result(
        tool_result: &CallToolResult,
        expected_uri: &str,
        text_contains: &str,
    ) {
        assert_eq!(tool_result.content.len(), 1);
        if let RawContent::Text(text_content) = &tool_result.content[0].raw {
            assert!(
                text_content.text.contains(text_contains),
                "Text fallback '{}' should contain '{}'",
                text_content.text,
                text_contains
            );
        } else {
            panic!("Expected text content as fallback");
        }

        assert!(
            tool_result.structured_content.is_some(),
            "structured_content should be present"
        );

        assert!(tool_result.meta.is_some(), "_meta should be present");
        let meta = tool_result.meta.as_ref().unwrap();
        let ui = meta.0.get("ui").expect("meta should have 'ui' key");
        assert_eq!(
            ui.get("resourceUri").and_then(|v| v.as_str()),
            Some(expected_uri)
        );
    }

    #[tokio::test]
    async fn test_render_sankey() {
        let router = AutoVisualiserRouter::new();
        let params = Parameters(RenderSankeyParams {
            data: SankeyData {
                nodes: vec![
                    SankeyNode {
                        name: "A".to_string(),
                        category: None,
                    },
                    SankeyNode {
                        name: "B".to_string(),
                        category: None,
                    },
                ],
                links: vec![SankeyLink {
                    source: "A".to_string(),
                    target: "B".to_string(),
                    value: 10.0,
                }],
            },
        });

        let result = router.render_sankey(params).await;
        assert!(result.is_ok());
        assert_mcp_apps_result(&result.unwrap(), "ui://autovisualiser/sankey", "sankey");
    }

    #[tokio::test]
    async fn test_render_radar() {
        let router = AutoVisualiserRouter::new();
        let params = Parameters(RenderRadarParams {
            data: RadarData {
                labels: vec![
                    "Speed".to_string(),
                    "Power".to_string(),
                    "Agility".to_string(),
                ],
                datasets: vec![RadarDataset {
                    label: "Player 1".to_string(),
                    data: vec![80.0, 90.0, 85.0],
                }],
            },
        });

        let result = router.render_radar(params).await;
        assert!(result.is_ok());
        assert_mcp_apps_result(&result.unwrap(), "ui://autovisualiser/radar", "radar");
    }

    #[tokio::test]
    async fn test_render_donut() {
        let router = AutoVisualiserRouter::new();
        let params = Parameters(RenderDonutParams {
            data: vec![SingleDonutChart {
                values: vec![
                    DonutDataItem::Number(30.0),
                    DonutDataItem::Number(40.0),
                    DonutDataItem::Number(30.0),
                ],
                labels: Some(vec!["A".to_string(), "B".to_string(), "C".to_string()]),
                title: None,
                chart_type: None,
            }],
        });

        let result = router.render_donut(params).await;
        assert!(result.is_ok());
        assert_mcp_apps_result(&result.unwrap(), "ui://autovisualiser/donut", "donut");
    }

    #[tokio::test]
    async fn test_render_treemap() {
        let router = AutoVisualiserRouter::new();
        let params = Parameters(RenderTreemapParams {
            data: TreemapNode {
                name: "root".to_string(),
                value: None,
                category: None,
                children: Some(vec![
                    TreemapNode {
                        name: "A".to_string(),
                        value: Some(100.0),
                        category: Some("Type1".to_string()),
                        children: None,
                    },
                    TreemapNode {
                        name: "B".to_string(),
                        value: Some(200.0),
                        category: Some("Type2".to_string()),
                        children: None,
                    },
                ]),
            },
        });

        let result = router.render_treemap(params).await;
        assert!(result.is_ok());
        assert_mcp_apps_result(&result.unwrap(), "ui://autovisualiser/treemap", "treemap");
    }

    #[tokio::test]
    async fn test_render_chord() {
        let router = AutoVisualiserRouter::new();
        let params = Parameters(RenderChordParams {
            data: ChordData {
                labels: vec!["A".to_string(), "B".to_string(), "C".to_string()],
                matrix: vec![
                    vec![0.0, 10.0, 5.0],
                    vec![10.0, 0.0, 15.0],
                    vec![5.0, 15.0, 0.0],
                ],
            },
        });

        let result = router.render_chord(params).await;
        assert!(result.is_ok());
        assert_mcp_apps_result(&result.unwrap(), "ui://autovisualiser/chord", "chord");
    }

    #[tokio::test]
    async fn test_render_map() {
        let router = AutoVisualiserRouter::new();
        let params = Parameters(RenderMapParams {
            data: MapData {
                markers: vec![MapMarker {
                    lat: 0.0,
                    lng: 0.0,
                    name: Some("Origin".to_string()),
                    value: None,
                    description: None,
                    popup: None,
                    color: None,
                    label: None,
                    use_default_icon: None,
                }],
                title: None,
                subtitle: None,
                center: None,
                zoom: None,
                clustering: None,
                cluster_radius: None,
                auto_fit: None,
            },
        });

        let result = router.render_map(params).await;
        assert!(result.is_ok());
        assert_mcp_apps_result(&result.unwrap(), "ui://autovisualiser/map", "map");
    }

    #[tokio::test]
    async fn test_show_chart() {
        let router = AutoVisualiserRouter::new();
        let params = Parameters(ShowChartParams {
            data: ChartData {
                chart_type: ChartType::Scatter,
                datasets: vec![ChartDataset {
                    label: "Test Data".to_string(),
                    data: ChartDataValues::Points(vec![
                        ChartPoint { x: 1.0, y: 2.0 },
                        ChartPoint { x: 2.0, y: 4.0 },
                    ]),
                    background_color: None,
                    border_color: None,
                    border_width: None,
                    tension: None,
                    fill: None,
                }],
                labels: None,
                title: None,
                subtitle: None,
                x_axis_label: None,
                y_axis_label: None,
            },
        });

        let result = router.show_chart(params).await;
        assert!(result.is_ok());
        assert_mcp_apps_result(&result.unwrap(), "ui://autovisualiser/chart", "scatter");
    }

    #[tokio::test]
    async fn test_render_mermaid() {
        let router = AutoVisualiserRouter::new();
        let params = Parameters(RenderMermaidParams {
            mermaid_code: "graph TD;\n    A-->B;\n    A-->C;\n    B-->D;\n    C-->D;".to_string(),
        });

        let result = router.render_mermaid(params).await;
        assert!(result.is_ok());
        assert_mcp_apps_result(&result.unwrap(), "ui://autovisualiser/mermaid", "mermaid");
    }
}

#[cfg(test)]
mod donut_format_tests {
    use super::*;
    use serde_json::json;

    fn round_trip(input: serde_json::Value) -> Result<serde_json::Value, String> {
        let parsed: RenderDonutParams = serde_json::from_value(input).map_err(|e| e.to_string())?;
        let serialized = serde_json::to_value(&parsed).map_err(|e| e.to_string())?;
        // Simulate validate_data_param extracting "data"
        Ok(serialized.get("data").cloned().unwrap_or(serialized))
    }

    #[test]
    fn labeled_values_single_chart() {
        // {"data": [{"values": [{"label": "A", "value": 10}, ...]}]}
        let input = json!({
            "data": [{
                "values": [
                    {"label": "A", "value": 10},
                    {"label": "B", "value": 20}
                ]
            }]
        });
        let result = round_trip(input);
        assert!(
            result.is_ok(),
            "labeled values should parse: {:?}",
            result.err()
        );
    }

    #[test]
    fn parallel_arrays_single_chart() {
        // {"data": [{"values": [10, 20], "labels": ["A", "B"]}]}
        let input = json!({
            "data": [{
                "values": [10, 20],
                "labels": ["A", "B"]
            }]
        });
        let result = round_trip(input);
        assert!(
            result.is_ok(),
            "parallel arrays should parse: {:?}",
            result.err()
        );
    }

    #[test]
    fn multiple_charts_array() {
        // {"data": [{"values": [10, 20], "labels": ["A", "B"], "title": "Q1"}, ...]}
        let input = json!({
            "data": [
                {"values": [60, 40], "labels": ["Yes", "No"], "title": "Q1"},
                {"values": [75, 25], "labels": ["Yes", "No"], "title": "Q2"}
            ]
        });
        let result = round_trip(input);
        assert!(
            result.is_ok(),
            "multiple charts should parse: {:?}",
            result.err()
        );
    }

    #[test]
    fn labeled_values_with_title_and_type() {
        let input = json!({
            "data": [{
                "values": [
                    {"label": "Marketing", "value": 25000},
                    {"label": "Development", "value": 35000}
                ],
                "title": "Budget",
                "type": "pie"
            }]
        });
        let result = round_trip(input);
        assert!(
            result.is_ok(),
            "labeled values with title/type should parse: {:?}",
            result.err()
        );
    }
}

#[cfg(test)]
mod donut_regression_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn old_format_1_label_value_as_data_rejects() {
        // Old broken format: {"data": [{"label": "A", "value": 10}]}
        // Items have no "values" field, so this should NOT parse as Multiple
        let input = json!({
            "data": [{"label": "A", "value": 10}, {"label": "B", "value": 20}]
        });
        let parsed: Result<RenderDonutParams, _> = serde_json::from_value(input);
        assert!(
            parsed.is_err(),
            "bare label/value items without 'values' wrapper should not parse"
        );
    }

    #[test]
    fn old_format_3_array_wrapped_with_data_key_still_works() {
        // Old working format used "data" key inside chart objects.
        // Template now reads c.values || c.data, so if an LLM sends the old shape
        // the Rust types would reject it (items need "values" not "data").
        // This verifies the schema enforces the new shape.
        let input = json!({
            "data": [{"labels": ["A", "B"], "data": [10, 20]}]
        });
        let parsed: Result<RenderDonutParams, _> = serde_json::from_value(input);
        assert!(
            parsed.is_err(),
            "old 'data' key inside chart objects should not parse — schema now requires 'values'"
        );
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn sankey_rejects_empty_nodes() {
        let data = SankeyData {
            nodes: vec![],
            links: vec![SankeyLink {
                source: "A".into(),
                target: "B".into(),
                value: 10.0,
            }],
        };
        assert!(data.validate().is_err());
    }

    #[test]
    fn sankey_rejects_unknown_source() {
        let data = SankeyData {
            nodes: vec![SankeyNode {
                name: "A".into(),
                category: None,
            }],
            links: vec![SankeyLink {
                source: "X".into(),
                target: "A".into(),
                value: 10.0,
            }],
        };
        let err = data.validate().unwrap_err();
        assert!(err.message.contains("'X' not found"));
    }

    #[test]
    fn sankey_rejects_non_positive_value() {
        let data = SankeyData {
            nodes: vec![
                SankeyNode {
                    name: "A".into(),
                    category: None,
                },
                SankeyNode {
                    name: "B".into(),
                    category: None,
                },
            ],
            links: vec![SankeyLink {
                source: "A".into(),
                target: "B".into(),
                value: 0.0,
            }],
        };
        assert!(data.validate().is_err());
    }

    #[test]
    fn radar_rejects_mismatched_dimensions() {
        let data = RadarData {
            labels: vec!["A".into(), "B".into(), "C".into()],
            datasets: vec![RadarDataset {
                label: "Test".into(),
                data: vec![1.0, 2.0], // 2 values but 3 labels
            }],
        };
        let err = data.validate().unwrap_err();
        assert!(err.message.contains("2 values"));
        assert!(err.message.contains("3 labels"));
    }

    #[test]
    fn radar_rejects_empty_datasets() {
        let data = RadarData {
            labels: vec!["A".into()],
            datasets: vec![],
        };
        assert!(data.validate().is_err());
    }

    #[test]
    fn donut_rejects_empty_values() {
        let data = vec![SingleDonutChart {
            values: vec![],
            title: None,
            chart_type: None,
            labels: None,
        }];
        assert!(validate_donut_charts(&data).is_err());
    }

    #[test]
    fn donut_rejects_mismatched_labels() {
        let data = vec![SingleDonutChart {
            values: vec![DonutDataItem::Number(10.0), DonutDataItem::Number(20.0)],
            title: None,
            chart_type: None,
            labels: Some(vec!["A".into()]), // 1 label but 2 values
        }];
        let err = validate_donut_charts(&data).unwrap_err();
        assert!(err.message.contains("labels"));
    }

    #[test]
    fn donut_rejects_empty_multiple() {
        let data: Vec<SingleDonutChart> = vec![];
        assert!(validate_donut_charts(&data).is_err());
    }

    #[test]
    fn chord_rejects_mismatched_matrix() {
        let data = ChordData {
            labels: vec!["A".into(), "B".into()],
            matrix: vec![vec![0.0, 1.0]], // 1 row but 2 labels
        };
        let err = data.validate().unwrap_err();
        assert!(err.message.contains("1 rows"));
        assert!(err.message.contains("2 labels"));
    }

    #[test]
    fn chord_rejects_ragged_matrix() {
        let data = ChordData {
            labels: vec!["A".into(), "B".into()],
            matrix: vec![vec![0.0, 1.0], vec![1.0]], // row 1 has 1 col instead of 2
        };
        let err = data.validate().unwrap_err();
        assert!(err.message.contains("row 1"));
    }

    #[test]
    fn treemap_rejects_empty_node() {
        let data = TreemapNode {
            name: "Root".into(),
            value: None,
            category: None,
            children: None,
        };
        assert!(data.validate().is_err());
    }

    #[test]
    fn treemap_validates_recursively() {
        let data = TreemapNode {
            name: "Root".into(),
            value: None,
            category: None,
            children: Some(vec![TreemapNode {
                name: "Empty child".into(),
                value: None,
                category: None,
                children: Some(vec![]),
            }]),
        };
        let err = data.validate().unwrap_err();
        assert!(err.message.contains("Empty child"));
    }

    #[test]
    fn chart_rejects_empty_datasets() {
        let data = ChartData {
            chart_type: ChartType::Line,
            datasets: vec![],
            labels: None,
            title: None,
            subtitle: None,
            x_axis_label: None,
            y_axis_label: None,
        };
        assert!(data.validate().is_err());
    }

    #[test]
    fn chart_rejects_mismatched_labels() {
        let data = ChartData {
            chart_type: ChartType::Bar,
            datasets: vec![ChartDataset {
                label: "Test".into(),
                data: ChartDataValues::Numbers(vec![1.0, 2.0]),
                background_color: None,
                border_color: None,
                border_width: None,
                tension: None,
                fill: None,
            }],
            labels: Some(vec!["A".into(), "B".into(), "C".into()]), // 3 labels but 2 values
            title: None,
            subtitle: None,
            x_axis_label: None,
            y_axis_label: None,
        };
        let err = data.validate().unwrap_err();
        assert!(err.message.contains("2 values"));
        assert!(err.message.contains("3 labels"));
    }

    #[test]
    fn map_rejects_empty_markers() {
        let data = MapData {
            markers: vec![],
            title: None,
            subtitle: None,
            center: None,
            zoom: None,
            clustering: None,
            cluster_radius: None,
            auto_fit: None,
        };
        assert!(data.validate().is_err());
    }

    #[test]
    fn map_rejects_invalid_lat() {
        let data = MapData {
            markers: vec![MapMarker {
                lat: 91.0,
                lng: 0.0,
                name: None,
                value: None,
                description: None,
                popup: None,
                color: None,
                label: None,
                use_default_icon: None,
            }],
            title: None,
            subtitle: None,
            center: None,
            zoom: None,
            clustering: None,
            cluster_radius: None,
            auto_fit: None,
        };
        let err = data.validate().unwrap_err();
        assert!(err.message.contains("latitude"));
    }

    #[test]
    fn map_rejects_invalid_lng() {
        let data = MapData {
            markers: vec![MapMarker {
                lat: 0.0,
                lng: 200.0,
                name: None,
                value: None,
                description: None,
                popup: None,
                color: None,
                label: None,
                use_default_icon: None,
            }],
            title: None,
            subtitle: None,
            center: None,
            zoom: None,
            clustering: None,
            cluster_radius: None,
            auto_fit: None,
        };
        let err = data.validate().unwrap_err();
        assert!(err.message.contains("longitude"));
    }
}
