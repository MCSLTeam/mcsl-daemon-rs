use ring::rand::{SecureRandom, SystemRandom};

const BASE64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(input: &[u8]) -> String {
    let mut output = String::new();
    let mut buffer = 0;
    let mut bits_collected = 0;

    for &byte in input {
        buffer = (buffer << 8) | byte as u32;
        bits_collected += 8;

        while bits_collected >= 6 {
            bits_collected -= 6;
            let index = (buffer >> bits_collected) & 0b111111;
            output.push(BASE64_TABLE[index as usize] as char);
        }
    }

    if bits_collected > 0 {
        buffer <<= 6 - bits_collected;
        let index = buffer & 0b111111;
        output.push(BASE64_TABLE[index as usize] as char);
    }

    while output.len() % 4 != 0 {
        output.push('=');
    }

    output
}

pub fn base64_decode(input: &str) -> Result<Vec<u8>, &'static str> {
    let mut output = Vec::new();
    let mut buffer = 0;
    let mut bits_collected = 0;

    for &byte in input.as_bytes() {
        if byte == b'=' {
            break;
        }

        let value = if byte.is_ascii_uppercase() {
            byte - b'A'
        } else if byte.is_ascii_lowercase() {
            byte - b'a' + 26
        } else if byte.is_ascii_digit() {
            byte - b'0' + 52
        } else if byte == b'+' {
            62
        } else if byte == b'/' {
            63
        } else {
            return Err("Invalid character in input");
        };

        buffer = (buffer << 6) | value as u32;
        bits_collected += 6;

        if bits_collected >= 8 {
            bits_collected -= 8;
            output.push((buffer >> bits_collected) as u8);
        }
    }

    Ok(output)
}

pub fn get_random_string(len: usize) -> String {
    let rng = SystemRandom::new();
    let mut buf = vec![0u8; len];
    rng.fill(&mut buf)
        .expect("Failed to generate random password");
    base64_encode(&buf)
}
