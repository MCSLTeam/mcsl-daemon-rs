use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
pub struct SlpStatus {
    pub payload: PingPayload,
    pub latency: std::time::Duration,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SlpLegacyStatus {
    pub motd: String,
    pub players_online: i32,
    pub max_players: i32,
    pub ping_version: i32,
    pub protocol_version: i32,
    pub game_version: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PingPayload {
    pub version: VersionPayload,
    pub players: PlayersPayload,
    #[serde(with = "description_serde")]
    pub description: String,
}

mod description_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use serde_json::Value;

    pub fn serialize<S>(value: &String, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(value)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        match value {
            Value::String(s) => Ok(s),
            Value::Object(obj) => Ok(obj["text"].as_str().unwrap_or("").to_string()),
            _ => Ok("".to_string()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VersionPayload {
    pub protocol: i32,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PlayersPayload {
    pub max: i32,
    pub online: i32,
    #[serde(default)]
    pub sample: Vec<PlayerSample>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PlayerSample {
    pub name: String,
    pub id: uuid::Uuid,
}

pub mod motd {
    use super::*;

    lazy_static! {
        static ref MINECRAFT_STYLES: HashMap<char, &'static str> = {
            let mut m = HashMap::new();
            m.insert('k', "none;font-weight:normal;font-style:normal");
            m.insert('m', "line-through;font-weight:normal;font-style:normal");
            m.insert('l', "none;font-weight:900;font-style:normal");
            m.insert('n', "underline;font-weight:normal;font-style:normal");
            m.insert('o', "none;font-weight:normal;font-style:italic");
            m.insert(
                'r',
                "none;font-weight:normal;font-style:normal;color:#FFFFFF",
            );
            m
        };
        static ref MINECRAFT_COLORS: HashMap<char, &'static str> = {
            let mut m = HashMap::new();
            m.insert('0', "#000000");
            m.insert('1', "#0000AA");
            m.insert('2', "#00AA00");
            m.insert('3', "#00AAAA");
            m.insert('4', "#AA0000");
            m.insert('5', "#AA00AA");
            m.insert('6', "#FFAA00");
            m.insert('7', "#AAAAAA");
            m.insert('8', "#555555");
            m.insert('9', "#5555FF");
            m.insert('a', "#55FF55");
            m.insert('b', "#55FFFF");
            m.insert('c', "#FF5555");
            m.insert('d', "#FF55FF");
            m.insert('e', "#FFFF55");
            m.insert('f', "#FFFFFF");
            m
        };
    }

    pub fn motd_html(motd: &str) -> String {
        let mut result = motd.to_string();
        let style_regex = Regex::new(r"§([k-oK-O])(.*?)(§[0-9a-fA-Fk-oK-OrR]|$)").unwrap();
        let color_regex = Regex::new(r"§([0-9a-fA-F])(.*?)(§[0-9a-fA-FrR]|$)").unwrap();

        while style_regex.is_match(&result) {
            result = style_regex
                .replace_all(&result, |caps: &regex::Captures| {
                    let style = MINECRAFT_STYLES
                        .get(&caps[1].chars().next().unwrap())
                        .unwrap();
                    format!(
                        "<span style=\"text-decoration:{}\">{}</span>{}",
                        style, &caps[2], &caps[3]
                    )
                })
                .to_string();
        }

        while color_regex.is_match(&result) {
            result = color_regex
                .replace_all(&result, |caps: &regex::Captures| {
                    let color = MINECRAFT_COLORS
                        .get(&caps[1].chars().next().unwrap())
                        .unwrap();
                    format!(
                        "<span style=\"color:{}\">{}</span>{}",
                        color, &caps[2], &caps[3]
                    )
                })
                .to_string();
        }

        result
    }
}
