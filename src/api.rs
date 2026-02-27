//! Gmail API v1 implementation.
//!
//! All API calls go through the host's HTTP capability, which handles
//! credential injection and rate limiting. The WASM tool never sees
//! the actual OAuth token.

use crate::near::agent::host;
use crate::types::*;

const GMAIL_API_BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";

/// Make a Gmail API call.
fn api_call(method: &str, path: &str, body: Option<&str>) -> Result<String, String> {
    let url = format!("{}/{}", GMAIL_API_BASE, path);

    let headers = if body.is_some() {
        r#"{"Content-Type": "application/json"}"#
    } else {
        "{}"
    };

    let body_bytes = body.map(|b| b.as_bytes().to_vec());

    host::log(
        host::LogLevel::Debug,
        &format!("Gmail API: {} {}", method, path),
    );

    let response = host::http_request(method, &url, headers, body_bytes.as_deref(), None)?;

    if response.status < 200 || response.status >= 300 {
        let body_text = String::from_utf8_lossy(&response.body);
        return Err(format!(
            "Gmail API returned status {}: {}",
            response.status, body_text
        ));
    }

    if response.body.is_empty() {
        return Ok(String::new());
    }

    String::from_utf8(response.body).map_err(|e| format!("Invalid UTF-8 in response: {}", e))
}

/// Extract a header value from a Gmail message payload.
fn get_header(payload: &serde_json::Value, name: &str) -> String {
    payload["headers"]
        .as_array()
        .and_then(|headers| {
            headers.iter().find(|h| {
                h["name"]
                    .as_str()
                    .map(|n| n.eq_ignore_ascii_case(name))
                    .unwrap_or(false)
            })
        })
        .and_then(|h| h["value"].as_str())
        .unwrap_or("")
        .to_string()
}

/// Extract plain text body from a Gmail message payload.
/// Walks the MIME parts tree to find text/plain content.
fn extract_body(payload: &serde_json::Value) -> String {
    // Try direct body first (simple messages)
    if let Some(data) = payload["body"]["data"].as_str() {
        if let Some(decoded) = base64url_decode(data) {
            return decoded;
        }
    }

    // Walk parts for multipart messages
    if let Some(parts) = payload["parts"].as_array() {
        for part in parts {
            let mime_type = part["mimeType"].as_str().unwrap_or("");

            if mime_type == "text/plain" {
                if let Some(data) = part["body"]["data"].as_str() {
                    if let Some(decoded) = base64url_decode(data) {
                        return decoded;
                    }
                }
            }

            // Recurse into nested parts (e.g., multipart/alternative inside multipart/mixed)
            if mime_type.starts_with("multipart/") {
                let nested = extract_body(part);
                if !nested.is_empty() {
                    return nested;
                }
            }
        }

        // Fall back to text/html if no text/plain found
        for part in parts {
            if part["mimeType"].as_str() == Some("text/html") {
                if let Some(data) = part["body"]["data"].as_str() {
                    if let Some(decoded) = base64url_decode(data) {
                        return decoded;
                    }
                }
            }
        }
    }

    String::new()
}

/// Parse a full message from the API response.
fn parse_message(v: &serde_json::Value) -> Message {
    let payload = &v["payload"];
    let label_ids: Vec<String> = v["labelIds"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|l| l.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Message {
        id: v["id"].as_str().unwrap_or("").to_string(),
        thread_id: v["threadId"].as_str().unwrap_or("").to_string(),
        subject: get_header(payload, "Subject"),
        from: get_header(payload, "From"),
        to: get_header(payload, "To"),
        cc: {
            let cc = get_header(payload, "Cc");
            if cc.is_empty() {
                None
            } else {
                Some(cc)
            }
        },
        date: get_header(payload, "Date"),
        body: extract_body(payload),
        snippet: v["snippet"].as_str().unwrap_or("").to_string(),
        is_unread: label_ids.iter().any(|l| l == "UNREAD"),
        label_ids,
    }
}

/// Gmail API maximum for maxResults parameter.
const MAX_RESULTS_LIMIT: u32 = 500;

/// List messages in the mailbox.
pub fn list_messages(
    query: Option<&str>,
    max_results: u32,
    label_ids: &[String],
) -> Result<ListMessagesResult, String> {
    let clamped = max_results.min(MAX_RESULTS_LIMIT);
    let mut params = vec![format!("maxResults={}", clamped)];

    if let Some(q) = query {
        params.push(format!("q={}", url_encode(q)));
    }
    for label in label_ids {
        params.push(format!("labelIds={}", url_encode(label)));
    }

    let path = format!("messages?{}", params.join("&"));
    let response = api_call("GET", &path, None)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&response).map_err(|e| format!("Failed to parse response: {}", e))?;

    let result_size_estimate = parsed["resultSizeEstimate"].as_u64().unwrap_or(0) as u32;

    // The list endpoint only returns message IDs and thread IDs.
    // We need to fetch each message to get summaries.
    let message_ids: Vec<String> = parsed["messages"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let mut messages = Vec::new();
    for id in &message_ids {
        // Fetch metadata format (lighter than full) for list view.
        // Must specify metadataHeaders to get headers back from the API.
        let msg_path = format!(
            "messages/{}?format=metadata\
             &metadataHeaders=Subject&metadataHeaders=From&metadataHeaders=To&metadataHeaders=Date\
             &fields=id,threadId,labelIds,snippet,payload/headers",
            url_encode(id)
        );
        match api_call("GET", &msg_path, None) {
            Ok(msg_response) => {
                if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&msg_response) {
                    let payload = &msg["payload"];
                    let label_ids: Vec<String> = msg["labelIds"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|l| l.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();

                    messages.push(MessageSummary {
                        id: msg["id"].as_str().unwrap_or("").to_string(),
                        thread_id: msg["threadId"].as_str().unwrap_or("").to_string(),
                        subject: get_header(payload, "Subject"),
                        from: get_header(payload, "From"),
                        to: get_header(payload, "To"),
                        date: get_header(payload, "Date"),
                        snippet: msg["snippet"].as_str().unwrap_or("").to_string(),
                        is_unread: label_ids.iter().any(|l| l == "UNREAD"),
                        label_ids,
                    });
                }
            }
            Err(e) => {
                host::log(
                    host::LogLevel::Warn,
                    &format!("Failed to fetch message {}: {}", id, e),
                );
            }
        }
    }

    Ok(ListMessagesResult {
        messages,
        result_size_estimate,
        next_page_token: parsed["nextPageToken"].as_str().map(|s| s.to_string()),
    })
}

/// Get a specific message with full content.
pub fn get_message(message_id: &str) -> Result<Message, String> {
    let path = format!("messages/{}?format=full", url_encode(message_id));
    let response = api_call("GET", &path, None)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&response).map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(parse_message(&parsed))
}

