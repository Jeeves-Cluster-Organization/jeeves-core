//! Sliding-window context compaction.

use agent_core::{ChatMessage, TransformContext};

/// Keeps the first `head` messages (typically the first user turn) and the
/// last `tail` messages, dropping anything in between once the window exceeds
/// `head + tail + 1` messages.
#[derive(Debug, Clone)]
pub struct SlidingWindow {
    pub head: usize,
    pub tail: usize,
}

impl Default for SlidingWindow {
    fn default() -> Self {
        Self { head: 1, tail: 20 }
    }
}

impl TransformContext for SlidingWindow {
    fn transform(&self, messages: &mut Vec<ChatMessage>) {
        if messages.len() <= self.head + self.tail + 1 {
            return;
        }
        let mut kept = Vec::with_capacity(self.head + self.tail + 1);
        kept.extend(messages.iter().take(self.head).cloned());
        kept.push(ChatMessage::system(format!(
            "[compacted {} messages]",
            messages.len() - self.head - self.tail
        )));
        let start = messages.len().saturating_sub(self.tail);
        kept.extend(messages[start..].iter().cloned());
        *messages = kept;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_compaction_below_threshold() {
        let w = SlidingWindow { head: 1, tail: 3 };
        let mut msgs: Vec<ChatMessage> =
            (0..4).map(|i| ChatMessage::user(format!("{i}"))).collect();
        let before = msgs.len();
        w.transform(&mut msgs);
        assert_eq!(msgs.len(), before);
    }

    #[test]
    fn drops_middle_above_threshold() {
        let w = SlidingWindow { head: 1, tail: 2 };
        let mut msgs: Vec<ChatMessage> =
            (0..10).map(|i| ChatMessage::user(format!("{i}"))).collect();
        w.transform(&mut msgs);
        assert_eq!(msgs.len(), 1 + 1 + 2); // head + marker + tail
        assert_eq!(msgs[1].role, "system");
    }
}
