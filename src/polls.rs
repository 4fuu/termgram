//! Poll identity, partial Telegram results and voting rules, without SDK/UI types.
use crate::entities::Entity;
use serde::{Deserialize, Serialize};
use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Clone, Debug, Default, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Text {
    pub text: String,
    pub entities: Vec<Entity>,
}

impl Text {
    #[must_use]
    pub fn preview(&self) -> String {
        crate::entities::conceal(&self.text, &self.entities).into_owned()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Answer {
    pub option: Vec<u8>,
    pub text: Text,
    pub has_media: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
// Independent Telegram capabilities, not mutually exclusive UI states.
#[allow(clippy::struct_excessive_bools)]
pub struct Definition {
    pub id: i64,
    pub hash: i64,
    pub question: Text,
    pub answers: Vec<Answer>,
    pub closed: bool,
    pub public_voters: bool,
    pub multiple_choice: bool,
    pub quiz: bool,
    pub revoting_disabled: bool,
    pub hide_results_until_close: bool,
    pub subscribers_only: bool,
    pub countries: Vec<String>,
    pub close_date: Option<i64>,
    pub has_media: bool,
}

impl Definition {
    #[must_use]
    pub fn revision(&self) -> u64 {
        let mut hash = DefaultHasher::new();
        self.hash(&mut hash);
        hash.finish()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Count {
    pub option: Vec<u8>,
    pub voters: Option<u32>,
    pub chosen: bool,
    pub correct: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Results {
    pub counts: Vec<Count>,
    pub total: Option<u32>,
    pub choice_known: bool,
    pub solution: Option<Text>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Poll {
    pub definition: Definition,
    pub results: Results,
    #[serde(default)]
    pub stale: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResultsPatch {
    pub min: bool,
    pub counts: Option<Vec<Count>>,
    pub total: Option<u32>,
    pub solution: Option<Text>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Update {
    pub id: i64,
    pub stale: bool,
    pub definition: Option<Definition>,
    pub results: ResultsPatch,
}

impl Poll {
    pub(crate) fn apply(&mut self, update: &Update) {
        if self.definition.id != update.id {
            return;
        }
        if let Some(definition) = &update.definition {
            let has_media = self.definition.has_media;
            self.definition.clone_from(definition);
            // updateMessagePoll does not carry the message's attached media.
            self.definition.has_media |= has_media;
            self.results.counts.retain(|count| {
                definition
                    .answers
                    .iter()
                    .any(|answer| answer.option == count.option)
            });
            self.stale = false;
        }
        let patch = &update.results;
        if let Some(total) = patch.total {
            self.results.total = Some(total);
        }
        if !patch.min {
            self.results.choice_known = true;
        }
        if let Some(counts) = &patch.counts {
            for new in counts {
                if !self
                    .definition
                    .answers
                    .iter()
                    .any(|answer| answer.option == new.option)
                {
                    continue;
                }
                let count = if let Some(index) = self
                    .results
                    .counts
                    .iter()
                    .position(|count| count.option == new.option)
                {
                    &mut self.results.counts[index]
                } else {
                    self.results.counts.push(Count {
                        option: new.option.clone(),
                        ..Count::default()
                    });
                    self.results.counts.last_mut().expect("inserted count")
                };
                if let Some(voters) = new.voters {
                    count.voters = Some(voters);
                }
                if !patch.min {
                    count.chosen = new.chosen;
                }
                count.correct |= new.correct;
            }
        } else if !patch.min && self.results.total == Some(0) {
            for count in &mut self.results.counts {
                count.chosen = false;
            }
        }
        if let Some(solution) = &patch.solution {
            self.results.solution = Some(solution.clone());
        }
        self.stale |= update.stale;
    }

    #[must_use]
    pub fn voted(&self) -> bool {
        self.results.counts.iter().any(|count| count.chosen)
    }

    #[must_use]
    pub fn count(&self, option: &[u8]) -> Option<&Count> {
        self.results
            .counts
            .iter()
            .find(|count| count.option == option)
    }

    #[must_use]
    pub fn closed(&self, now: i64) -> bool {
        self.definition.closed || self.definition.close_date.is_some_and(|date| date <= now)
    }

    #[must_use]
    pub fn results_visible(&self, now: i64) -> bool {
        !self.stale
            && (self.closed(now) || (!self.definition.hide_results_until_close && self.voted()))
    }

    #[must_use]
    pub fn read_only_reason(&self, now: i64) -> Option<&'static str> {
        if self.stale || !self.results.choice_known {
            Some("Refresh this poll before voting")
        } else if self.closed(now) {
            Some("This poll is closed")
        } else if self.voted() && (self.definition.quiz || self.definition.revoting_disabled) {
            Some("This poll does not allow changing your vote")
        } else if self.definition.has_media
            || self
                .definition
                .answers
                .iter()
                .any(|answer| answer.has_media)
        {
            Some("This poll includes media; review and vote in an official client")
        } else {
            None
        }
    }

    /// # Errors
    /// Returns a reason when the current server definition no longer permits
    /// these stable option IDs. An empty set explicitly retracts an earlier vote.
    pub fn validate_vote(
        &self,
        revision: u64,
        options: &[Vec<u8>],
        now: i64,
    ) -> Result<(), String> {
        if self.definition.revision() != revision {
            return Err("Poll options changed; refresh and choose again".to_owned());
        }
        if let Some(reason) = self.read_only_reason(now) {
            return Err(reason.to_owned());
        }
        if options.is_empty() && !self.voted() {
            return Err("Choose an answer first".to_owned());
        }
        if !self.definition.multiple_choice && options.len() > 1 {
            return Err("Choose one answer".to_owned());
        }
        for (index, option) in options.iter().enumerate() {
            if !self
                .definition
                .answers
                .iter()
                .any(|answer| &answer.option == option)
                || options[..index].contains(option)
            {
                return Err("Poll options changed; refresh and choose again".to_owned());
            }
        }
        Ok(())
    }

    pub fn texts(&self) -> impl Iterator<Item = &Text> {
        std::iter::once(&self.definition.question)
            .chain(self.definition.answers.iter().map(|answer| &answer.text))
            .chain(self.results.solution.iter())
    }
}

#[cfg(test)]
pub(crate) fn example() -> Poll {
    Poll {
        definition: Definition {
            id: 123,
            hash: 456,
            question: Text {
                text: "Choose your drink".to_owned(),
                entities: Vec::new(),
            },
            answers: [([0, 255], "Tea 茶"), ([1, 128], "Coffee ☕")]
                .into_iter()
                .map(|(option, text)| Answer {
                    option: option.to_vec(),
                    text: Text {
                        text: text.to_owned(),
                        entities: Vec::new(),
                    },
                    has_media: false,
                })
                .collect(),
            closed: false,
            public_voters: false,
            multiple_choice: true,
            quiz: false,
            revoting_disabled: false,
            hide_results_until_close: false,
            subscribers_only: false,
            countries: Vec::new(),
            close_date: None,
            has_media: false,
        },
        results: Results {
            choice_known: true,
            total: Some(0),
            ..Results::default()
        },
        stale: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_min_results_preserve_choice_correctness_and_omitted_counts() {
        let mut poll = example();
        let mut update = Update {
            id: poll.definition.id,
            stale: false,
            definition: None,
            results: ResultsPatch {
                min: false,
                counts: Some(vec![Count {
                    option: vec![0, 255],
                    voters: Some(4),
                    chosen: true,
                    correct: true,
                }]),
                total: Some(5),
                solution: None,
            },
        };
        poll.apply(&update);
        update.results.min = true;
        update.results.counts = Some(vec![Count {
            option: vec![0, 255],
            voters: None,
            chosen: false,
            correct: false,
        }]);
        update.results.total = Some(6);
        poll.apply(&update);
        assert!(poll.voted());
        assert!(poll.results.counts[0].correct);
        assert_eq!(poll.results.counts[0].voters, Some(4));
        assert_eq!(poll.results.total, Some(6));
        assert!(poll.results_visible(0));
        update.results.min = false;
        poll.apply(&update);
        assert!(!poll.voted());
        assert!(!poll.results_visible(0));
        assert!(poll.results.counts[0].correct);
    }

    #[test]
    fn poll_votes_use_stable_bytes_and_enforce_changed_closed_and_quiz_rules() {
        let mut poll = example();
        let revision = poll.definition.revision();
        assert!(
            poll.validate_vote(revision, &[vec![0, 255], vec![1, 128]], 0)
                .is_ok()
        );
        assert!(poll.validate_vote(revision, &[], 0).is_err());
        assert!(
            poll.validate_vote(revision, &[vec![0, 255], vec![0, 255]], 0)
                .is_err()
        );
        assert!(poll.validate_vote(revision, &[vec![0]], 0).is_err());
        poll.results.counts.push(Count {
            option: vec![0, 255],
            chosen: true,
            ..Count::default()
        });
        assert!(poll.validate_vote(revision, &[], 0).is_ok());
        poll.definition.quiz = true;
        assert!(
            poll.validate_vote(poll.definition.revision(), &[vec![1, 128]], 0)
                .is_err()
        );
        poll.definition.quiz = false;
        poll.definition.hide_results_until_close = true;
        assert!(!poll.results_visible(0));
        poll.definition.close_date = Some(10);
        assert!(poll.results_visible(10));
        assert!(
            poll.validate_vote(poll.definition.revision(), &[vec![0, 255]], 10)
                .is_err()
        );
        poll.definition.close_date = None;
        poll.definition.answers.swap(0, 1);
        assert!(poll.validate_vote(revision, &[vec![0, 255]], 0).is_err());
        poll.definition.has_media = true;
        assert!(poll.read_only_reason(0).unwrap().contains("official"));
    }
}
