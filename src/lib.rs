//! Gmail WASM Tool for IronClaw.
//!
//! Provides Gmail integration for reading, searching, and managing email labels.
//!
//! # Capabilities Required
//!
//! - HTTP: `gmail.googleapis.com/gmail/v1/*` (GET, POST)
//! - Secrets: `google_oauth_token` (shared OAuth 2.0 token, injected automatically)
//!
//! # Supported Actions
//!
//! - `list_messages`: List/search messages with Gmail query syntax
//! - `get_message`: Get a specific message with full content
//! - `modify_message`: Modify labels (mark read/unread, add/remove labels)
//!
//! # Permission Levels (via request context)
//!
//! The host injects a permission level per instance via `request.context`:
//!
//! - `read_only` (default): Only list_messages and get_message allowed.
//! - `read_and_mark`: Read + mark as read/unread (UNREAD label only).
//! - `read_and_labels`: Read + full label management.
//!
//! ```json
//! {"permission": "read_and_mark"}
//! ```

mod api;
mod types;

use types::{GmailAction, PermissionLevel, ToolContext};

wit_bindgen::generate!({
    world: "sandboxed-tool",
    path: "../../wit/tool.wit",
});

struct GmailTool;

impl exports::near::agent::tool::Guest for GmailTool {
    fn execute(req: exports::near::agent::tool::Request) -> exports::near::agent::tool::Response {
        match execute_inner(&req.params, req.context.as_deref()) {
            Ok(result) => exports::near::agent::tool::Response {
                output: Some(result),
                error: None,
            },
            Err(e) => exports::near::agent::tool::Response {
                output: None,
                error: Some(e),
            },
        }
    }

    fn schema() -> String {
        r#"{
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list_messages", "get_message", "modify_message"],
                    "description": "The Gmail operation to perform"
                },
                "query": {
                    "type": "string",
                    "description": "Gmail search query (same syntax as Gmail search box, e.g., 'is:unread', 'from:alice@example.com'). Used by: list_messages"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of messages to return (default: 20). Used by: list_messages",
                    "default": 20
                },
                "label_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Label IDs to filter by (e.g., 'INBOX', 'SENT', 'DRAFT'). Used by: list_messages"
                },
                "message_id": {
                    "type": "string",
                    "description": "Message ID. Required for: get_message, modify_message"
                },
                "add_label_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Label IDs to add (e.g., 'STARRED', 'IMPORTANT'). Used by: modify_message"
                },
                "remove_label_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Label IDs to remove (e.g., 'UNREAD', 'INBOX'). Used by: modify_message"
                }
            }
        }"#
        .to_string()
    }

    fn description() -> String {
        "Gmail integration for reading, searching, and managing email labels. \
         Supports Gmail search query syntax (is:unread, from:, subject:, after:, etc.). \
         Can mark messages as read/unread and add/remove labels. \
         Requires a Google OAuth token with gmail.modify scope."
            .to_string()
    }
}

fn execute_inner(params: &str, context: Option<&str>) -> Result<String, String> {
    if !crate::near::agent::host::secret_exists("google_oauth_token") {
        return Err(
            "Google OAuth token not configured. Run `ironclaw tool auth gmail` to set up \
             OAuth, or set the GOOGLE_OAUTH_TOKEN environment variable."
                .to_string(),
        );
    }

    // Parse permission from host-injected context. Defaults to read_only.
    let ctx: ToolContext = context
        .map(|c| serde_json::from_str(c).unwrap_or_default())
        .unwrap_or_default();

    let action: GmailAction =
        serde_json::from_str(params).map_err(|e| format!("Invalid parameters: {}", e))?;

    crate::near::agent::host::log(
        crate::near::agent::host::LogLevel::Info,
        &format!("Executing Gmail action: {:?} (permission: {:?})", action, ctx.permission),
    );

    let result = match action {
        GmailAction::ListMessages {
            query,
            max_results,
            label_ids,
        } => {
            let result = api::list_messages(query.as_deref(), max_results, &label_ids)?;
            serde_json::to_string(&result).map_err(|e| e.to_string())?
        }

        GmailAction::GetMessage { message_id } => {
            let result = api::get_message(&message_id)?;
            serde_json::to_string(&result).map_err(|e| e.to_string())?
        }

        GmailAction::ModifyMessage {
            message_id,
            add_label_ids,
            remove_label_ids,
        } => {
            check_modify_permission(&ctx.permission, &add_label_ids, &remove_label_ids)?;
            let result =
                api::modify_message(&message_id, &add_label_ids, &remove_label_ids)?;
            serde_json::to_string(&result).map_err(|e| e.to_string())?
        }
    };

    Ok(result)
}

/// Check if the current permission level allows the requested label modification.
fn check_modify_permission(
    permission: &PermissionLevel,
    add_label_ids: &[String],
    remove_label_ids: &[String],
) -> Result<(), String> {
    match permission {
        PermissionLevel::ReadOnly => {
            Err("This account is configured as read-only. Label modification is not allowed.".to_string())
        }
        PermissionLevel::ReadAndMark => {
            // Only allow adding/removing "UNREAD" label
            for label in add_label_ids.iter().chain(remove_label_ids.iter()) {
                if label != "UNREAD" {
                    return Err(format!(
                        "This account only allows mark-as-read/unread. Cannot modify label '{}'.",
                        label
                    ));
                }
            }
            Ok(())
        }
        PermissionLevel::ReadAndLabels => Ok(()),
    }
}

export!(GmailTool);