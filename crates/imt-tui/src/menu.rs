//! Top menu bar (app functions, with dropdowns) and the email-actions bar.
//! Entries map to existing `KeyAction`s so the menus reuse the same handlers
//! as the keyboard shortcuts.

use crate::keymap::KeyAction;

/// A selectable entry that runs an action (dropdown item or action-bar button).
pub struct MenuItem {
    pub label: &'static str,
    pub key_hint: &'static str,
    pub action: KeyAction,
}

/// A top-level menu-bar entry. With `items`, Enter/Down opens a dropdown;
/// otherwise `action` runs directly.
pub struct TopMenu {
    pub label: &'static str,
    pub items: &'static [MenuItem],
    pub action: Option<KeyAction>,
}

/// Row 0 - application menus / functions.
pub const MENUS: &[TopMenu] = &[
    TopMenu {
        label: "Account",
        items: &[
            MenuItem { label: "Add Account",     key_hint: "A",  action: KeyAction::OpenOnboarding },
            MenuItem { label: "Manage Accounts", key_hint: "m",  action: KeyAction::OpenAccounts },
            MenuItem { label: "Refresh",         key_hint: "F5", action: KeyAction::Refresh },
        ],
        action: None,
    },
    TopMenu { label: "Settings", items: &[], action: Some(KeyAction::OpenSettings) },
    TopMenu { label: "Info",     items: &[], action: Some(KeyAction::OpenInfo) },
    TopMenu { label: "Help",     items: &[], action: Some(KeyAction::Help) },
    TopMenu { label: "Quit",     items: &[], action: Some(KeyAction::Quit) },
];

/// Row 1 - email actions for the selected message.
pub const ACTIONS: &[MenuItem] = &[
    MenuItem { label: "Compose",     key_hint: "c", action: KeyAction::Compose },
    MenuItem { label: "Reply",       key_hint: "r", action: KeyAction::Reply },
    MenuItem { label: "Reply All",   key_hint: "R", action: KeyAction::ReplyAll },
    MenuItem { label: "Forward",     key_hint: "f", action: KeyAction::Forward },
    MenuItem { label: "Mark Read",   key_hint: "u", action: KeyAction::ToggleRead },
    MenuItem { label: "Flag",        key_hint: "s", action: KeyAction::ToggleFlag },
    MenuItem { label: "Attachments", key_hint: "a", action: KeyAction::OpenAttachments },
    MenuItem { label: "Move",        key_hint: "v", action: KeyAction::OpenMoveModal },
    MenuItem { label: "Delete",      key_hint: "d", action: KeyAction::Delete },
];

/// Interactive state of the menu/actions bars while in `Mode::Menu`.
#[derive(Debug, Clone, Copy)]
pub struct MenuState {
    /// 0 = top menu bar, 1 = actions bar.
    pub row: u8,
    /// Selected column within the current row.
    pub col: usize,
    /// Whether the current top-menu dropdown is open (row 0 only).
    pub open: bool,
    /// Selected item within the open dropdown.
    pub item: usize,
}

impl MenuState {
    pub fn new() -> Self {
        Self { row: 0, col: 0, open: false, item: 0 }
    }
}

impl Default for MenuState {
    fn default() -> Self {
        Self::new()
    }
}
