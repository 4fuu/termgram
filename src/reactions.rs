//! Telegram reaction identity and partial-result semantics.
use crate::model::ChatId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum Kind {
    Emoji(String),
    Custom(i64),
    Paid,
}

impl Kind {
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Emoji(emoji) => crate::model::sanitize_terminal_line(emoji),
            Self::Custom(_) => "[custom emoji]".to_owned(),
            Self::Paid => "★".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Count {
    pub kind: Kind,
    pub count: u32,
    pub chosen: Option<i32>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub counts: Vec<Count>,
    pub choices_known: bool,
    pub as_tags: bool,
    pub stale: bool,
}

impl Summary {
    pub fn apply(&mut self, incoming: &Self) {
        let mut counts = incoming.counts.clone();
        if !incoming.choices_known {
            for count in &mut counts {
                count.chosen = self
                    .counts
                    .iter()
                    .find(|old| old.kind == count.kind)
                    .and_then(|old| old.chosen);
            }
        }
        self.counts = counts;
        self.as_tags = incoming.as_tags;
        self.stale = incoming.stale || (!incoming.choices_known && self.stale);
        self.choices_known |= incoming.choices_known;
    }

    #[must_use]
    pub fn chosen(&self) -> Vec<Kind> {
        let mut chosen: Vec<_> = self
            .counts
            .iter()
            .filter_map(|count| {
                (count.kind != Kind::Paid)
                    .then_some(count.chosen)
                    .flatten()
                    .map(|order| (order, count.kind.clone()))
            })
            .collect();
        chosen.sort_by_key(|(order, _)| *order);
        chosen.into_iter().map(|(_, kind)| kind).collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Update {
    pub chat: ChatId,
    pub message: i32,
    pub summary: Summary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Choice {
    pub emoji: String,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Review {
    pub summary: Summary,
    pub choices: Vec<Choice>,
    pub max_chosen: usize,
    pub max_unique: usize,
    pub read_only: Option<String>,
}

impl Review {
    /// Compute a fresh desired set, preserving the order and identity of other
    /// choices. Telegram replaces the oldest choice when the user's limit is hit.
    /// # Errors
    /// Rejects unavailable choices, unknown state and unique-reaction limits.
    pub fn toggle(&self, emoji: Option<&str>) -> Result<Vec<Kind>, String> {
        if let Some(reason) = &self.read_only {
            return Err(reason.clone());
        }
        if self.summary.stale || !self.summary.choices_known {
            return Err("Refresh reactions before changing them".to_owned());
        }
        let Some(emoji) = emoji else {
            return Ok(Vec::new());
        };
        let target = Kind::Emoji(emoji.to_owned());
        let mut chosen = self.summary.chosen();
        if chosen.contains(&target) {
            chosen.retain(|kind| *kind != target);
            return Ok(chosen);
        }
        if !self.choices.iter().any(|choice| choice.emoji == emoji) || self.max_chosen == 0 {
            return Err("This reaction is not available in this chat".to_owned());
        }
        while chosen.len() >= self.max_chosen {
            chosen.remove(0);
        }
        let unique = self
            .summary
            .counts
            .iter()
            .filter(|count| {
                count.kind != Kind::Paid
                    && count.count > 0
                    && !(count.count == 1
                        && count.chosen.is_some()
                        && !chosen.contains(&count.kind))
            })
            .count();
        if !self
            .summary
            .counts
            .iter()
            .any(|count| count.kind == target && count.count > 0)
            && unique >= self.max_unique
        {
            return Err("This message has reached its unique reaction limit".to_owned());
        }
        chosen.push(target);
        Ok(chosen)
    }
}

#[cfg(test)]
pub(crate) fn example() -> Review {
    Review {
        summary: Summary {
            counts: vec![
                Count {
                    kind: Kind::Emoji("👍".to_owned()),
                    count: 4,
                    chosen: Some(0),
                },
                Count {
                    kind: Kind::Custom(7),
                    count: 1,
                    chosen: Some(1),
                },
            ],
            choices_known: true,
            ..Summary::default()
        },
        choices: [("👍", "Like"), ("❤", "Love"), ("🔥", "Fire")]
            .into_iter()
            .map(|(emoji, title)| Choice {
                emoji: emoji.to_owned(),
                title: title.to_owned(),
            })
            .collect(),
        max_chosen: 3,
        max_unique: 11,
        read_only: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_reactions_keep_private_choices_but_drop_removed_counters() {
        let mut summary = example().summary;
        summary.apply(&Summary {
            counts: vec![Count {
                kind: Kind::Emoji("👍".to_owned()),
                count: 5,
                chosen: None,
            }],
            ..Summary::default()
        });
        assert_eq!(summary.chosen(), [Kind::Emoji("👍".to_owned())]);
        assert!(summary.choices_known);
        assert_eq!(summary.counts[0].count, 5);
        summary.apply(&Summary {
            choices_known: true,
            ..Summary::default()
        });
        assert!(summary.counts.is_empty());
    }

    #[test]
    fn changing_reactions_preserves_other_choices_and_telegram_limits() {
        let mut review = example();
        assert_eq!(review.toggle(Some("👍")).unwrap(), [Kind::Custom(7)]);
        assert_eq!(
            review.toggle(Some("❤")).unwrap(),
            [
                Kind::Emoji("👍".to_owned()),
                Kind::Custom(7),
                Kind::Emoji("❤".to_owned())
            ]
        );
        review.max_chosen = 2;
        assert_eq!(
            review.toggle(Some("❤")).unwrap(),
            [Kind::Custom(7), Kind::Emoji("❤".to_owned())]
        );
        review.max_unique = 2;
        assert!(
            review.toggle(Some("❤")).is_err(),
            "oldest choice still has other voters"
        );
        review.summary.counts[0].count = 1;
        assert!(
            review.toggle(Some("❤")).is_ok(),
            "replacing own exclusive choice frees a slot"
        );
        assert!(review.toggle(Some("unavailable")).is_err());
        assert!(review.toggle(None).unwrap().is_empty());
        review.summary.stale = true;
        assert!(review.toggle(None).is_err());
    }
}
