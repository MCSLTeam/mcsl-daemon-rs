use axum::extract::ws::Message;
use mcsl_protocol::v1::action::{ActionRequest, ActionResponse};

pub trait Protocol {
    async fn process_text_request<'req>(
        &self,
        raw: &'req str,
    ) -> Result<ActionRequest<'req>, ActionResponse>;

    async fn process_bin_request<'req>(
        &self,
        raw: &'req [u8],
    ) -> Result<ActionRequest<'req>, ActionResponse>;

    async fn process_text(&self, raw: &str) -> Option<Message>;
    async fn process_binary(&self, raw: &[u8]) -> Option<Message>;

    async fn handle_text_rate_limit_exceed(&self, raw: &str) -> Option<Message>;
    async fn handle_bin_rate_limit_exceed(&self, raw: &[u8]) -> Option<Message>;
}
