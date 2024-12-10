pub trait Protocol {
    async fn process_text(&self, raw: &str) -> Option<String> {
        None
    }
    async fn process_binary(&self, raw: &[u8]) -> Option<Vec<u8>> {
        None
    }
}
