//! Intent documents: the content-level schema of a `document::intent` entity.
//!
//! An intent is a registered reaction — *"when this trigger fires, do this
//! action."* It is stored as a UCM entity of type `document::intent` whose body
//! is Markdown with YAML frontmatter that serializes the rule. This module is
//! the typed view of that frontmatter: parse it into an [`IntentDocument`],
//! inspect the [`Trigger`], and (for later stages) compile the [`ActionSpec`]
//! into the executable action AST.
//!
//! Scope boundaries within the Events phase:
//! - The [`Trigger`] is fully typed here — the engine needs it to subscribe and
//!   the timer trigger converts straight into a [`Schedule`].
//! - The action is kept as a loosely-typed [`ActionSpec`] (name + parameter
//!   map). The typed `Expr`/`Action` AST is built on top of it in task 1.5, so
//!   the parameters round-trip losslessly until then.
//! - Compound-condition *evaluation* is task 5.1; this module only models the
//!   condition data so documents that use it still load.

use std::time::Duration as StdDuration;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use super::Schedule;
use crate::storage::vault::markdown::split_markdown;

/// Frontmatter keys owned by the storage/vault layer, stripped before the
/// remainder is treated as action parameters.
const META_KEYS: &[&str] = &["id", "kind", "type", "title", "tags", "origin", "privacy"];

/// A parsed intent document: trigger + action + addressing + body.
#[derive(Clone, Debug, PartialEq)]
pub struct IntentDocument {
    /// When the intent fires.
    pub trigger: Trigger,
    /// What it does when it fires (typed into an action AST in task 1.5).
    pub action: ActionSpec,
    /// Where the action is directed (e.g. `user:chris@discord:12345`).
    pub target: Option<String>,
    /// Who created the intent (e.g. `user:chris`, `agent:lumina`).
    pub created_by: Option<String>,
    /// Free-text body — context for LLM-backed actions.
    pub body: String,
}

impl IntentDocument {
    /// Parse an intent from a Markdown document with `type: intent` frontmatter.
    pub fn from_markdown(input: &str) -> Result<Self> {
        let split = split_markdown(input);
        let raw = split
            .raw_frontmatter
            .context("intent document requires YAML frontmatter")?;

        let value: Value =
            serde_yaml::from_str(raw).context("failed to parse intent frontmatter")?;
        let Value::Object(mut map) = value else {
            bail!("intent frontmatter must be a YAML mapping");
        };

        ensure_intent_type(&map)?;

        let trigger_value = map
            .remove("trigger")
            .context("intent requires a `trigger`")?;
        let trigger: Trigger =
            serde_json::from_value(trigger_value).context("invalid intent `trigger`")?;

        let action = take_string(&mut map, "action").context("intent requires an `action`")?;
        let target = map.remove("target").and_then(value_into_string);
        let created_by = map.remove("created_by").and_then(value_into_string);

        for key in META_KEYS {
            map.remove(*key);
        }

        Ok(IntentDocument {
            trigger,
            action: ActionSpec {
                action,
                params: map,
            },
            target,
            created_by,
            body: split.body.to_string(),
        })
    }

    /// Serialize back to a `type: intent` Markdown document.
    pub fn to_markdown(&self) -> Result<String> {
        let mut map = Map::new();
        map.insert("type".to_string(), Value::String("intent".to_string()));
        map.insert(
            "trigger".to_string(),
            serde_json::to_value(&self.trigger).context("failed to serialize trigger")?,
        );
        map.insert(
            "action".to_string(),
            Value::String(self.action.action.clone()),
        );
        for (key, value) in &self.action.params {
            map.insert(key.clone(), value.clone());
        }
        if let Some(target) = &self.target {
            map.insert("target".to_string(), Value::String(target.clone()));
        }
        if let Some(created_by) = &self.created_by {
            map.insert("created_by".to_string(), Value::String(created_by.clone()));
        }

        let yaml = serde_yaml::to_string(&Value::Object(map))
            .context("failed to serialize intent frontmatter")?;
        Ok(format!("---\n{}---\n{}", yaml, self.body))
    }

    /// Convenience: read an action parameter by key.
    pub fn param(&self, key: &str) -> Option<&Value> {
        self.action.params.get(key)
    }
}

