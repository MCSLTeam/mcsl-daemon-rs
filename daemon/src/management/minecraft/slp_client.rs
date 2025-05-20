use anyhow::{bail, Context, Result};
use encoding::codec::{utf_16::UTF_16BE_ENCODING, utf_8};
use encoding::{DecoderTrap, Encoding};
use log::{debug, error, warn};
use mcsl_protocol::management::minecraft::{PingPayload, SlpLegacyStatus, SlpStatus};
use std::fs;
use std::marker::PhantomData;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

// 状态机 trait
pub trait SlpClientState {}

// 未连接状态
pub struct Unconnected;
impl SlpClientState for Unconnected {}

// 已连接状态
pub struct Connected;
impl SlpClientState for Connected {}

pub struct SlpClient<TState: SlpClientState> {
    stream: Option<TcpStream>,
    buffer: Vec<u8>,
    _state: PhantomData<TState>,
}

impl SlpClient<Unconnected> {
    pub fn new() -> Self {
        SlpClient {
            stream: None,
            buffer: Vec::new(),
            _state: PhantomData,
        }
    }

    pub async fn handshake(self, host: &str, port: u16) -> Result<SlpClient<Connected>> {
        let stream = TcpStream::connect(format!("{}:{}", host, port))
            .await
            .context(format!("Failed to connect to {}:{}", host, port))?;
        let mut client = SlpClient {
            stream: Some(stream),
            buffer: Vec::new(),
            _state: PhantomData::<Connected>,
        };

        client.write_varint(47)?; // Protocol version
        client.write_string(host)?;
        client.write_short(port)?;
        client.write_varint(1)?; // Next state
        client.flush(0).await?;

        Ok(client)
    }
}

impl<TState: SlpClientState> SlpClient<TState> {
    fn write_short(&mut self, value: u16) -> Result<()> {
        self.buffer.extend_from_slice(&value.to_be_bytes());
        Ok(())
    }

    fn write_varint(&mut self, mut value: i32) -> Result<()> {
        while value >= 0x80 {
            self.buffer.push((value as u8) | 0x80);
            value >>= 7;
        }
        self.buffer.push(value as u8);
        Ok(())
    }

    fn write_long(&mut self, value: i64) -> Result<()> {
        self.buffer.extend_from_slice(&value.to_be_bytes());
        Ok(())
    }

    fn write_string(&mut self, value: &str) -> Result<()> {
        let data = value.as_bytes();
        self.write_varint(data.len() as i32)?;
        self.buffer.extend_from_slice(data);
        Ok(())
    }

    async fn flush(&mut self, id: i32) -> Result<()> {
        let data = std::mem::take(&mut self.buffer);
        let mut packet_data = vec![0x00];
        let mut add = 0;

        if id >= 0 {
            self.write_varint(id)?;
            packet_data = std::mem::take(&mut self.buffer);
            add = packet_data.len();
        }

        self.write_varint((data.len() + add) as i32)?;
        let buffer_length = std::mem::take(&mut self.buffer);

        let stream = self.stream.as_mut().context("Stream not initialized")?;
        stream.write_all(&buffer_length).await?;
        stream.write_all(&packet_data).await?;
        stream.write_all(&data).await?;
        stream.flush().await?;
        Ok(())
    }

    fn read_varint(data: &[u8], offset: &mut usize) -> Result<i32> {
        let mut result = 0;
        let mut shift = 0;
        loop {
            if *offset >= data.len() {
                anyhow::bail!("Unexpected end of data");
            }
            let b = data[*offset];
            *offset += 1;
            result |= ((b & 0x7F) as i32) << shift;
            if (b & 0x80) == 0 {
                return Ok(result);
            }
            shift += 7;
        }
    }

    fn read_long(data: &[u8], offset: &mut usize) -> Result<i64> {
        if *offset + 8 > data.len() {
            anyhow::bail!("Not enough data for long");
        }
        let value = i64::from_be_bytes(data[*offset..*offset + 8].try_into().unwrap());
        *offset += 8;
        Ok(value)
    }

