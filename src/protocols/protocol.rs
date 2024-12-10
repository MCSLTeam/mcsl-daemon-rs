pub trait Protocol {
    async fn process_text(&self, raw: &str) -> Option<String>;
    async fn process_binary(&self, raw: &[u8]) -> Option<Vec<u8>>;
}
