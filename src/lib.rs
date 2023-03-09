use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub team: String,
    pub is_local: bool,
    pub health: u32,
    pub position: [f32; 3],
    pub ang_rotation: [f32; 2],
}