    fn read_string(data: &[u8], length: i32, offset: &mut usize) -> Result<String> {
        if *offset + length as usize > data.len() {
            bail!("Not enough data for string");
        }
        let str = utf_8::UTF8Encoding {}
            .decode(
                &data[*offset..*offset + length as usize],
                DecoderTrap::Ignore,
            )
            .map_err(|e| anyhow::anyhow!(e))?;
        *offset += length as usize;
        Ok(str)
    }
}

impl SlpClient<Connected> {
    pub async fn get_status_modern(&mut self) -> Result<Option<SlpStatus>> {
        self.flush(0).await?;
        let mut received = vec![0u8; 65536];
        let n = self
            .stream
            .as_mut()
            .context("Stream not initialized")?
            .read(&mut received)
            .await?;
        let mut offset = 0;

        let length = Self::read_varint(&received, &mut offset)?;
        let packet_id = Self::read_varint(&received, &mut offset)?;
        let json_length = Self::read_varint(&received, &mut offset)?;
        debug!(
            "Received packetId 0x{:02x} with a length of {}",
            packet_id, length
        );

        let json = Self::read_string(&received, json_length, &mut offset)?;
        fs::write("slp.json", &json)?;
        let payload = serde_json::from_str::<PingPayload>(&json)
            .context("Failed to parse server ping payload")?;
        let latency = self.get_latency().await?;
        Ok(Some(SlpStatus { payload, latency }))
    }

    pub async fn get_latency(&mut self) -> Result<Duration> {
        let send_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .context("Failed to get system time")?;
        self.write_long(send_time)?;
        self.flush(1).await?;

        let mut received = [0u8; 16];
        self.stream
            .as_mut()
            .context("Stream not initialized")?
            .read(&mut received)
            .await
            .context("Failed to read pong packet")?;
        let received_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .context("Failed to get system time")?;

        let mut offset = 0;
        let length = Self::read_varint(&received, &mut offset)?;
        let packet_id = Self::read_varint(&received, &mut offset)?;
        debug!(
            "Received packetId 0x{:02x} with a length of {}",
            packet_id, length
        );

        let echo = Self::read_long(&received, &mut offset)?;
        if echo != send_time {
            warn!("Received echo time is not equal to send time");
        }
        Ok(Duration::from_millis((received_time - send_time) as u64))
    }
}

pub async fn get_status_legacy(host: &str, port: u16) -> Result<Option<SlpLegacyStatus>> {
    let mut stream = TcpStream::connect(format!("{}:{}", host, port))
        .await
        .context(format!("Failed to connect to {}:{}", host, port))?;
    stream.write_all(&[0xFE, 0x01]).await?;
    stream.flush().await?;

    let mut buffer = vec![0u8; 2048];
    let length = stream.read(&mut buffer).await?;

    if buffer[0] != 0xFF {
        error!("Received invalid packet");
        return Ok(None);
    }

    // 使用 encoding_rs::UTF_16BE 解码
    let payload = match UTF_16BE_ENCODING.decode(&buffer[3..length], DecoderTrap::Ignore) {
        Ok(s) => s,
        Err(_) => {
            bail!("Invalid UTF-16BE encoding")
        }
    };

    let data: Vec<&str> = payload.split('\0').collect();

    if payload.starts_with('§') {
        let ping_version = data[0][1..]
            .parse::<i32>()
            .context("Failed to parse ping version")?;
        let protocol_version = data[1]
            .parse::<i32>()
            .context("Failed to parse protocol version")?;
        let game_version = data[2].to_string();
        let motd = data[3].to_string();
        let players_online = data[4]
            .parse::<i32>()
            .context("Failed to parse players online")?;
        let max_players = data[5]
            .parse::<i32>()
            .context("Failed to parse max players")?;

        Ok(Some(SlpLegacyStatus {
            motd,
            players_online,
            max_players,
            ping_version,
            protocol_version,
            game_version,
        }))
    } else {
        let motd = data[0].to_string();
        let players_online = data[1]
            .parse::<i32>()
            .context("Failed to parse players online")?;
        let max_players = data[2]
            .parse::<i32>()
            .context("Failed to parse max players")?;

        Ok(Some(SlpLegacyStatus {
            motd,
            players_online,
            max_players,
            ping_version: 0,
            protocol_version: 0,
            game_version: String::new(),
        }))
    }
}
