//! Keymap: translate raw `KeyEvent`s into high-level `KeyAction`s.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Currently focused pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    MessageList,
    Reader,
}

/// High-level mode of the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Compose,
    Search,
    Help,
    Onboarding,
    Settings,
    Accounts,
}

/// Field focus inside the onboarding modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardingField {
    DisplayName,
    Email,
    ImapHost,
    ImapPort,
    ImapTls,
    SmtpHost,
    SmtpPort,
    SmtpTls,
    Username,
    Password,
}

impl OnboardingField {
    /// Cycle to the next onboarding field.
    pub fn next(self) -> Self {
        match self {
            Self::DisplayName => Self::Email,
            Self::Email => Self::ImapHost,
            Self::ImapHost => Self::ImapPort,
            Self::ImapPort => Self::ImapTls,
            Self::ImapTls => Self::SmtpHost,
            Self::SmtpHost => Self::SmtpPort,
            Self::SmtpPort => Self::SmtpTls,
            Self::SmtpTls => Self::Username,
            Self::Username => Self::Password,
            Self::Password => Self::DisplayName,
        }
    }
    /// Cycle to the previous onboarding field.
    pub fn prev(self) -> Self {
        match self {
            Self::DisplayName => Self::Password,
            Self::Email => Self::DisplayName,
            Self::ImapHost => Self::Email,
            Self::ImapPort => Self::ImapHost,
            Self::ImapTls => Self::ImapPort,
            Self::SmtpHost => Self::ImapTls,
            Self::SmtpPort => Self::SmtpHost,
            Self::SmtpTls => Self::SmtpPort,
            Self::Username => Self::SmtpTls,
            Self::Password => Self::Username,
        }
    }
}

/// Compose-mode focused field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeField {
    From,
    To,
    Cc,
    Bcc,
    Subject,
    Body,
    Attachments,
}

impl ComposeField {
    /// Cycle to the next field forward.
    pub fn next(self) -> Self {
        match self {
            Self::From => Self::To,
            Self::To => Self::Cc,
            Self::Cc => Self::Bcc,
            Self::Bcc => Self::Subject,
            Self::Subject => Self::Body,
            Self::Body => Self::Attachments,
            Self::Attachments => Self::From,
        }
    }
    /// Cycle to the previous field.
    pub fn prev(self) -> Self {
        match self {
            Self::From => Self::Attachments,
            Self::To => Self::From,
            Self::Cc => Self::To,
            Self::Bcc => Self::Cc,
            Self::Subject => Self::Bcc,
            Self::Body => Self::Subject,
            Self::Attachments => Self::Body,
        }
    }
}

/// High-level intent extracted from a key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Quit,
    Help,
    FocusNext,
    FocusPrev,
    Up,
    Down,
    PageUp,
    PageDown,
    Top,
    Bottom,
    OpenMessage,
    BackToList,
    Compose,
    Reply,
    ReplyAll,
    Forward,
    Delete,
    ToggleFlag,
    MarkRead,
    Search,
    GotoFolder,
    NextAccount,
    PrevAccount,
    Send,
    SaveDraft,
    CancelCompose,
    AddAttachment,
    OpenOnboarding,
    SaveOnboarding,
    CancelOnboarding,
    OnboardingCycleLeft,
    OnboardingCycleRight,
    OpenHtmlInBrowser,
    Refresh,
    OpenSettings,
    SaveSettings,
    CancelSettings,
    SettingsToggle,
    OpenAccounts,
    CloseAccounts,
    AccountsEdit,
    AccountsDelete,
    AccountsAdd,
}

/// Translate a key event to a `KeyAction` in normal mode (compose mode handled separately).
pub fn map_key(focus: Focus, mode: Mode, key: KeyEvent) -> Option<KeyAction> {
    match mode {
        Mode::Compose => map_compose(key),
        Mode::Help => map_help(key),
        Mode::Search => map_search(key),
        Mode::Normal => map_normal(focus, key),
        Mode::Onboarding => map_onboarding(key),
        Mode::Settings => map_settings(key),
        Mode::Accounts => map_accounts(key),
    }
}

fn map_settings(key: KeyEvent) -> Option<KeyAction> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('s') if ctrl => Some(KeyAction::SaveSettings),
        KeyCode::Esc => Some(KeyAction::CancelSettings),
        KeyCode::Tab => Some(KeyAction::FocusNext),
        KeyCode::BackTab => Some(KeyAction::FocusPrev),
        KeyCode::Char(' ') | KeyCode::Enter => Some(KeyAction::SettingsToggle),
        _ => None,
    }
}

