# gmail-tool

A sandboxed WASM component for [IronClaw](https://github.com/nearai/ironclaw) that provides Gmail integration — read emails, search messages, and manage labels with fine-grained permission control.

## Features

- **Search & list** messages using Gmail query syntax (`is:unread`, `from:alice@example.com`, etc.)
- **Read** full message content with MIME multipart handling
- **Modify labels** — mark as read/unread, star, and manage custom labels
- **Permission levels** — host controls what each instance can do (read-only, mark-only, full labels)
- **Sandboxed** — runs as a WASM component; never sees OAuth tokens or raw credentials
- **Hardened** — system labels (INBOX, SPAM, TRASH, etc.) are blocked from modification; HTTP allowlist restricts API endpoints

## Actions

| Action | Description | Permission Required |
|--------|-------------|-------------------|
| `list_messages` | Search/list messages with Gmail query syntax | Any |
| `get_message` | Get full message content (headers + body) | Any |
| `modify_message` | Add/remove labels on a message | `read_and_mark` or `read_and_labels` |

## Usage Examples

```json
// List unread messages
{"action": "list_messages", "query": "is:unread", "max_results": 10}

// Search by sender
{"action": "list_messages", "query": "from:boss@company.com after:2025/01/01"}

// Filter by label
{"action": "list_messages", "label_ids": ["INBOX"], "max_results": 5}

// Read a specific message
{"action": "get_message", "message_id": "18f1a2b3c4d5e6f7"}

// Mark as read
{"action": "modify_message", "message_id": "18f1a2b3c4d5e6f7", "remove_label_ids": ["UNREAD"]}

// Star a message
{"action": "modify_message", "message_id": "18f1a2b3c4d5e6f7", "add_label_ids": ["STARRED"]}

// Add custom label + mark as read in one call
{"action": "modify_message", "message_id": "18f1a2b3c4d5e6f7", "add_label_ids": ["Label_123"], "remove_label_ids": ["UNREAD"]}
```

## Permission Levels

The host injects a permission level per instance via `request.context`:

```json
{"permission": "read_and_mark"}
```

| Level | `list_messages` | `get_message` | `modify_message` |
|-------|:-:|:-:|:-:|
| `read_only` (default) | O | O | X |
| `read_and_mark` | O | O | UNREAD only |
| `read_and_labels` | O | O | O |

When no context is provided, the tool defaults to `read_only`.

## Security

### Label Protection

System labels that affect mail structure are blocked from modification:

`INBOX`, `SENT`, `DRAFT`, `SPAM`, `TRASH`, `CHAT`, `CATEGORY_PERSONAL`, `CATEGORY_SOCIAL`, `CATEGORY_PROMOTIONS`, `CATEGORY_UPDATES`, `CATEGORY_FORUMS`

Allowed system labels: `UNREAD`, `STARRED`, `IMPORTANT` (plus any user-created labels).

### HTTP Allowlist

The capabilities file restricts API access to:

| Method | Allowed Path |
|--------|-------------|
| GET | `/gmail/v1/users/me/messages*` |
| POST | `/gmail/v1/users/me/messages/{id}/modify` only |

This prevents the WASM component from calling send, draft, delete, or any other Gmail endpoints — even if the binary is tampered with.

### Credential Isolation

- OAuth tokens are **never exposed** to the WASM sandbox
- Credentials are injected by the host at the HTTP boundary
- The tool can only check if a secret exists, never read its value
- Multi-account support is achieved via separate WASM instances, each with its own token

## Building

```bash
cargo build --target wasm32-unknown-unknown --release
```

The output WASM component will be at `target/wasm32-unknown-unknown/release/gmail_tool.wasm`.

## Configuration

### OAuth Setup

The tool requires a Google OAuth 2.0 token with the `gmail.modify` scope. Configure these environment variables for the OAuth flow:

| Variable | Description |
|----------|-------------|
| `GOOGLE_OAUTH_CLIENT_ID` | Google OAuth client ID |
| `GOOGLE_OAUTH_CLIENT_SECRET` | Google OAuth client secret |

Then run:

```bash
ironclaw tool auth gmail
```

### Capabilities File

See [`gmail-tool.capabilities.json`](gmail-tool.capabilities.json) for the full capability declaration including HTTP allowlist, rate limits, and OAuth configuration.

## Project Structure

```
src/
  lib.rs     Entry point, action dispatch, permission enforcement
  types.rs   Request/response types, permission levels
  api.rs     Gmail API v1 implementation, label validation
```

## License

MIT
