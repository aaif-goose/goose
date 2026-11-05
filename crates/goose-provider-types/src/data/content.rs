// Adapted from rmcp 3.4.1, Copyright Model Context Protocol contributors.
// Licensed under Apache-2.0, like this repository.
#[cfg(not(feature = "std"))]
use crate::prelude::*;

// ToolUseContent/ToolResultContent are SEP-2577-deprecated; internal references are expected.
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{resource::ResourceContents, Annotations, MetaObject};

// ---------------------------------------------------------------------------
// Flat content structs
// ---------------------------------------------------------------------------

/// Text content block (spec `TextContent`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct TextContent {
    /// The text content of the message.
    pub text: String,
    /// Optional protocol-level metadata for this content block.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<MetaObject>,
    /// Optional annotations describing how the client should use this content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

impl TextContent {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            meta: None,
            annotations: None,
        }
    }

    pub fn with_meta(mut self, meta: MetaObject) -> Self {
        self.meta = Some(meta);
        self
    }

    pub fn with_annotations(mut self, annotations: Annotations) -> Self {
        self.annotations = Some(annotations);
        self
    }
}

/// Image content with base64-encoded data (spec `ImageContent`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ImageContent {
    /// The base64-encoded image data.
    pub data: String,
    /// The MIME type of the image (e.g. `image/png`).
    pub mime_type: String,
    /// Optional protocol-level metadata for this content block.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<MetaObject>,
    /// Optional annotations describing how the client should use this content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

impl ImageContent {
    pub fn new(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self {
            data: data.into(),
            mime_type: mime_type.into(),
            meta: None,
            annotations: None,
        }
    }

    pub fn with_meta(mut self, meta: MetaObject) -> Self {
        self.meta = Some(meta);
        self
    }

    pub fn with_annotations(mut self, annotations: Annotations) -> Self {
        self.annotations = Some(annotations);
        self
    }
}

/// Audio content with base64-encoded data (spec `AudioContent`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct AudioContent {
    /// The base64-encoded audio data.
    pub data: String,
    /// The MIME type of the audio (e.g. `audio/wav`).
    pub mime_type: String,
    /// Optional protocol-level metadata for this content block.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<MetaObject>,
    /// Optional annotations describing how the client should use this content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

impl AudioContent {
    pub fn new(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self {
            data: data.into(),
            mime_type: mime_type.into(),
            meta: None,
            annotations: None,
        }
    }

    pub fn with_meta(mut self, meta: MetaObject) -> Self {
        self.meta = Some(meta);
        self
    }

    pub fn with_annotations(mut self, annotations: Annotations) -> Self {
        self.annotations = Some(annotations);
        self
    }
}

/// Embedded resource content (spec `EmbeddedResource`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct EmbeddedResource {
    /// The embedded resource contents (text or blob).
    pub resource: ResourceContents,
    /// Optional protocol-level metadata for this content block.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<MetaObject>,
    /// Optional annotations describing how the client should use this content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

impl EmbeddedResource {
    pub fn new(resource: ResourceContents) -> Self {
        Self {
            resource,
            meta: None,
            annotations: None,
        }
    }

    pub fn get_text(&self) -> String {
        match &self.resource {
            ResourceContents::TextResourceContents { text, .. } => text.clone(),
            _ => String::new(),
        }
    }

    pub fn with_meta(mut self, meta: MetaObject) -> Self {
        self.meta = Some(meta);
        self
    }

    pub fn with_annotations(mut self, annotations: Annotations) -> Self {
        self.annotations = Some(annotations);
        self
    }
}

/// Unified content block union (spec `ContentBlock`).
///
/// `text | image | audio | resource_link | resource`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContentBlock {
    Text(TextContent),
    Image(ImageContent),
    Audio(AudioContent),
    Resource(EmbeddedResource),
    ResourceLink(super::resource::Resource),
}

impl ContentBlock {
    pub fn json<S: Serialize>(json: S) -> Result<Self, super::ErrorData> {
        let json = serde_json::to_string(&json).map_err(|e| {
            super::ErrorData::internal_error(
                "fail to serialize response to json",
                Some(json!(
                    {"reason": e.to_string()}
                )),
            )
        })?;
        Ok(ContentBlock::text(json))
    }

    pub fn text(text: impl Into<String>) -> Self {
        ContentBlock::Text(TextContent::new(text))
    }

    pub fn image(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        ContentBlock::Image(ImageContent::new(data, mime_type))
    }

    pub fn audio(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        ContentBlock::Audio(AudioContent::new(data, mime_type))
    }

    pub fn resource(resource: ResourceContents) -> Self {
        ContentBlock::Resource(EmbeddedResource::new(resource))
    }

    pub fn embedded_text(uri: impl Into<String>, content: impl Into<String>) -> Self {
        ContentBlock::Resource(EmbeddedResource::new(
            ResourceContents::TextResourceContents {
                uri: uri.into(),
                mime_type: Some("text/plain".to_string()),
                text: content.into(),
                meta: None,
            },
        ))
    }

    pub fn resource_link(resource: super::resource::Resource) -> Self {
        ContentBlock::ResourceLink(resource)
    }

    pub fn as_text(&self) -> Option<&TextContent> {
        match self {
            ContentBlock::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_image(&self) -> Option<&ImageContent> {
        match self {
            ContentBlock::Image(image) => Some(image),
            _ => None,
        }
    }

    pub fn as_resource(&self) -> Option<&EmbeddedResource> {
        match self {
            ContentBlock::Resource(resource) => Some(resource),
            _ => None,
        }
    }

    pub fn as_resource_link(&self) -> Option<&super::resource::Resource> {
        match self {
            ContentBlock::ResourceLink(link) => Some(link),
            _ => None,
        }
    }

    pub fn as_audio(&self) -> Option<&AudioContent> {
        match self {
            ContentBlock::Audio(audio) => Some(audio),
            _ => None,
        }
    }
}
