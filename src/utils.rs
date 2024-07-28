use ring::rand::{SecureRandom, SystemRandom};

use crate::base64;

pub fn get_random_string(len: usize) -> String {
    let rng = SystemRandom::new();
    let mut buf = vec![0u8; len];
    rng.fill(&mut buf).expect("Failed to generate random password");
    base64::encode(&buf)
}