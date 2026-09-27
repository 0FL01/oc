use oc_core::tool_stream::{
    ARGUMENT_PREVIEW_MAX, PENDING_TOOL_MAX, ToolStreamEvent, ToolStreamIdentity,
};

struct Pending {
    identity: ToolStreamIdentity,
    name: String,
    preview: String,
    truncated: bool,
    dirty: bool,
    since_emit: usize,
}

impl Pending {
    fn snapshot(&mut self) -> ToolStreamEvent {
        self.dirty = false;
        self.since_emit = 0;
        ToolStreamEvent::Pending {
            identity: self.identity.clone(),
            name: self.name.clone(),
            preview: self.preview.clone(),
            truncated: self.truncated,
        }
    }
}

/// The provider assembler owns full arguments. Presentation retains only a
/// prefix and emits at most once per 256 newly retained bytes, plus boundaries.
pub(super) struct PendingToolStreams {
    round: u32,
    calls: Vec<Pending>,
}

impl PendingToolStreams {
    pub(super) fn new(round: u32) -> Self {
        Self {
            round,
            calls: Vec::new(),
        }
    }

    pub(super) fn announce(
        &mut self,
        item_id: &str,
        call_id: &str,
        name: &str,
    ) -> Option<ToolStreamEvent> {
        if item_id.is_empty()
            || call_id.is_empty()
            || name.is_empty()
            || item_id.len() > 512
            || call_id.len() > 512
            || name.len() > 512
            || self.calls.len() >= PENDING_TOOL_MAX
            || self
                .calls
                .iter()
                .any(|call| call.identity.item_id == item_id)
        {
            return None;
        }
        let mut call = Pending {
            identity: ToolStreamIdentity {
                round: self.round,
                item_id: item_id.into(),
                call_id: call_id.into(),
            },
            name: name.into(),
            preview: String::new(),
            truncated: false,
            dirty: false,
            since_emit: 0,
        };
        let event = call.snapshot();
        self.calls.push(call);
        Some(event)
    }

    pub(super) fn delta(&mut self, item_id: &str, delta: &str) -> Option<ToolStreamEvent> {
        let call = self
            .calls
            .iter_mut()
            .find(|call| call.identity.item_id == item_id)?;
        if call.truncated || delta.is_empty() {
            return None;
        }
        let mut len = delta.len().min(ARGUMENT_PREVIEW_MAX - call.preview.len());
        while !delta.is_char_boundary(len) {
            len -= 1;
        }
        call.preview.push_str(&delta[..len]);
        call.truncated = len < delta.len();
        call.dirty = true;
        call.since_emit += len;
        (call.since_emit >= 256 || call.truncated).then(|| call.snapshot())
    }

    pub(super) fn flush(&mut self) -> Vec<ToolStreamEvent> {
        self.calls
            .iter_mut()
            .filter(|call| call.dirty)
            .map(Pending::snapshot)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announcement_split_utf8_caps_and_coalescing() {
        let mut streams = PendingToolStreams::new(2);
        assert!(
            matches!(streams.announce("item", "call", "apply_patch"), Some(ToolStreamEvent::Pending { name, preview, identity, .. }) if name == "apply_patch" && preview.is_empty() && identity.round == 2)
        );
        assert!(streams.announce("item", "different", "bash").is_none());
        assert!(streams.delta("wrong", "x").is_none());
        assert!(streams.delta("item", "{\"patch").is_none());
        assert!(streams.delta("item", "Text\":\"界").is_none());
        let events = streams.flush();
        assert!(
            matches!(&events[0], ToolStreamEvent::Pending { preview, .. } if preview == "{\"patchText\":\"界")
        );
        assert!(streams.flush().is_empty());
        let event = streams.delta("item", &"界".repeat(4096)).unwrap();
        assert!(
            matches!(event, ToolStreamEvent::Pending { preview, truncated: true, .. } if preview.len() <= ARGUMENT_PREVIEW_MAX)
        );
        assert!(streams.delta("item", &"x".repeat(100_000)).is_none());
        for i in 0..100 {
            streams.announce(&format!("i{i}"), &format!("c{i}"), "unknown");
        }
        assert_eq!(streams.calls.len(), PENDING_TOOL_MAX);
    }
}
