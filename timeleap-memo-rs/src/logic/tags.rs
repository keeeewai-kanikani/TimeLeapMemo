use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpressionTag {
    pub virtual_time: f32,
    pub label: String,
    pub color: [f32; 3], // [r, g, b]
}

impl ImpressionTag {
    pub fn new(vt: f32, label: String) -> Self {
        Self {
            virtual_time: vt,
            label,
            color: [0.1, 0.6, 0.9], // Default blue-ish
        }
    }
}
