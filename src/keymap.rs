//! Declarative Lua configuration and contextual chords. Yazi owns terminal key
//! parsing/normalization; this module only maps those keys to Termgram actions.

use anyhow::{Context as _, Result, bail};
use mlua::{HookTriggers, Lua, LuaOptions, LuaSerdeExt, StdLib, VmState};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::Path,
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use yazi_config::keymap::Key;
use yazi_term::event::{KeyEvent, KeyEventKind};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Context {
    Global,
    Chats,
    Conversation,
    Compose,
    Input,
    Overlay,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingSpec {
    context: Context,
    on: Vec<String>,
    run: String,
    #[serde(default)]
    desc: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Configuration {
    keymap: Vec<BindingSpec>,
    chats: BTreeMap<String, i64>,
    ghost_text: Option<String>,
}

#[derive(Clone, Debug)]
struct Binding {
    context: Context,
    keys: Vec<Key>,
    run: String,
    description: String,
}

#[derive(Clone, Debug)]
pub struct Keymap {
    bindings: Vec<Binding>,
    pub chats: BTreeMap<String, i64>,
    pub ghost_text: String,
    pending: Vec<Key>,
    count: usize,
    context: Option<Context>,
    last_key: Option<Instant>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Resolution {
    Action { run: String, count: usize },
    Pending(String),
    Unbound,
}

const ACTIONS: &[&str] = &[
    "quit",
    "help",
    "settings",
    "accounts",
    "next_account",
    "add_account",
    "open",
    "compose",
    "send",
    "newline",
    "cancel",
    "focus",
    "up",
    "down",
    "message_up",
    "message_down",
    "page_up",
    "page_down",
    "oldest",
    "latest",
    "filter",
    "refresh",
    "reply",
    "reply_target",
    "open_link",
    "next_action",
    "previous_action",
    "redraw",
    "home",
    "end",
    "left",
    "right",
    "backspace",
    "delete",
    "clear",
    "delete_word",
    "noop",
    "chat_info",
];

impl Default for Keymap {
    #[allow(clippy::too_many_lines)]
    fn default() -> Self {
        let mut result = Self {
            bindings: Vec::new(),
            chats: BTreeMap::new(),
            ghost_text: "{send} to send".to_owned(),
            pending: Vec::new(),
            count: 0,
            context: None,
            last_key: None,
        };
        for (context, definitions) in [
            (
                Context::Global,
                &[
                    ("<C-c>", "quit"),
                    ("<C-l>", "redraw"),
                    ("<F2>", "next_account"),
                    ("<F3>", "add_account"),
                ][..],
            ),
            (
                Context::Chats,
                &[
                    ("j", "down"),
                    ("k", "up"),
                    ("<Down>", "down"),
                    ("<Up>", "up"),
                    ("<Enter>", "open"),
                    ("<Right>", "open"),
                    ("<Tab>", "focus"),
                    ("/", "filter"),
                    ("q", "quit"),
                    ("?", "help"),
                    ("s", "settings"),
                    ("a", "accounts"),
                    ("<PageUp>", "page_up"),
                    ("<PageDown>", "page_down"),
                    ("G", "latest"),
                    ("g g", "oldest"),
                    ("g i", "chat_info"),
                    ("<C-r>", "refresh"),
                ][..],
            ),
            (
                Context::Conversation,
                &[
                    ("j", "message_down"),
                    ("k", "message_up"),
                    ("[", "message_up"),
                    ("]", "message_down"),
                    ("<Down>", "down"),
                    ("<Up>", "up"),
                    ("<PageUp>", "page_up"),
                    ("<PageDown>", "page_down"),
                    ("G", "latest"),
                    ("<End>", "latest"),
                    ("g g", "oldest"),
                    ("g i", "chat_info"),
                    ("<Home>", "oldest"),
                    ("i", "compose"),
                    ("<Enter>", "open"),
                    ("<Esc>", "cancel"),
                    ("<Left>", "cancel"),
                    ("<Tab>", "focus"),
                    ("q", "quit"),
                    ("?", "help"),
                    ("s", "settings"),
                    ("a", "accounts"),
                    ("o", "next_action"),
                    ("O", "previous_action"),
                    ("R", "reply"),
                    ("r", "reply_target"),
                    ("l", "open_link"),
                    ("<C-r>", "refresh"),
                ][..],
            ),
            (
                Context::Compose,
                &[
                    ("<Enter>", "send"),
                    ("<S-Enter>", "newline"),
                    ("<C-j>", "newline"),
                    ("<Esc>", "cancel"),
                ][..],
            ),
            (
                Context::Input,
                &[
                    ("<Enter>", "open"),
                    ("<Esc>", "cancel"),
                    ("<Up>", "up"),
                    ("<Down>", "down"),
                ][..],
            ),
            (
                Context::Overlay,
                &[
                    ("<Enter>", "open"),
                    ("<Esc>", "cancel"),
                    ("?", "cancel"),
                    ("j", "down"),
                    ("k", "up"),
                    ("<Up>", "up"),
                    ("<Down>", "down"),
                ][..],
            ),
        ] {
            for &(keys, run) in definitions {
                result
                    .insert(BindingSpec {
                        context,
                        on: keys.split(' ').map(str::to_owned).collect(),
                        run: run.to_owned(),
                        desc: None,
                    })
                    .expect("valid builtin binding");
            }
        }
        for context in [Context::Compose, Context::Input] {
            for (key, run) in [
                ("<C-a>", "home"),
                ("<C-e>", "end"),
                ("<C-u>", "clear"),
                ("<C-w>", "delete_word"),
                ("<Home>", "home"),
                ("<End>", "end"),
                ("<Left>", "left"),
                ("<Right>", "right"),
                ("<Backspace>", "backspace"),
                ("<Delete>", "delete"),
            ] {
                result
                    .insert(BindingSpec {
                        context,
                        on: vec![key.to_owned()],
                        run: run.to_owned(),
                        desc: None,
                    })
                    .expect("valid editor binding");
            }
        }
        result
    }
}

impl Keymap {
    /// # Errors
    /// Returns a recoverable configuration error. No partially loaded bindings
    /// are installed, and execution/memory limits keep startup responsive.
    pub fn load(path: &Path) -> Result<Self> {
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error.into()),
        };
        Self::parse(&source)
            .with_context(|| format!("invalid Lua configuration at {}", path.display()))
    }

    /// # Errors
    /// Returns Lua, action, key or chord-conflict errors.
    pub fn parse(source: &str) -> Result<Self> {
        if source.len() > 64 * 1024 {
            bail!("Lua configuration exceeds 64 KiB");
        }
        let lua = Lua::new_with(
            StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8,
            LuaOptions::default(),
        )?;
        lua.set_memory_limit(8 * 1024 * 1024)?;
        let remaining = Arc::new(AtomicUsize::new(100));
        lua.set_hook(
            HookTriggers::new().every_nth_instruction(10_000),
            move |_, _| {
                if remaining.fetch_sub(1, Ordering::Relaxed) == 0 {
                    return Err(mlua::Error::runtime(
                        "configuration instruction limit exceeded",
                    ));
                }
                Ok(VmState::Continue)
            },
        )?;
        let configuration: Configuration =
            lua.from_value(lua.load(source).set_name("config.lua").eval()?)?;
        let mut keymap = Self {
            chats: configuration.chats,
            ..Self::default()
        };
        if let Some(text) = configuration.ghost_text {
            keymap.ghost_text = text;
        }
        for binding in configuration.keymap {
            keymap.insert(binding)?;
        }
        for (index, binding) in keymap.bindings.iter().enumerate() {
            for other in &keymap.bindings[index + 1..] {
                if binding.context == other.context
                    && (binding.keys.starts_with(&other.keys)
                        || other.keys.starts_with(&binding.keys))
                {
                    bail!(
                        "ambiguous key prefix: {} / {}",
                        display_keys(&binding.keys),
                        display_keys(&other.keys)
                    );
                }
            }
        }
        Ok(keymap)
    }

    fn insert(&mut self, spec: BindingSpec) -> Result<()> {
        if spec.on.is_empty() || spec.on.len() > 4 {
            bail!("a binding must contain one to four keys");
        }
        let keys = spec
            .on
            .iter()
            .map(|key| {
                if !key.starts_with('<') && key.chars().count() != 1 {
                    bail!("use separate keys for a chord: {key}");
                }
                Key::from_str(key)
            })
            .collect::<Result<Vec<_>>>()?;
        if let Some(alias) = spec.run.strip_prefix("jump ") {
            if !self.chats.contains_key(alias) {
                bail!("unknown chat alias: {alias}");
            }
        } else if !ACTIONS.contains(&spec.run.as_str()) {
            bail!("unknown action: {}", spec.run);
        }
        self.bindings
            .retain(|binding| !(binding.context == spec.context && binding.keys == keys));
        if spec.run == "noop" {
            return Ok(());
        }
        self.bindings.push(Binding {
            context: spec.context,
            keys,
            description: spec.desc.unwrap_or_else(|| spec.run.replace('_', " ")),
            run: spec.run,
        });
        Ok(())
    }

    pub fn reset(&mut self) {
        self.pending.clear();
        self.count = 0;
        self.last_key = None;
    }

    #[must_use]
    pub fn pending(&self) -> bool {
        self.last_key.is_some()
    }

    pub fn expire(&mut self) -> bool {
        if self
            .last_key
            .is_some_and(|last| last.elapsed() > Duration::from_secs(1))
        {
            self.reset();
            true
        } else {
            false
        }
    }

    pub fn feed(&mut self, context: Context, event: &KeyEvent) -> Resolution {
        if event.kind == KeyEventKind::Release {
            return Resolution::Unbound;
        }
        self.expire();
        if self.context != Some(context) {
            self.reset();
            self.context = Some(context);
        }
        let Ok(key) = Key::try_from(event.clone()) else {
            return Resolution::Unbound;
        };
        if key.code == yazi_term::event::KeyCode::Escape && self.pending() {
            self.reset();
            return Resolution::Pending(String::new());
        }
        if matches!(context, Context::Chats | Context::Conversation)
            && self.pending.is_empty()
            && let Ok(digit) = key.to_string().parse::<usize>()
            && (digit != 0 || self.count != 0)
            && (self.count != 0
                || !self.bindings.iter().any(|binding| {
                    (binding.context == Context::Global || binding.context == context)
                        && binding.keys == [key]
                }))
        {
            self.count = (self.count * 10 + digit).min(9999);
            self.last_key = Some(Instant::now());
            return Resolution::Pending(self.count.to_string());
        }
        self.pending.push(key);
        for priority in [context, Context::Global] {
            if let Some(binding) = self
                .bindings
                .iter()
                .find(|binding| binding.context == priority && binding.keys == self.pending)
            {
                let result = Resolution::Action {
                    run: binding.run.clone(),
                    count: self.count.max(1),
                };
                self.reset();
                return result;
            }
            let candidates = self
                .bindings
                .iter()
                .filter(|binding| {
                    binding.context == priority && binding.keys.starts_with(&self.pending)
                })
                .collect::<Vec<_>>();
            if !candidates.is_empty() {
                self.last_key = Some(Instant::now());
                return Resolution::Pending(
                    candidates
                        .iter()
                        .map(|binding| {
                            format!("{} {}", display_keys(&binding.keys), binding.description)
                        })
                        .collect::<Vec<_>>()
                        .join(" · "),
                );
            }
        }
        self.reset();
        Resolution::Unbound
    }

    #[must_use]
    pub fn hint(&self, context: Context, run: &str) -> String {
        self.bindings
            .iter()
            .find(|binding| binding.context == context && binding.run == run)
            .map_or_else(
                || "unbound".to_owned(),
                |binding| display_keys(&binding.keys),
            )
    }

    #[must_use]
    pub fn help(&self) -> Vec<String> {
        self.bindings
            .iter()
            .filter(|binding| binding.run != "noop")
            .map(|binding| {
                format!(
                    "{:?}  {:18} {}",
                    binding.context,
                    display_keys(&binding.keys),
                    binding.description
                )
            })
            .collect()
    }
}

fn display_keys(keys: &[Key]) -> String {
    keys.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::{Context, Keymap, Resolution};
    use yazi_term::event::{KeyCode, KeyEvent, Modifiers};

    #[test]
    fn lua_overrides_hints_and_keeps_counts_out_of_the_editor() {
        let mut map = Keymap::parse(
            r"return {
            chats = { work = -1000000000042 },
            keymap = {
                { context='compose', on={'<Enter>'}, run='newline' },
                { context='compose', on={'<C-s>'}, run='send' },
                { context='conversation', on={'g','w'}, run='jump work' },
            }
        }",
        )
        .unwrap();
        assert_eq!(map.hint(Context::Compose, "send"), "<C-s>");
        let key = |c| KeyEvent::new(KeyCode::Char(c), Modifiers::empty());
        assert!(matches!(
            map.feed(Context::Conversation, &key('2')),
            Resolution::Pending(_)
        ));
        assert!(matches!(
            map.feed(Context::Conversation, &key('0')),
            Resolution::Pending(_)
        ));
        assert_eq!(
            map.feed(Context::Conversation, &key('k')),
            Resolution::Action {
                run: "message_up".to_owned(),
                count: 20
            }
        );
        assert_eq!(map.feed(Context::Compose, &key('2')), Resolution::Unbound);
        assert!(matches!(
            map.feed(Context::Conversation, &key('g')),
            Resolution::Pending(_)
        ));
        assert_eq!(
            map.feed(Context::Conversation, &key('w')),
            Resolution::Action {
                run: "jump work".to_owned(),
                count: 1
            }
        );
        assert!(
            map.help()
                .iter()
                .any(|line| line.contains("<C-s>") && line.ends_with("send"))
        );
    }

    #[test]
    fn invalid_configuration_is_rejected_without_hanging() {
        assert!(Keymap::parse("while true do end").is_err());
        assert!(
            Keymap::parse("return { keymap={{context='conversation',on={'g'},run='latest'}} }")
                .is_err()
        );
        assert!(
            Keymap::parse("return { keymap={{context='conversation',on={'x'},run='unknown'}} }")
                .is_err()
        );
    }
}
