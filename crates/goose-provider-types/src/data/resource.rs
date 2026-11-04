// Adapted from rmcp 3.4.1, Copyright Model Context Protocol contributors.
// Licensed under Apache-2.0, like this repository.
#[cfg(not(feature = "std"))]
use crate::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Annotations, Icon, MetaObject};

/// A known resource that the server is capable of reading (spec `Resource`).
///
/// Also used as the inner type of `ContentBlock::ResourceLink` (spec `ResourceLink extends Resource`).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Resource {
    /// The URI of this resource (e.g. `file:///path/to/file`).
    pub uri: String,
    /// The programmatic name of the resource.
    pub name: String,
    /// Optional human-readable display title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional description of what this resource represents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The MIME type of this resource, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// The size of the raw resource content in bytes (before base64/tokenization), if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Optional set of icons the client may display for this resource.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<Icon>>,
    /// Optional protocol-level metadata for this resource.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<MetaObject>,
    /// Optional annotations describing how the client should use this resource.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

impl Resource {
    pub fn new(uri: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            name: name.into(),
            title: None,
            description: None,
            mime_type: None,
            size: None,
            icons: None,
            meta: None,
            annotations: None,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    pub fn with_icons(mut self, icons: Vec<Icon>) -> Self {
        self.icons = Some(icons);
        self
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

/// A template description for resources available on the server (spec `ResourceTemplate`).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ResourceTemplate {
    /// An RFC 6570 URI template for constructing resource URIs.
    pub uri_template: String,
    /// The programmatic name of the resource template.
    pub name: String,
    /// Optional human-readable display title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional description of what this template is for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The MIME type for resources matching this template, if uniform.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Optional set of icons the client may display for this template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<Icon>>,
    /// Optional protocol-level metadata for this resource template.
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<MetaObject>,
    /// Optional annotations describing how the client should use this template.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

impl ResourceTemplate {
    pub fn new(uri_template: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            uri_template: uri_template.into(),
            name: name.into(),
            title: None,
            description: None,
            mime_type: None,
            icons: None,
            meta: None,
            annotations: None,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    pub fn with_icons(mut self, icons: Vec<Icon>) -> Self {
        self.icons = Some(icons);
        self
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

/// The contents of a specific resource or sub-resource.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
#[non_exhaustive]
pub enum ResourceContents {
    #[serde(rename_all = "camelCase")]
    TextResourceContents {
        uri: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        text: String,
        #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
        meta: Option<MetaObject>,
    },
    #[serde(rename_all = "camelCase")]
    BlobResourceContents {
        uri: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        blob: String,
        #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
        meta: Option<MetaObject>,
    },
}

impl ResourceContents {
    pub fn text(text: impl Into<String>, uri: impl Into<String>) -> Self {
        Self::TextResourceContents {
            uri: uri.into(),
            mime_type: Some("text/plain".into()),
            text: text.into(),
            meta: None,
        }
    }

    pub fn blob(blob: impl Into<String>, uri: impl Into<String>) -> Self {
        Self::BlobResourceContents {
            uri: uri.into(),
            mime_type: None,
            blob: blob.into(),
            meta: None,
        }
    }

    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        match &mut self {
            Self::TextResourceContents { mime_type: mt, .. } => *mt = Some(mime_type.into()),
            Self::BlobResourceContents { mime_type: mt, .. } => *mt = Some(mime_type.into()),
        }
        self
    }

    pub fn with_meta(mut self, meta: MetaObject) -> Self {
        match &mut self {
            Self::TextResourceContents { meta: m, .. } => *m = Some(meta),
            Self::BlobResourceContents { meta: m, .. } => *m = Some(meta),
        }
        self
    }
}