/// The action half of an intent, kept as a name plus a parameter map until the
/// task 1.5 compiler turns it into a typed action AST.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionSpec {
    /// Action type, e.g. `notify`, `forward`, `execute_prompt`.
    pub action: String,
    /// Action-specific parameters (e.g. `prompt`, `conversation_id`, `message`).
    pub params: Map<String, Value>,
}

/// When an intent fires. Discriminated by frontmatter shape rather than an
/// explicit tag, matching the hand-authorable design schema.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawTrigger", into = "RawTrigger")]
pub enum Trigger {
    /// A single event from a platform/source.
    Event(EventTrigger),
    /// A time-based trigger handled by the timer source.
    Timer(TimerTrigger),
    /// Multiple conditions combined with all/any semantics (evaluated in 5.1).
    Compound(CompoundTrigger),
}

/// `{ source: discord, event: member_joined, recurrence: every }`
#[derive(Clone, Debug, PartialEq)]
pub struct EventTrigger {
    pub source: String,
    pub event: String,
    pub recurrence: Option<Recurrence>,
}

/// A timer trigger: exact, fuzzy, cron, or interval.
#[derive(Clone, Debug, PartialEq)]
pub struct TimerTrigger {
    /// Concrete fire time for exact/fuzzy one-shots.
    pub resolved: Option<DateTime<Utc>>,
    /// Original human phrase, preserved for fuzzy times.
    pub original: Option<String>,
    pub precision: Option<TimePrecision>,
    /// Cron expression for recurring schedules (seconds-first 6/7-field).
    pub cron: Option<String>,
    /// Fixed interval in seconds for recurring schedules.
    pub interval_secs: Option<u64>,
    pub recurrence: Option<Recurrence>,
}

impl TimerTrigger {
    /// Build the [`Schedule`] this trigger represents, relative to `now` (used
    /// only to resolve a leftover fuzzy `original` with no `resolved` time).
    ///
    /// Precedence: cron → interval → resolved instant → fuzzy original.
    pub fn to_schedule(&self, now: DateTime<Utc>) -> Option<Schedule> {
        if let Some(cron) = &self.cron {
            return Schedule::cron(cron).ok();
        }
        if let Some(secs) = self.interval_secs {
            return Some(Schedule::interval(StdDuration::from_secs(secs)));
        }
        if let Some(resolved) = self.resolved {
            return Some(Schedule::once(resolved));
        }
        if let Some(original) = &self.original {
            return Schedule::fuzzy(original.clone(), now);
        }
        None
    }
}

/// `{ mode: all, conditions: [ ... ] }`
#[derive(Clone, Debug, PartialEq)]
pub struct CompoundTrigger {
    pub mode: ConditionMode,
    pub conditions: Vec<EventMatch>,
}

/// One condition of a compound trigger: a matched event plus optional filters
/// (e.g. `intent_id`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EventMatch {
    pub source: String,
    pub event: String,
    #[serde(flatten, default, skip_serializing_if = "Map::is_empty")]
    pub filters: Map<String, Value>,
}

/// Whether a single matching event fires the intent once or on every match.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Recurrence {
    Once,
    Every,
}

/// How a resolved time was derived.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimePrecision {
    Exact,
    Fuzzy,
}

/// Combination mode for compound conditions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConditionMode {
    All,
    Any,
}

// ============================================================================
// Trigger wire form
// ============================================================================

/// Flat serialization shape for any [`Trigger`]. Classified into the typed enum
/// on the way in, flattened on the way out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct RawTrigger {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    event: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recurrence: Option<Recurrence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    original: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_opt_datetime",
        skip_serializing_if = "Option::is_none"
    )]
    resolved: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    precision: Option<TimePrecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cron: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    interval_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<ConditionMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    conditions: Option<Vec<EventMatch>>,
}

impl TryFrom<RawTrigger> for Trigger {
    type Error = String;

    fn try_from(raw: RawTrigger) -> Result<Self, Self::Error> {
        // Compound is recognised by mode/conditions, which no other shape uses.
        if raw.mode.is_some() || raw.conditions.is_some() {
            let mode = raw.mode.ok_or("compound trigger requires `mode`")?;
            let conditions = raw.conditions.unwrap_or_default();
            if conditions.is_empty() {
                return Err("compound trigger requires at least one condition".into());
            }
            return Ok(Trigger::Compound(CompoundTrigger { mode, conditions }));
        }

        let source = raw
            .source
            .ok_or("trigger requires a `source` (or `mode` for compound triggers)")?;

        if source == "timer" {
            Ok(Trigger::Timer(TimerTrigger {
                resolved: raw.resolved,
                original: raw.original,
                precision: raw.precision,
                cron: raw.cron,
                interval_secs: raw.interval_secs,
                recurrence: raw.recurrence,
            }))
        } else {
            let event = raw.event.ok_or("event trigger requires an `event`")?;
            Ok(Trigger::Event(EventTrigger {
                source,
                event,
                recurrence: raw.recurrence,
            }))
        }
    }
}