fn map_accounts(key: KeyEvent) -> Option<KeyAction> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Some(KeyAction::CloseAccounts),
        KeyCode::Up | KeyCode::Char('k') => Some(KeyAction::Up),
        KeyCode::Down | KeyCode::Char('j') => Some(KeyAction::Down),
        KeyCode::Enter | KeyCode::Char('e') => Some(KeyAction::AccountsEdit),
        KeyCode::Char('d') | KeyCode::Delete => Some(KeyAction::AccountsDelete),
        KeyCode::Char('a') | KeyCode::Char('A') => Some(KeyAction::AccountsAdd),
        _ => None,
    }
}

fn map_normal(focus: Focus, key: KeyEvent) -> Option<KeyAction> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    match key.code {
        KeyCode::Char('q') => Some(KeyAction::Quit),
        KeyCode::Char('?') => Some(KeyAction::Help),
        KeyCode::Tab => Some(KeyAction::FocusNext),
        KeyCode::BackTab => Some(KeyAction::FocusPrev),
        KeyCode::Char('j') | KeyCode::Down => Some(KeyAction::Down),
        KeyCode::Char('k') | KeyCode::Up => Some(KeyAction::Up),
        KeyCode::PageDown | KeyCode::Char('d') if ctrl => Some(KeyAction::PageDown),
        KeyCode::PageUp | KeyCode::Char('u') if ctrl => Some(KeyAction::PageUp),
        KeyCode::PageDown => Some(KeyAction::PageDown),
        KeyCode::PageUp => Some(KeyAction::PageUp),
        KeyCode::Char('g') => Some(KeyAction::Top),
        KeyCode::Char('G') => Some(KeyAction::Bottom),
        KeyCode::Enter => Some(KeyAction::OpenMessage),
        KeyCode::Esc => Some(KeyAction::BackToList),
        KeyCode::Char('c') => Some(KeyAction::Compose),
        KeyCode::Char('r') if !shift => Some(KeyAction::Reply),
        KeyCode::Char('R') => Some(KeyAction::ReplyAll),
        KeyCode::Char('f') => Some(KeyAction::Forward),
        KeyCode::Char('d') => Some(KeyAction::Delete),
        KeyCode::Char('s') => Some(KeyAction::ToggleFlag),
        KeyCode::Char('m') => Some(KeyAction::MarkRead),
        KeyCode::Char('/') => Some(KeyAction::Search),
        KeyCode::Char('}') => Some(KeyAction::NextAccount),
        KeyCode::Char('{') => Some(KeyAction::PrevAccount),
        KeyCode::Char('A') if focus == Focus::Sidebar => Some(KeyAction::OpenOnboarding),
        KeyCode::Char('o') => Some(KeyAction::OpenHtmlInBrowser),
        KeyCode::F(5) => Some(KeyAction::Refresh),
        KeyCode::Char('r') if ctrl => Some(KeyAction::Refresh),
        KeyCode::Char(',') => Some(KeyAction::OpenSettings),
        KeyCode::Char('M') => Some(KeyAction::OpenAccounts),
        _ => None,
    }
}

fn map_compose(key: KeyEvent) -> Option<KeyAction> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('s') if ctrl => Some(KeyAction::Send),
        KeyCode::Char('d') if ctrl => Some(KeyAction::SaveDraft),
        KeyCode::Char('a') if ctrl => Some(KeyAction::AddAttachment),
        KeyCode::Esc => Some(KeyAction::CancelCompose),
        KeyCode::Tab => Some(KeyAction::FocusNext),
        KeyCode::BackTab => Some(KeyAction::FocusPrev),
        _ => None,
    }
}

fn map_help(key: KeyEvent) -> Option<KeyAction> {
    match key.code {
        KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => Some(KeyAction::Help),
        _ => None,
    }
}

fn map_search(key: KeyEvent) -> Option<KeyAction> {
    match key.code {
        KeyCode::Esc => Some(KeyAction::BackToList),
        KeyCode::Enter => Some(KeyAction::OpenMessage),
        _ => None,
    }
}

fn map_onboarding(key: KeyEvent) -> Option<KeyAction> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('s') if ctrl => Some(KeyAction::SaveOnboarding),
        KeyCode::Esc => Some(KeyAction::CancelOnboarding),
        KeyCode::Tab => Some(KeyAction::FocusNext),
        KeyCode::BackTab => Some(KeyAction::FocusPrev),
        KeyCode::Left => Some(KeyAction::OnboardingCycleLeft),
        KeyCode::Right => Some(KeyAction::OnboardingCycleRight),
        _ => None,
    }
}
