use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum Message {
    /// Players
    Players(Vec<PlayerInfo>),
    /// Map bytestream, chunked
    InitMap(MapInfo),
    MapChunk(MapChunk),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapChunk {
    pub i: u32,
    pub data: String, // base64 encoded jpeg image
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapInfo {
    pub chunks: u32,
    pub name: String,
    pub upper_left_x: f32,
    pub upper_left_y: f32,
    pub scale: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub team: String,
    pub is_local: bool,
    pub health: u32,
    pub position: [f32; 3],
    pub ang_rotation: [f32; 2],
}