/// Gmail system labels that cannot be added or removed via modify_message.
const BLOCKED_LABELS: &[&str] = &[
    "INBOX", "SENT", "DRAFT", "SPAM", "TRASH",
    "CATEGORY_PERSONAL", "CATEGORY_SOCIAL", "CATEGORY_PROMOTIONS",
    "CATEGORY_UPDATES", "CATEGORY_FORUMS",
    "CHAT",
];

/// Check if a label is a blocked system label (case-insensitive).
fn is_blocked_label(label: &str) -> bool {
    let upper = label.to_uppercase();
    BLOCKED_LABELS.iter().any(|&b| b == upper)
}

/// Modify a message's labels.
///
/// Only user-created labels and safe system labels (UNREAD, STARRED, IMPORTANT)
/// are allowed. Structural system labels (INBOX, SENT, SPAM, TRASH, etc.) are blocked.
pub fn modify_message(
    message_id: &str,
    add_label_ids: &[String],
    remove_label_ids: &[String],
) -> Result<ModifyResult, String> {
    // Reject empty requests
    if add_label_ids.is_empty() && remove_label_ids.is_empty() {
        return Err("At least one of add_label_ids or remove_label_ids must be provided.".to_string());
    }

    // Validate labels against blocklist
    for label in add_label_ids.iter().chain(remove_label_ids.iter()) {
        if is_blocked_label(label) {
            return Err(format!(
                "Cannot modify system label '{}'. Only user labels and UNREAD/STARRED/IMPORTANT are allowed.",
                label
            ));
        }
    }

    // Audit log: record what is being changed
    host::log(
        host::LogLevel::Info,
        &format!(
            "modify_message: id={}, add={:?}, remove={:?}",
            message_id, add_label_ids, remove_label_ids
        ),
    );

    let path = format!("messages/{}/modify", url_encode(message_id));
    let payload = serde_json::json!({
        "addLabelIds": add_label_ids,
        "removeLabelIds": remove_label_ids,
    });
    let body_str = serde_json::to_string(&payload).map_err(|e| e.to_string())?;

    let response = api_call("POST", &path, Some(&body_str))?;
    let parsed: serde_json::Value =
        serde_json::from_str(&response).map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(ModifyResult {
        id: parsed["id"].as_str().unwrap_or("").to_string(),
        label_ids: parsed["labelIds"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|l| l.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

// ==================== Encoding Utilities ====================

/// Base64url-decode a string. Returns None on invalid input.
fn base64url_decode(input: &str) -> Option<String> {
    let input = input.trim_end_matches('=');
    let mut bytes = Vec::with_capacity(input.len() * 3 / 4);

    let mut buf: u32 = 0;
    let mut bits: u32 = 0;

    for c in input.bytes() {
        let val = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            b'+' => 62, // accept standard base64 too
            b'/' => 63,
            b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        };

        buf = (buf << 6) | val as u32;
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            bytes.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    String::from_utf8(bytes).ok()
}

/// Minimal percent-encoding for URL path segments and query values.
fn url_encode(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            _ => {
                encoded.push('%');
                encoded.push(char::from(HEX[(b >> 4) as usize]));
                encoded.push(char::from(HEX[(b & 0x0F) as usize]));
            }
        }
    }
    encoded
}

const HEX: [u8; 16] = *b"0123456789ABCDEF";