//! Top menu bar (app functions, with dropdowns). Entries map to existing
//! `KeyAction`s so the menus reuse the same handlers as the keyboard shortcuts.
//! Email actions (compose/reply/...) live in the footer, not a second bar.

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

/// Interactive state of the menu bar while in `Mode::Menu`.
#[derive(Debug, Clone, Copy)]
pub struct MenuState {
    /// Selected top-level menu.
    pub col: usize,
    /// Whether the current menu's dropdown is open.
    pub open: bool,
    /// Selected item within the open dropdown.
    pub item: usize,
}

impl MenuState {
    pub fn new() -> Self {
        Self { col: 0, open: false, item: 0 }
    }
}

impl Default for MenuState {
    fn default() -> Self {
        Self::new()
    }
}
