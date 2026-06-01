//! In-process event bus: typed events, namespaced types, fan-out pub/sub.
//!
//! Events carry a namespaced `event_type` (e.g. `time.exact`,
//! `discord.member_joined`) and a structured JSON `payload`. Sources publish
//! events; subscribers receive them, optionally filtered by type or namespace
//! prefix. The bus is a thin wrapper over `tokio::sync::broadcast`, so it is
//! cheap to clone and share across the daemon.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Default channel capacity. A slow subscriber that falls this far behind
/// receives `RecvError::Lagged` and skips the dropped events.
const DEFAULT_CAPACITY: usize = 1024;

/// A single event flowing through the bus.
///
/// The `payload` is an arbitrary JSON object whose shape depends on
/// `event_type`. Producers build events from typed structs via [`Event::new`];
/// consumers read fields back via [`Event::payload_as`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Namespaced event type, e.g. `time.exact`, `discord.message`.
    pub event_type: String,
    /// Identifier of the source that emitted the event (see [`EventSource`]).
    pub source: String,
    /// Structured event data. Conventionally a JSON object.
    pub payload: serde_json::Value,
    /// When the event was created.
    pub timestamp: DateTime<Utc>,
}

impl Event {
    /// Build an event from a serializable payload.
    ///
    /// Panics only if `payload` fails to serialize to JSON, which for normal
    /// `#[derive(Serialize)]` structs cannot happen.
    pub fn new(
        event_type: impl Into<String>,
        source: impl Into<String>,
        payload: impl Serialize,
    ) -> Self {
        Self {
            event_type: event_type.into(),
            source: source.into(),
            payload: serde_json::to_value(payload).unwrap_or(serde_json::Value::Null),
            timestamp: Utc::now(),
        }
    }

    /// Deserialize the payload into a typed struct.
    pub fn payload_as<T: serde::de::DeserializeOwned>(&self) -> serde_json::Result<T> {
        serde_json::from_value(self.payload.clone())
    }

    /// Read a dotted path out of the payload, e.g. `member.name`.
    ///
    /// Returns `None` if any segment is missing or traverses a non-object.
    pub fn field(&self, path: &str) -> Option<&serde_json::Value> {
        let mut current = &self.payload;
        for segment in path.split('.') {
            current = current.get(segment)?;
        }
        Some(current)
    }
}

/// Selects which events a subscriber receives.
#[derive(Debug, Clone)]
pub enum EventFilter {
    /// Every event.
    All,
    /// Exact `event_type` match.
    Exact(String),
    /// Namespace prefix match, e.g. `discord.` matches `discord.message`.
    /// A bare `discord` also matches `discord.message` (treated as `discord.`).
    Prefix(String),
}

impl EventFilter {
    /// Whether `event` passes this filter.
    pub fn matches(&self, event: &Event) -> bool {
        match self {
            EventFilter::All => true,
            EventFilter::Exact(ty) => event.event_type == *ty,
            EventFilter::Prefix(prefix) => {
                let prefix = prefix.strip_suffix('.').unwrap_or(prefix);
                event.event_type == *prefix
                    || event
                        .event_type
                        .strip_prefix(prefix)
                        .is_some_and(|rest| rest.starts_with('.'))
            }
        }
    }
}

/// A cloneable handle to the event bus. Publishing and subscribing both go
/// through this type; clones share the same underlying channel.
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Event>,
}

impl EventBus {
    /// Create a bus with the default channel capacity.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Create a bus with an explicit channel capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Publish an event to all current subscribers.
    ///
    /// Returns the number of subscribers that received it. Sending with no
    /// subscribers is not an error — the event is simply dropped.
    pub fn publish(&self, event: Event) -> usize {
        self.tx.send(event).unwrap_or(0)
    }

    /// Convenience: build and publish an event in one call.
    pub fn emit(
        &self,
        event_type: impl Into<String>,
        source: impl Into<String>,
        payload: impl Serialize,
    ) -> usize {
        self.publish(Event::new(event_type, source, payload))
    }

