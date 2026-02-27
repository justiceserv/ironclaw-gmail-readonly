//! Types for Gmail API requests and responses.

use serde::{Deserialize, Serialize};

/// Input parameters for the Gmail tool.
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum GmailAction {
    /// List messages in the mailbox.
    ListMessages {
        /// Gmail search query (same syntax as the Gmail search box).
        /// Examples: "from:alice@example.com", "subject:meeting", "is:unread",
        /// "after:2025/01/01 before:2025/02/01".
        #[serde(default)]
        query: Option<String>,
        /// Maximum number of messages to return (default: 20).
        #[serde(default = "default_max_results")]
        max_results: u32,
        /// Label IDs to filter by (e.g., "INBOX", "SENT", "DRAFT").
        #[serde(default)]
        label_ids: Vec<String>,
    },

    /// Get a specific message with full content.
    GetMessage {
        /// The message ID.
        message_id: String,
    },

    /// Modify a message's labels (add/remove labels, mark as read/unread, etc.).
    ModifyMessage {
        /// The message ID.
        message_id: String,
        /// Label IDs to add (e.g., "STARRED", "IMPORTANT").
        #[serde(default)]
        add_label_ids: Vec<String>,
        /// Label IDs to remove (e.g., "UNREAD", "INBOX").
        #[serde(default)]
        remove_label_ids: Vec<String>,
    },
}

fn default_max_results() -> u32 {
    20
}

/// Permission level for this tool instance, injected via request context by the host.
///
/// # Permission Levels
///
/// - `read_only`: Can only list and read messages (list_messages, get_message).
/// - `read_and_mark`: Read + mark as read/unread (modify UNREAD label only).
/// - `read_and_labels`: Read + full label management (modify any allowed label).
///
/// Defaults to `read_only` if not provided in context.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionLevel {
    ReadOnly,
    ReadAndMark,
    ReadAndLabels,
}

impl Default for PermissionLevel {
    fn default() -> Self {
        PermissionLevel::ReadOnly
    }
}

/// Context injected by the host when launching this tool instance.
#[derive(Debug, Deserialize, Default)]
pub struct ToolContext {
    #[serde(default)]
    pub permission: PermissionLevel,
}

/// A Gmail message summary (from list endpoint).
#[derive(Debug, Serialize)]
pub struct MessageSummary {
    pub id: String,
    pub thread_id: String,
    pub subject: String,
    pub from: String,
    pub to: String,
    pub date: String,
    pub snippet: String,
    pub label_ids: Vec<String>,
    pub is_unread: bool,
}

/// A full Gmail message (from get endpoint).
#[derive(Debug, Serialize)]
pub struct Message {
    pub id: String,
    pub thread_id: String,
    pub subject: String,
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<String>,
    pub date: String,
    pub body: String,
    pub snippet: String,
    pub label_ids: Vec<String>,
    pub is_unread: bool,
}

/// Result from modify_message.
#[derive(Debug, Serialize)]
pub struct ModifyResult {
    pub id: String,
    pub label_ids: Vec<String>,
}

/// Result from list_messages.
#[derive(Debug, Serialize)]
pub struct ListMessagesResult {
    pub messages: Vec<MessageSummary>,
    pub result_size_estimate: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
}

