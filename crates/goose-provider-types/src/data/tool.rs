// Adapted from rmcp 3.4.1, Copyright Model Context Protocol contributors.
// Licensed under Apache-2.0, like this repository.
#[cfg(not(feature = "std"))]
use crate::prelude::*;
use crate::runtime::Shared as Arc;
use alloc::borrow::Cow;

/// Tools represent a routine that a server can execute
/// Tool calls represent requests from the client to execute one
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Icon, JsonObject, MetaObject};

/// A tool that can be used by a model.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Tool {
    /// The name of the tool
    pub name: Cow<'static, str>,
    /// A human-readable title for the tool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// A description of what the tool does
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Cow<'static, str>>,
    /// A JSON Schema object defining the expected parameters for the tool
    #[serde(
        serialize_with = "serialize_shared",
        deserialize_with = "deserialize_shared"
    )]
    pub input_schema: Arc<JsonObject>,
    /// An optional JSON Schema object defining the structure of the tool's output
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(
        default,
        serialize_with = "serialize_optional_shared",
        deserialize_with = "deserialize_optional_shared"
    )]
    pub output_schema: Option<Arc<JsonObject>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Optional additional tool information.
    pub annotations: Option<ToolAnnotations>,
    /// Optional list of icons for the tool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<Icon>>,
    /// Optional additional metadata for this tool
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<MetaObject>,
}

/// Additional properties describing a Tool to clients.
///
/// NOTE: all properties in ToolAnnotations are **hints**.
/// They are not guaranteed to provide a faithful description of
/// tool behavior (including descriptive properties like `title`).
///
/// Clients should never make tool use decisions based on ToolAnnotations
/// received from untrusted servers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ToolAnnotations {
    /// A human-readable title for the tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// If true, the tool does not modify its environment.
    ///
    /// Default: false
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,

    /// If true, the tool may perform destructive updates to its environment.
    /// If false, the tool performs only additive updates.
    ///
    /// (This property is meaningful only when `readOnlyHint == false`)
    ///
    /// Default: true
    /// A human-readable description of the tool's purpose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,

    /// If true, calling the tool repeatedly with the same arguments
    /// will have no additional effect on the its environment.
    ///
    /// (This property is meaningful only when `readOnlyHint == false`)
    ///
    /// Default: false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,

    /// If true, this tool may interact with an "open world" of external
    /// entities. If false, the tool's domain of interaction is closed.
    /// For example, the world of a web search tool is open, whereas that
    /// of a memory tool is not.
    ///
    /// Default: true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

impl ToolAnnotations {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new ToolAnnotations with all fields specified
    pub fn from_raw(
        title: Option<String>,
        read_only_hint: Option<bool>,
        destructive_hint: Option<bool>,
        idempotent_hint: Option<bool>,
        open_world_hint: Option<bool>,
    ) -> Self {
        ToolAnnotations {
            title,
            read_only_hint,
            destructive_hint,
            idempotent_hint,
            open_world_hint,
        }
    }

    pub fn with_title<T>(title: T) -> Self
    where
        T: Into<String>,
    {
        ToolAnnotations {
            title: Some(title.into()),
            ..Self::default()
        }
    }
    pub fn read_only(self, read_only: bool) -> Self {
        ToolAnnotations {
            read_only_hint: Some(read_only),
            ..self
        }
    }
    pub fn destructive(self, destructive: bool) -> Self {
        ToolAnnotations {
            destructive_hint: Some(destructive),
            ..self
        }
    }
    pub fn idempotent(self, idempotent: bool) -> Self {
        ToolAnnotations {
            idempotent_hint: Some(idempotent),
            ..self
        }
    }
    pub fn open_world(self, open_world: bool) -> Self {
        ToolAnnotations {
            open_world_hint: Some(open_world),
            ..self
        }
    }

    /// If not set, defaults to true.
    pub fn is_destructive(&self) -> bool {
        self.destructive_hint.unwrap_or(true)
    }

    /// If not set, defaults to false.
    pub fn is_idempotent(&self) -> bool {
        self.idempotent_hint.unwrap_or(false)
    }
}

impl Tool {
    /// Create a new tool with the given name and description
    pub fn new<N, D, S>(name: N, description: D, input_schema: S) -> Self
    where
        N: Into<Cow<'static, str>>,
        D: Into<Cow<'static, str>>,
        S: Into<Arc<JsonObject>>,
    {
        Tool {
            name: name.into(),
            title: None,
            description: Some(description.into()),
            input_schema: input_schema.into(),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }

    /// Create a new tool with just a name and input schema (no description)
    pub fn new_with_raw<N, S>(
        name: N,
        description: Option<Cow<'static, str>>,
        input_schema: S,
    ) -> Self
    where
        N: Into<Cow<'static, str>>,
        S: Into<Arc<JsonObject>>,
    {
        Tool {
            name: name.into(),
            title: None,
            description,
            input_schema: input_schema.into(),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }

    /// Set the human-readable title
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the output schema from a raw value
    pub fn with_raw_output_schema(mut self, output_schema: Arc<JsonObject>) -> Self {
        self.output_schema = Some(output_schema);
        self
    }

    /// Set the annotations
    pub fn with_annotations(mut self, annotations: ToolAnnotations) -> Self {
        self.annotations = Some(annotations);
        self
    }

    /// Set the icons
    pub fn with_icons(mut self, icons: Vec<Icon>) -> Self {
        self.icons = Some(icons);
        self
    }

    /// Set the metadata
    pub fn with_meta(mut self, meta: MetaObject) -> Self {
        self.meta = Some(meta);
        self
    }

    pub fn annotate(self, annotations: ToolAnnotations) -> Self {
        Tool {
            annotations: Some(annotations),
            ..self
        }
    }

    /// Get the schema as json value
    pub fn schema_as_json_value(&self) -> Value {
        Value::Object(self.input_schema.as_ref().clone())
    }
}

fn serialize_shared<S: serde::Serializer>(
    value: &Arc<JsonObject>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    value.as_ref().serialize(serializer)
}
fn deserialize_shared<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Arc<JsonObject>, D::Error> {
    JsonObject::deserialize(deserializer).map(Arc::new)
}
fn serialize_optional_shared<S: serde::Serializer>(
    value: &Option<Arc<JsonObject>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    value.as_deref().serialize(serializer)
}
fn deserialize_optional_shared<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Arc<JsonObject>>, D::Error> {
    Option::<JsonObject>::deserialize(deserializer).map(|v| v.map(Arc::new))
}
