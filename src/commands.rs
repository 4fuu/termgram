//! Finite application commands, with explicit aliases and argument contracts.
//! This is an adapter to actions, not an Ex or shell interpreter.

use crate::actions::Action;

#[derive(Clone, Debug)]
pub enum Kind {
    Action(Action),
    Help,
    Chat,
    Folder,
    Account,
    Search,
    Pin(bool),
    Archive(bool),
    Sidebar,
    Color,
    Status,
    Attach,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    None,
    Chat,
    Conversation,
    Message,
    Attachment,
    Preview,
}

pub struct Spec {
    pub name: &'static str,
    pub alias: Option<&'static str>,
    pub arguments: &'static str,
    pub kind: Kind,
    pub target: Target,
    description: &'static str,
}

impl Spec {
    #[must_use]
    pub fn description(&self) -> &str {
        if let Kind::Action(action) = &self.kind {
            action.description()
        } else {
            self.description
        }
    }

    #[must_use]
    pub fn usage(&self) -> String {
        format!(
            "{}{}{}",
            self.name,
            if self.arguments.is_empty() { "" } else { " " },
            self.arguments
        )
    }

    #[must_use]
    pub fn action(&self) -> Option<Action> {
        match &self.kind {
            Kind::Action(action) => Some(action.clone()),
            Kind::Chat | Kind::Status => None,
            Kind::Help => Some(Action::CommandLine),
            Kind::Folder => Some(Action::FolderNext),
            Kind::Account => Some(Action::Accounts),
            Kind::Search => Some(Action::Search),
            Kind::Pin(_) => Some(Action::Pin),
            Kind::Archive(_) => Some(Action::Archive),
            Kind::Sidebar => Some(Action::ToggleSidebar),
            Kind::Color => Some(Action::ChatColor),
            Kind::Attach => Some(Action::Attach),
        }
    }
}

macro_rules! command {
    ($name:literal, $alias:expr, $arguments:literal, $kind:expr, $target:ident, $description:literal) => {
        Spec {
            name: $name,
            alias: $alias,
            arguments: $arguments,
            kind: $kind,
            target: Target::$target,
            description: $description,
        }
    };
}

pub static COMMANDS: &[Spec] = &[
    command!(
        "attach",
        None,
        "<paths...>",
        Kind::Attach,
        Chat,
        "Add files to this chat's draft without sending"
    ),
    command!(
        "attachments",
        None,
        "",
        Kind::Action(Action::Attachments),
        Conversation,
        ""
    ),
    command!(
        "help",
        Some("h"),
        "[command]",
        Kind::Help,
        None,
        "Browse commands, usage and availability"
    ),
    command!(
        "chat",
        None,
        "<alias, ID or title>",
        Kind::Chat,
        None,
        "Open a cached chat in this account"
    ),
    command!(
        "folder",
        None,
        "<ID or name>",
        Kind::Folder,
        None,
        "Switch Telegram folders, including Archive"
    ),
    command!(
        "account",
        None,
        "[slot]",
        Kind::Account,
        None,
        "Choose or switch Telegram accounts"
    ),
    command!(
        "search",
        None,
        "[regex]",
        Kind::Search,
        None,
        "Search cached messages; keep the pattern verbatim"
    ),
    command!(
        "latest",
        None,
        "",
        Kind::Action(Action::Latest),
        Conversation,
        ""
    ),
    command!("reply", None, "", Kind::Action(Action::Reply), Message, ""),
    command!(
        "preview",
        None,
        "",
        Kind::Action(Action::Preview),
        Preview,
        ""
    ),
    command!(
        "reveal",
        None,
        "",
        Kind::Action(Action::Reveal),
        Attachment,
        ""
    ),
    command!(
        "pins",
        None,
        "",
        Kind::Action(Action::Pins),
        Conversation,
        ""
    ),
    command!(
        "pin",
        None,
        "<chat or message>",
        Kind::Pin(true),
        Chat,
        "Pin the specified kind of target"
    ),
    command!(
        "unpin",
        None,
        "<chat or message>",
        Kind::Pin(false),
        Chat,
        "Unpin the specified kind of target"
    ),
    command!(
        "archive",
        None,
        "",
        Kind::Archive(true),
        Chat,
        "Move this chat to Archive"
    ),
    command!(
        "unarchive",
        None,
        "",
        Kind::Archive(false),
        Chat,
        "Move this chat out of Archive"
    ),
    command!(
        "sidebar",
        None,
        "[show, hide or toggle]",
        Kind::Sidebar,
        None,
        "Control sidebar visibility"
    ),
    command!(
        "color",
        None,
        "<chat or folder>",
        Kind::Color,
        None,
        "Choose the target's color"
    ),
    command!(
        "settings",
        None,
        "",
        Kind::Action(Action::Settings),
        None,
        ""
    ),
    command!(
        "status",
        None,
        "",
        Kind::Status,
        None,
        "Inspect account, connection, DC and synchronization"
    ),
    command!("refresh", None, "", Kind::Action(Action::Refresh), None, ""),
    command!("quit", Some("q"), "", Kind::Action(Action::Quit), None, ""),
];

#[must_use]
pub fn find(name: &str) -> Option<&'static Spec> {
    COMMANDS
        .iter()
        .find(|spec| spec.name == name || spec.alias == Some(name))
}

/// Only the first whitespace separates a name from its raw argument. In
/// particular, regex backslashes and trailing spaces are never shell-decoded.
#[must_use]
pub fn split(input: &str) -> (&str, Option<&str>) {
    let input = input.trim_start();
    input
        .char_indices()
        .find(|(_, character)| character.is_whitespace())
        .map_or((input, None), |(end, separator)| {
            (&input[..end], Some(&input[end + separator.len_utf8()..]))
        })
}
