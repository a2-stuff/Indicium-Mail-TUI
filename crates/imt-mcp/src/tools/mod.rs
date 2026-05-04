pub mod accounts;
pub mod folders;
pub mod messages;
pub mod search;
pub mod compose;
pub mod flags;
pub mod move_;

pub fn all_tools() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "name": "list_accounts",
            "description": "List all configured email accounts.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        }),
        serde_json::json!({
            "name": "list_folders",
            "description": "List all folders for a given account.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id": {
                        "type": "string",
                        "description": "The UUID of the account to list folders for."
                    }
                },
                "required": ["account_id"]
            }
        }),
        serde_json::json!({
            "name": "list_messages",
            "description": "List messages in a folder with optional pagination.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id": {
                        "type": "string",
                        "description": "The UUID of the account."
                    },
                    "folder_id": {
                        "type": "string",
                        "description": "The UUID of the folder to list messages from."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of messages to return (default 50)."
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Number of messages to skip for pagination (default 0)."
                    }
                },
                "required": ["account_id", "folder_id"]
            }
        }),
        serde_json::json!({
            "name": "read_message",
            "description": "Fetch and return the full body of a message, fetching from IMAP if not yet cached locally.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "description": "The UUID of the message to read."
                    }
                },
                "required": ["message_id"]
            }
        }),
        serde_json::json!({
            "name": "search",
            "description": "Full-text search across messages, optionally scoped to an account.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query string."
                    },
                    "account_id": {
                        "type": "string",
                        "description": "Optional UUID of the account to scope the search to."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default 25)."
                    }
                },
                "required": ["query"]
            }
        }),
        serde_json::json!({
            "name": "send",
            "description": "Compose and send a new email message via SMTP.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id": {
                        "type": "string",
                        "description": "The UUID of the account to send from."
                    },
                    "to": {
                        "type": "string",
                        "description": "Comma-separated list of recipient email addresses."
                    },
                    "subject": {
                        "type": "string",
                        "description": "The email subject line."
                    },
                    "body": {
                        "type": "string",
                        "description": "The plain-text body of the email."
                    },
                    "cc": {
                        "type": "string",
                        "description": "Optional comma-separated list of CC recipients."
                    },
                    "bcc": {
                        "type": "string",
                        "description": "Optional comma-separated list of BCC recipients."
                    }
                },
                "required": ["account_id", "to", "subject", "body"]
            }
        }),
        serde_json::json!({
            "name": "reply",
            "description": "Reply to an existing message, optionally replying to all recipients.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "description": "The UUID of the message to reply to."
                    },
                    "body": {
                        "type": "string",
                        "description": "The plain-text body of the reply."
                    },
                    "reply_all": {
                        "type": "boolean",
                        "description": "Whether to reply to all recipients (default false)."
                    }
                },
                "required": ["message_id", "body"]
            }
        }),
        serde_json::json!({
            "name": "mark_read",
            "description": "Mark a message as read or unread.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "description": "The UUID of the message to update."
                    },
                    "read": {
                        "type": "boolean",
                        "description": "Whether to mark the message as read (default true)."
                    }
                },
                "required": ["message_id"]
            }
        }),
        serde_json::json!({
            "name": "toggle_flag",
            "description": "Toggle the starred/flagged status of a message.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "description": "The UUID of the message to toggle the flag on."
                    }
                },
                "required": ["message_id"]
            }
        }),
        serde_json::json!({
            "name": "move_message",
            "description": "Move a message to a different folder.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "description": "The UUID of the message to move."
                    },
                    "folder_id": {
                        "type": "string",
                        "description": "The UUID of the destination folder."
                    }
                },
                "required": ["message_id", "folder_id"]
            }
        }),
        serde_json::json!({
            "name": "delete_message",
            "description": "Delete a message by moving it to the Trash folder.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "description": "The UUID of the message to delete."
                    }
                },
                "required": ["message_id"]
            }
        }),
        serde_json::json!({
            "name": "sync_folder",
            "description": "Trigger an explicit one-shot IMAP sync for a specific folder.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "account_id": {
                        "type": "string",
                        "description": "The UUID of the account that owns the folder."
                    },
                    "folder_id": {
                        "type": "string",
                        "description": "The UUID of the folder to sync."
                    }
                },
                "required": ["account_id", "folder_id"]
            }
        }),
    ]
}
