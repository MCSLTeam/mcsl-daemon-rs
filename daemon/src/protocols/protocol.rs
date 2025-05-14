pub trait Protocol {
    async fn process_text(&self, raw: String) -> Option<String>;
    async fn process_binary(&self, raw: Vec<u8>) -> Option<Vec<u8>>;
}
