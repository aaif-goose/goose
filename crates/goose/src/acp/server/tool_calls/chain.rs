use crate::conversation::message::{MessageContent, ToolRequest};

pub(crate) fn breaks_consecutive_tool_calls(content: &MessageContent) -> bool {
    matches!(
        content,
        MessageContent::Text(_) | MessageContent::Thinking(_) | MessageContent::Image(_)
    )
}

#[derive(Debug)]
struct ToolChainStep {
    request: ToolRequest,
    responded: bool,
}

#[derive(Debug)]
struct TrackedToolChain {
    steps: Vec<ToolChainStep>,
}

impl TrackedToolChain {
    fn new(request: ToolRequest) -> Self {
        Self {
            steps: vec![ToolChainStep {
                request,
                responded: false,
            }],
        }
    }

    fn add_request(&mut self, request: ToolRequest) {
        self.steps.push(ToolChainStep {
            request,
            responded: false,
        });
    }

    fn contains(&self, tool_call_id: &str) -> bool {
        self.steps
            .iter()
            .any(|step| step.request.id == tool_call_id)
    }

    fn mark_responded(&mut self, tool_call_id: &str) {
        let Some(step) = self
            .steps
            .iter_mut()
            .find(|step| step.request.id == tool_call_id)
        else {
            return;
        };

        step.responded = true;
    }

    fn is_complete(&self) -> bool {
        self.steps.iter().all(|step| step.responded)
    }

    fn into_ready(self) -> ReadyToolChain {
        ReadyToolChain {
            tool_requests: self.steps.into_iter().map(|step| step.request).collect(),
        }
    }
}

pub(crate) struct ReadyToolChain {
    pub(crate) tool_requests: Vec<ToolRequest>,
}

/// Tracks tool-chain membership and readiness for one ACP prompt stream.
#[derive(Default)]
pub(crate) struct ToolChainTracker {
    current_chain: Option<TrackedToolChain>,
    waiting_chains: Vec<TrackedToolChain>,
}

impl ToolChainTracker {
    pub(crate) fn record_request(&mut self, request: ToolRequest) {
        if let Some(chain) = &mut self.current_chain {
            chain.add_request(request);
        } else {
            self.current_chain = Some(TrackedToolChain::new(request));
        }
    }

    pub(crate) fn record_response(&mut self, tool_call_id: &str) -> Option<ReadyToolChain> {
        if let Some(current_chain) = &mut self.current_chain {
            if current_chain.contains(tool_call_id) {
                current_chain.mark_responded(tool_call_id);
                return None;
            }
        }

        let waiting_chain_index = self
            .waiting_chains
            .iter()
            .position(|chain| chain.contains(tool_call_id))?;

        let waiting_chain = &mut self.waiting_chains[waiting_chain_index];
        waiting_chain.mark_responded(tool_call_id);

        if !waiting_chain.is_complete() {
            return None;
        }

        let ready_chain = self.waiting_chains.remove(waiting_chain_index);
        Some(ready_chain.into_ready())
    }

    pub(crate) fn close_current_chain(&mut self) -> Option<ReadyToolChain> {
        let chain = self.current_chain.take()?;
        if chain.steps.len() < 2 {
            return None;
        }

        if !chain.is_complete() {
            self.waiting_chains.push(chain);
            return None;
        }

        Some(chain.into_ready())
    }
}

#[cfg(test)]
mod tests {
    use super::ToolChainTracker;
    use crate::conversation::message::ToolRequest;
    use rmcp::model::CallToolRequestParams;

    fn request(id: &str) -> ToolRequest {
        ToolRequest {
            id: id.to_string(),
            tool_call: Ok(CallToolRequestParams::new(format!("tool-{id}"))),
            metadata: None,
            tool_meta: None,
        }
    }

    fn request_ids(chain: &super::ReadyToolChain) -> Vec<&str> {
        chain
            .tool_requests
            .iter()
            .map(|request| request.id.as_str())
            .collect()
    }

    #[test]
    fn open_chain_waits_for_a_boundary() {
        let mut tracker = ToolChainTracker::default();

        for id in ["a", "b", "c"] {
            tracker.record_request(request(id));
            assert!(tracker.record_response(id).is_none());
        }

        let ready = tracker.close_current_chain().expect("A-B-C is ready");
        assert_eq!(request_ids(&ready), ["a", "b", "c"]);
    }

    #[test]
    fn closed_chain_waits_for_its_last_response() {
        let mut tracker = ToolChainTracker::default();
        for id in ["a", "b", "c"] {
            tracker.record_request(request(id));
        }
        tracker.record_response("a");
        tracker.record_response("b");

        assert!(tracker.close_current_chain().is_none());

        let ready = tracker.record_response("c").expect("A-B-C is ready");
        assert_eq!(request_ids(&ready), ["a", "b", "c"]);
    }

    #[test]
    fn boundary_separates_request_runs_and_discards_singletons() {
        let mut tracker = ToolChainTracker::default();
        for id in ["a", "b"] {
            tracker.record_request(request(id));
            tracker.record_response(id);
        }

        let first = tracker.close_current_chain().expect("A-B is ready");
        assert_eq!(request_ids(&first), ["a", "b"]);

        tracker.record_request(request("c"));
        tracker.record_response("c");
        assert!(tracker.close_current_chain().is_none());

        for id in ["d", "e"] {
            tracker.record_request(request(id));
            tracker.record_response(id);
        }
        let second = tracker.close_current_chain().expect("D-E is ready");
        assert_eq!(request_ids(&second), ["d", "e"]);
    }
}