    /// Subscribe to every event on the bus.
    pub fn subscribe(&self) -> EventSubscriber {
        self.subscribe_filtered(EventFilter::All)
    }

    /// Subscribe to events matching `filter`.
    pub fn subscribe_filtered(&self, filter: EventFilter) -> EventSubscriber {
        EventSubscriber {
            rx: self.tx.subscribe(),
            filter,
        }
    }

    /// Number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Receiving end of a subscription. Yields events that pass its filter,
/// transparently skipping the rest.
pub struct EventSubscriber {
    rx: broadcast::Receiver<Event>,
    filter: EventFilter,
}

impl EventSubscriber {
    /// Await the next matching event.
    ///
    /// Non-matching events are skipped without being surfaced. A `Lagged` error
    /// (slow consumer that missed events) is returned to the caller; `Closed`
    /// means the bus and all its senders were dropped.
    pub async fn recv(&mut self) -> Result<Event, broadcast::error::RecvError> {
        loop {
            let event = self.rx.recv().await?;
            if self.filter.matches(&event) {
                return Ok(event);
            }
        }
    }

    /// Try to receive the next matching event without awaiting.
    pub fn try_recv(&mut self) -> Result<Event, broadcast::error::TryRecvError> {
        loop {
            let event = self.rx.try_recv()?;
            if self.filter.matches(&event) {
                return Ok(event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct MemberJoined {
        member: Member,
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Member {
        name: String,
    }

    #[test]
    fn typed_payload_roundtrips() {
        let payload = MemberJoined {
            member: Member {
                name: "ada".into(),
            },
        };
        let event = Event::new("discord.member_joined", "discord", &payload);
        assert_eq!(event.payload_as::<MemberJoined>().unwrap(), payload);
    }

    #[test]
    fn field_reads_dotted_path() {
        let event = Event::new(
            "discord.member_joined",
            "discord",
            MemberJoined {
                member: Member {
                    name: "grace".into(),
                },
            },
        );
        assert_eq!(event.field("member.name").unwrap(), "grace");
        assert!(event.field("member.missing").is_none());
        assert!(event.field("nope.name").is_none());
    }

    #[test]
    fn exact_and_prefix_filters() {
        let event = Event::new("discord.message", "discord", serde_json::json!({}));
        assert!(EventFilter::All.matches(&event));
        assert!(EventFilter::Exact("discord.message".into()).matches(&event));
        assert!(!EventFilter::Exact("discord.reaction".into()).matches(&event));
        assert!(EventFilter::Prefix("discord".into()).matches(&event));
        assert!(EventFilter::Prefix("discord.".into()).matches(&event));
        assert!(!EventFilter::Prefix("disc".into()).matches(&event));

        // Bare namespace event matches its own prefix.
        let bare = Event::new("discord", "discord", serde_json::json!({}));
        assert!(EventFilter::Prefix("discord".into()).matches(&bare));
    }

    #[tokio::test]
    async fn publish_fans_out_to_all_subscribers() {
        let bus = EventBus::new();
        let mut a = bus.subscribe();
        let mut b = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);

        bus.emit("time.exact", "timer", serde_json::json!({ "n": 1 }));

        assert_eq!(a.recv().await.unwrap().field("n").unwrap(), 1);
        assert_eq!(b.recv().await.unwrap().field("n").unwrap(), 1);
    }

    #[tokio::test]
    async fn filtered_subscriber_skips_non_matching() {
        let bus = EventBus::new();
        let mut sub = bus.subscribe_filtered(EventFilter::Prefix("discord".into()));

        bus.emit("time.exact", "timer", serde_json::json!({}));
        bus.emit("discord.message", "discord", serde_json::json!({ "id": "m1" }));

        let event = sub.recv().await.unwrap();
        assert_eq!(event.event_type, "discord.message");
        assert_eq!(event.field("id").unwrap(), "m1");
    }

    #[tokio::test]
    async fn publish_without_subscribers_is_ok() {
        let bus = EventBus::new();
        assert_eq!(bus.emit("time.exact", "timer", serde_json::json!({})), 0);
    }
}
