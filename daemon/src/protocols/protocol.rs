use mcsl_protocol::v1::action::{ActionRequest, ActionResponse};

pub trait Protocol {
    fn process_text_request<'req>(
        &self,
        raw: &'req str,
    ) -> Result<ActionRequest<'req>, ActionResponse>;

    fn process_bin_request<'req>(
        &self,
        raw: &'req [u8],
    ) -> Result<ActionRequest<'req>, ActionResponse>;
    async fn process_text(&self, raw: &str) -> Option<String>;
    async fn process_binary(&self, raw: &[u8]) -> Option<Vec<u8>>;

    fn handle_text_rate_limit_exceed(&self, raw: &str) -> Option<String>;
    fn handle_bin_rate_limit_exceed(&self, raw: &[u8]) -> Option<Vec<u8>>;
}