impl From<Trigger> for RawTrigger {
    fn from(trigger: Trigger) -> Self {
        let mut raw = RawTrigger {
            source: None,
            event: None,
            recurrence: None,
            original: None,
            resolved: None,
            precision: None,
            cron: None,
            interval_secs: None,
            mode: None,
            conditions: None,
        };
        match trigger {
            Trigger::Event(e) => {
                raw.source = Some(e.source);
                raw.event = Some(e.event);
                raw.recurrence = e.recurrence;
            }
            Trigger::Timer(t) => {
                raw.source = Some("timer".to_string());
                raw.resolved = t.resolved;
                raw.original = t.original;
                raw.precision = t.precision;
                raw.cron = t.cron;
                raw.interval_secs = t.interval_secs;
                raw.recurrence = t.recurrence;
            }
            Trigger::Compound(c) => {
                raw.mode = Some(c.mode);
                raw.conditions = Some(c.conditions);
            }
        }
        raw
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Accept either `type: intent` or an entity `kind: document::intent`.
fn ensure_intent_type(map: &Map<String, Value>) -> Result<()> {
    let type_ok = map.get("type").and_then(Value::as_str) == Some("intent");
    let kind_ok = map
        .get("kind")
        .and_then(Value::as_str)
        .is_some_and(|k| k == "intent" || k.ends_with("::intent"));
    if type_ok || kind_ok {
        Ok(())
    } else {
        bail!("not an intent document (expected `type: intent`)");
    }
}

fn take_string(map: &mut Map<String, Value>, key: &str) -> Option<String> {
    map.remove(key).and_then(value_into_string)
}

fn value_into_string(value: Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s),
        _ => None,
    }
}

