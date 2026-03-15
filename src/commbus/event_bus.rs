//! Event pub/sub methods for CommBus.

use super::types::{Event, Subscriber, Subscription};
use super::CommBus;
use crate::types::Result;
use tokio::sync::mpsc;

impl CommBus {
    /// Publish an event to all subscribers.
    ///
    /// This is a fan-out operation - the event is delivered to ALL subscribers
    /// that have registered interest in this event_type.
    pub fn publish(&mut self, event: Event) -> Result<usize> {
        // Find all subscribers interested in this event type
        let interested = self.subscribers
            .get(&event.event_type)
            .map(|subs| subs.as_slice())
            .unwrap_or(&[]);

        let mut delivered = 0;
        for subscriber in interested {
            // Fire-and-forget send to subscriber
            // If channel is closed, subscriber has disconnected (we'll clean them up later)
            if subscriber.tx.send(event.clone()).is_ok() {
                delivered += 1;
            }
        }

        // Update stats
        self.stats.events_published += 1;

        tracing::debug!(
            "Published event type={} to {} subscribers",
            event.event_type,
            delivered
        );

        Ok(delivered)
    }

    /// Subscribe to event types.
    ///
    /// Returns (subscription handle, receiver channel) for receiving events.
    pub fn subscribe(
        &mut self,
        subscriber_id: String,
        event_types: Vec<String>,
    ) -> Result<(Subscription, mpsc::UnboundedReceiver<Event>)> {
        let (tx, rx) = mpsc::unbounded_channel();

        let subscriber = Subscriber {
            id: subscriber_id.clone(),
            event_types: event_types.clone(),
            tx,
        };

        // Register subscriber for each event type
        for event_type in &event_types {
            self.subscribers
                .entry(event_type.clone())
                .or_default()
                .push(Subscriber {
                    id: subscriber.id.clone(),
                    event_types: subscriber.event_types.clone(),
                    tx: subscriber.tx.clone(),
                });
        }

        // Update stats
        self.stats.active_subscribers = self.subscribers.values().map(|v| v.len()).sum();

        tracing::debug!(
            "Subscriber {} registered for events: {:?}",
            subscriber_id,
            event_types
        );

        Ok((
            Subscription {
                id: subscriber_id,
                event_types,
            },
            rx,
        ))
    }

    /// Unsubscribe a subscriber by ID.
    ///
    /// Removes the subscriber from all event type lists.
    pub fn unsubscribe(&mut self, subscription: &Subscription) {
        for event_type in &subscription.event_types {
            if let Some(subs) = self.subscribers.get_mut(event_type) {
                subs.retain(|s| s.id != subscription.id);
            }
        }
        // Update stats
        self.stats.active_subscribers = self.subscribers.values().map(|v| v.len()).sum();

        tracing::debug!("Unsubscribed: {}", subscription.id);
    }

    /// Prune disconnected subscribers from all event type lists.
    pub fn cleanup_disconnected(&mut self) {
        for subs in self.subscribers.values_mut() {
            subs.retain(|s| !s.tx.is_closed());
        }
        // Update stats
        self.stats.active_subscribers = self.subscribers.values().map(|v| v.len()).sum();
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn test_publish_to_zero_subscribers() {
        let mut bus = CommBus::new();

        let result = bus.publish(Event::test("test.event", b"{}".to_vec()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);

        let stats = bus.get_stats();
        assert_eq!(stats.events_published, 1);
    }

    #[test]
    fn test_subscribe_and_publish() {
        let mut bus = CommBus::new();

        let (_subscription, mut rx) = bus
            .subscribe("subscriber1".to_string(), vec!["test.event".to_string()])
            .unwrap();

        let event = Event::test("test.event", b"{\"msg\":\"hello\"}".to_vec());
        let delivered = bus.publish(event).unwrap();
        assert_eq!(delivered, 1);

        let received = rx.try_recv().unwrap();
        assert_eq!(received.event_type, "test.event");
    }

    #[test]
    fn test_multiple_subscribers_fan_out() {
        let mut bus = CommBus::new();

        let (_sub1, mut rx1) = bus
            .subscribe("sub1".to_string(), vec!["test.event".to_string()])
            .unwrap();
        let (_sub2, mut rx2) = bus
            .subscribe("sub2".to_string(), vec!["test.event".to_string()])
            .unwrap();

        let delivered = bus.publish(Event::test("test.event", b"{}".to_vec())).unwrap();
        assert_eq!(delivered, 2);

        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    #[test]
    fn test_subscribe_multiple_event_types() {
        let mut bus = CommBus::new();

        let (_sub, mut rx) = bus
            .subscribe("sub1".to_string(), vec!["event.a".to_string(), "event.b".to_string()])
            .unwrap();

        bus.publish(Event::test("event.a", b"{}".to_vec())).unwrap();
        bus.publish(Event::test("event.b", b"{}".to_vec())).unwrap();

        assert_eq!(rx.try_recv().unwrap().event_type, "event.a");
        assert_eq!(rx.try_recv().unwrap().event_type, "event.b");
    }

    #[test]
    fn test_unsubscribe_removes_subscriber() {
        let mut bus = CommBus::new();

        let (sub, _rx) = bus
            .subscribe("sub1".to_string(), vec!["test.event".to_string()])
            .unwrap();
        assert_eq!(bus.get_stats().active_subscribers, 1);

        bus.unsubscribe(&sub);

        let delivered = bus.publish(Event::test("test.event", b"{}".to_vec())).unwrap();
        assert_eq!(delivered, 0);
    }

    #[test]
    fn test_unsubscribe_only_affects_target() {
        let mut bus = CommBus::new();

        let (sub1, _rx1) = bus
            .subscribe("sub1".to_string(), vec!["test.event".to_string()])
            .unwrap();
        let (_sub2, mut rx2) = bus
            .subscribe("sub2".to_string(), vec!["test.event".to_string()])
            .unwrap();

        bus.unsubscribe(&sub1);

        let delivered = bus.publish(Event::test("test.event", b"{}".to_vec())).unwrap();
        assert_eq!(delivered, 1);
        assert!(rx2.try_recv().is_ok());
    }

    #[test]
    fn test_cleanup_disconnected_prunes_dead_subscribers() {
        let mut bus = CommBus::new();

        let (_sub1, rx1) = bus
            .subscribe("sub1".to_string(), vec!["test.event".to_string()])
            .unwrap();
        let (_sub2, _rx2) = bus
            .subscribe("sub2".to_string(), vec!["test.event".to_string()])
            .unwrap();
        assert_eq!(bus.get_stats().active_subscribers, 2);

        drop(rx1);
        bus.cleanup_disconnected();
        assert_eq!(bus.get_stats().active_subscribers, 1);

        let delivered = bus.publish(Event::test("test.event", b"{}".to_vec())).unwrap();
        assert_eq!(delivered, 1);
    }
}
