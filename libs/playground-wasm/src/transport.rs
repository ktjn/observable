//! Browser replacement for the production Redpanda transport.
//!
//! The transport deliberately has no browser or async-runtime dependency. The
//! embedded component graph owns one instance and advances consumers explicitly,
//! which keeps ordering and backpressure deterministic in WASM and in tests.

use std::collections::{BTreeMap, VecDeque};

/// A bounded, topic-aware in-memory transport with broadcast fan-out.
#[derive(Debug)]
pub struct InMemoryTransport<T> {
    next_subscription_id: u64,
    subscribers: BTreeMap<u64, Subscriber<T>>,
}

#[derive(Debug)]
struct Subscriber<T> {
    topic: String,
    queue: VecDeque<T>,
    capacity: usize,
}

/// Failure returned when at least one matching consumer cannot accept a message.
#[derive(Debug, PartialEq, Eq)]
pub enum SendError {
    /// The message was not delivered to any subscriber. The send is atomic, so
    /// subscribers that had room are not partially advanced.
    Backpressure {
        topic: String,
        subscription_id: SubscriptionId,
    },
}

impl<T> Default for InMemoryTransport<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> InMemoryTransport<T> {
    pub fn new() -> Self {
        Self {
            next_subscription_id: 0,
            subscribers: BTreeMap::new(),
        }
    }

    /// Registers a bounded consumer for one topic and returns its handle.
    pub fn subscribe(&mut self, topic: impl Into<String>, capacity: usize) -> SubscriptionId {
        assert!(
            capacity > 0,
            "transport subscriber capacity must be positive"
        );
        let id = SubscriptionId(self.next_subscription_id);
        self.next_subscription_id += 1;
        self.subscribers.insert(
            id.0,
            Subscriber {
                topic: topic.into(),
                queue: VecDeque::with_capacity(capacity),
                capacity,
            },
        );
        id
    }

    /// Publishes to every subscriber on `topic`, preserving FIFO order.
    ///
    /// The operation is atomic across matching subscribers: if one queue is
    /// full, no matching queue receives the message and the caller can apply
    /// its explicit retry/drop policy.
    pub fn publish(&mut self, topic: &str, message: T) -> Result<usize, SendError>
    where
        T: Clone,
    {
        let matching: Vec<u64> = self
            .subscribers
            .iter()
            .filter_map(|(id, subscriber)| (subscriber.topic == topic).then_some(*id))
            .collect();

        if let Some(subscription_id) = matching.iter().find(|id| {
            self.subscribers
                .get(id)
                .is_some_and(|subscriber| subscriber.queue.len() >= subscriber.capacity)
        }) {
            return Err(SendError::Backpressure {
                topic: topic.to_owned(),
                subscription_id: SubscriptionId(*subscription_id),
            });
        }

        for subscription_id in &matching {
            self.subscribers
                .get_mut(subscription_id)
                .expect("matching subscriber disappeared")
                .queue
                .push_back(message.clone());
        }
        Ok(matching.len())
    }

    /// Removes and returns the oldest message for a consumer.
    pub fn receive(&mut self, subscription: SubscriptionId) -> Option<T> {
        self.subscribers
            .get_mut(&subscription.0)
            .and_then(|subscriber| subscriber.queue.pop_front())
    }

    pub fn unsubscribe(&mut self, subscription: SubscriptionId) -> bool {
        self.subscribers.remove(&subscription.0).is_some()
    }

    pub fn queued_messages(&self, subscription: SubscriptionId) -> Option<usize> {
        self.subscribers
            .get(&subscription.0)
            .map(|subscriber| subscriber.queue.len())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubscriptionId(u64);

#[cfg(test)]
mod tests {
    use super::{InMemoryTransport, SendError};

    #[test]
    fn publishes_fifo_to_every_matching_consumer() {
        let mut transport = InMemoryTransport::new();
        let storage = transport.subscribe("telemetry", 2);
        let alerts = transport.subscribe("telemetry", 2);
        let unrelated = transport.subscribe("control", 2);

        assert_eq!(transport.publish("telemetry", 1), Ok(2));
        assert_eq!(transport.publish("telemetry", 2), Ok(2));
        assert_eq!(transport.receive(storage), Some(1));
        assert_eq!(transport.receive(storage), Some(2));
        assert_eq!(transport.receive(alerts), Some(1));
        assert_eq!(transport.receive(alerts), Some(2));
        assert_eq!(transport.receive(unrelated), None);
    }

    #[test]
    fn full_consumer_rejects_atomically_and_exposes_backpressure() {
        let mut transport = InMemoryTransport::new();
        let first = transport.subscribe("telemetry", 1);
        let second = transport.subscribe("telemetry", 2);

        assert_eq!(transport.publish("telemetry", "first"), Ok(2));
        assert_eq!(
            transport.publish("telemetry", "second"),
            Err(SendError::Backpressure {
                topic: "telemetry".into(),
                subscription_id: first,
            })
        );
        assert_eq!(transport.queued_messages(first), Some(1));
        assert_eq!(transport.queued_messages(second), Some(1));
    }

    #[test]
    fn unsubscribe_stops_fan_out_without_affecting_other_consumers() {
        let mut transport = InMemoryTransport::new();
        let retained = transport.subscribe("telemetry", 1);
        let removed = transport.subscribe("telemetry", 1);
        assert!(transport.unsubscribe(removed));
        assert!(!transport.unsubscribe(removed));

        assert_eq!(transport.publish("telemetry", 42), Ok(1));
        assert_eq!(transport.receive(retained), Some(42));
    }
}