/// Parse an optional datetime, accepting RFC 3339 (`...Z`/offset) or a naive
/// `YYYY-MM-DDTHH:MM:SS` form interpreted as UTC.
fn deserialize_opt_datetime<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Option::<String>::deserialize(deserializer)?;
    let Some(text) = raw else { return Ok(None) };

    if let Ok(dt) = DateTime::parse_from_rfc3339(&text) {
        return Ok(Some(dt.with_timezone(&Utc)));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(&text, "%Y-%m-%dT%H:%M:%S") {
        return Ok(Some(Utc.from_utc_datetime(&naive)));
    }
    Err(serde::de::Error::custom(format!(
        "invalid datetime: {text:?}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_timer_reminder() {
        let md = "---\n\
type: intent\n\
trigger:\n  source: timer\n  original: \"tomorrow morning\"\n  resolved: \"2026-04-01T09:00:00\"\n  precision: fuzzy\n\
action: resume_conversation\n\
conversation_id: \"conv-abc-123\"\n\
target: \"user:chris@discord:12345\"\n\
created_by: \"user:chris\"\n\
---\n\
Check in on the voice migration progress\n";

        let intent = IntentDocument::from_markdown(md).unwrap();
        let Trigger::Timer(timer) = &intent.trigger else {
            panic!("expected timer trigger");
        };
        assert_eq!(
            timer.resolved,
            Some(Utc.with_ymd_and_hms(2026, 4, 1, 9, 0, 0).unwrap())
        );
        assert_eq!(timer.original.as_deref(), Some("tomorrow morning"));
        assert_eq!(timer.precision, Some(TimePrecision::Fuzzy));

        assert_eq!(intent.action.action, "resume_conversation");
        assert_eq!(
            intent.param("conversation_id").and_then(Value::as_str),
            Some("conv-abc-123")
        );
        assert_eq!(intent.target.as_deref(), Some("user:chris@discord:12345"));
        assert_eq!(intent.created_by.as_deref(), Some("user:chris"));
        assert_eq!(intent.body.trim(), "Check in on the voice migration progress");
        // `target`/`created_by` are intent-level, not action params.
        assert!(intent.param("target").is_none());
    }

    #[test]
    fn parses_event_welcome() {
        let md = "---\n\
type: intent\n\
trigger:\n  source: discord\n  event: member_joined\n  recurrence: every\n\
action: execute_prompt\n\
prompt: \"Welcome {event.member.name}\"\n\
target: \"channel:welcome\"\n\
---\n\
Generate a welcome message\n";

        let intent = IntentDocument::from_markdown(md).unwrap();
        let Trigger::Event(event) = &intent.trigger else {
            panic!("expected event trigger");
        };
        assert_eq!(event.source, "discord");
        assert_eq!(event.event, "member_joined");
        assert_eq!(event.recurrence, Some(Recurrence::Every));
        assert_eq!(intent.action.action, "execute_prompt");
        assert_eq!(
            intent.param("prompt").and_then(Value::as_str),
            Some("Welcome {event.member.name}")
        );
    }

    #[test]
    fn parses_compound() {
        let md = "---\n\
type: intent\n\
trigger:\n  mode: all\n  conditions:\n    - source: intent_lifecycle\n      event: intent.completed\n      intent_id: \"intent-a\"\n    - source: intent_lifecycle\n      event: intent.completed\n      intent_id: \"intent-b\"\n\
action: resume_conversation\n\
conversation_id: \"conv-456\"\n\
---\n\
Synthesize findings\n";

        let intent = IntentDocument::from_markdown(md).unwrap();
        let Trigger::Compound(compound) = &intent.trigger else {
            panic!("expected compound trigger");
        };
        assert_eq!(compound.mode, ConditionMode::All);
        assert_eq!(compound.conditions.len(), 2);
        assert_eq!(compound.conditions[0].source, "intent_lifecycle");
        assert_eq!(
            compound.conditions[1].filters.get("intent_id").and_then(Value::as_str),
            Some("intent-b")
        );
    }

    #[test]
    fn round_trips_through_markdown() {
        let md = "---\n\
type: intent\n\
trigger:\n  source: timer\n  cron: \"0 0 9 * * *\"\n\
action: notify\n\
message: \"Standup time\"\n\
target: \"channel:team\"\n\
---\n\
Daily standup reminder\n";

        let intent = IntentDocument::from_markdown(md).unwrap();
        let reparsed = IntentDocument::from_markdown(&intent.to_markdown().unwrap()).unwrap();
        assert_eq!(intent, reparsed);
    }

    #[test]
    fn timer_trigger_converts_to_schedule() {
        let now = Utc.with_ymd_and_hms(2026, 6, 1, 8, 0, 0).unwrap();

        let exact = TimerTrigger {
            resolved: Some(Utc.with_ymd_and_hms(2026, 6, 1, 9, 0, 0).unwrap()),
            original: None,
            precision: Some(TimePrecision::Exact),
            cron: None,
            interval_secs: None,
            recurrence: None,
        };
        assert!(matches!(exact.to_schedule(now), Some(Schedule::Once { .. })));

        let cron = TimerTrigger {
            resolved: None,
            original: None,
            precision: None,
            cron: Some("0 0 9 * * *".into()),
            interval_secs: None,
            recurrence: None,
        };
        assert!(matches!(cron.to_schedule(now), Some(Schedule::Cron(_))));

        let interval = TimerTrigger {
            resolved: None,
            original: None,
            precision: None,
            cron: None,
            interval_secs: Some(300),
            recurrence: Some(Recurrence::Every),
        };
        assert!(matches!(
            interval.to_schedule(now),
            Some(Schedule::Interval { .. })
        ));

        let fuzzy = TimerTrigger {
            resolved: None,
            original: Some("in 1 hour".into()),
            precision: Some(TimePrecision::Fuzzy),
            cron: None,
            interval_secs: None,
            recurrence: None,
        };
        match fuzzy.to_schedule(now) {
            Some(Schedule::Once { at, .. }) => {
                assert_eq!(at, now + chrono::Duration::hours(1))
            }
            other => panic!("expected resolved once, got {other:?}"),
        }
    }

    #[test]
    fn rejects_non_intent_and_missing_trigger() {
        let not_intent = "---\ntype: note\n---\nhi\n";
        assert!(IntentDocument::from_markdown(not_intent).is_err());

        let no_trigger = "---\ntype: intent\naction: notify\n---\nhi\n";
        assert!(IntentDocument::from_markdown(no_trigger).is_err());

        let no_frontmatter = "just a body";
        assert!(IntentDocument::from_markdown(no_frontmatter).is_err());
    }
}
