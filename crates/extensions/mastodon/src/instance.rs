//! Instance documents and advertised limits.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceV1 {
    pub uri: String,
    pub title: String,
    pub short_description: String,
    pub description: String,
    pub email: String,
    pub version: String,
    pub urls: Value,
    pub stats: Value,
    pub thumbnail: String,
    pub languages: Vec<String>,
    pub registrations: bool,
    pub approval_required: bool,
    pub invites_enabled: bool,
    pub configuration: Configuration,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceV2 {
    pub domain: String,
    pub title: String,
    pub version: String,
    pub source_url: String,
    pub description: String,
    pub usage: Value,
    pub thumbnail: Value,
    pub languages: Vec<String>,
    pub configuration: Configuration,
    pub registrations: Registrations,
    pub contact: Value,
    pub rules: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Registrations {
    pub enabled: bool,
    pub approval_required: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Configuration {
    pub urls: Value,
    pub accounts: Value,
    pub statuses: StatusConfiguration,
    pub media_attachments: MediaConfiguration,
    pub polls: Value,
    pub translation: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusConfiguration {
    pub max_characters: u32,
    pub max_media_attachments: usize,
    pub characters_reserved_per_url: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaConfiguration {
    pub supported_mime_types: Vec<String>,
    pub image_size_limit: u64,
    pub image_matrix_limit: u64,
    pub video_size_limit: u64,
    pub video_frame_rate_limit: u32,
    pub video_matrix_limit: u64,
}
