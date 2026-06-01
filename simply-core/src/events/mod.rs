//! Reactive event system.
//!
//! The event bus is the backbone of the agentic engine: event *sources* (timer,
//! Discord, document lifecycle, …) publish typed events, and the intent engine
//! and other consumers subscribe to react. Sources are code; the routing of
//! which events trigger which actions lives in UCM intent documents.
//!
//! This module currently provides the bus ([`EventBus`], [`Event`]) and the
//! [`EventSource`] contract. Concrete sources (timer first) and the intent
//! engine build on top of it.

mod bus;
mod fuzzy_time;
mod timer;

pub use bus::{Event, EventBus, EventFilter, EventSubscriber};
pub use fuzzy_time::parse_fuzzy;
pub use timer::{Schedule, Timer, TimerFired, TimerSource};

use async_trait::async_trait;

/// A producer of events. Sources are started once and run for the lifetime of
/// the daemon, publishing into the [`EventBus`] they are handed.
///
/// Implementors own their scheduling/IO and should loop until the bus is
/// dropped or they are otherwise told to stop.
#[async_trait]
pub trait EventSource: Send + Sync {
    /// Stable identifier for this source, used as [`Event::source`] and for
    /// diagnostics (e.g. `timer`, `discord`).
    fn source_id(&self) -> &str;

    /// Run the source, publishing events into `bus` until it completes.
    async fn start(&self, bus: EventBus) -> anyhow::Result<()>;
}
