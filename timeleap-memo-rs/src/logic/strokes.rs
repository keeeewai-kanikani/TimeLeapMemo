use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
}

const MAX_POINTS_PER_STROKE: usize = 2000;
const MIN_DIST_SQ: f32 = 0.000004; // 0.002 * 0.002 (normalized coordinates)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stroke {
    pub points: Vec<Point>,
    pub width: f32,
    pub color: [f32; 3], // [r, g, b]
    pub virtual_time_created: f32,
    pub erased_at: Option<f32>,
    pub segment_id: usize, // 世界線（セグメント）ID

    // --- UI/Transient state ---
    #[serde(skip)]
    pub cached_path: Option<slint::SharedString>,
    #[serde(skip)]
    pub last_render_res: (f32, f32),
}

impl Stroke {
    pub fn new(virtual_time: f32) -> Self {
        Self {
            points: Vec::new(),
            width: 6.0,
            color: [0.0, 0.0, 0.0],
            virtual_time_created: virtual_time,
            erased_at: None,
            segment_id: 0,
            cached_path: None,
            last_render_res: (0.0, 0.0),
        }
    }

    pub fn add_point(&mut self, x: f32, y: f32, pressure: f32) {
        if self.points.len() >= MAX_POINTS_PER_STROKE {
            return;
        }

        // Distance-based decimation (iOS-style simplification)
        if let Some(last) = self.points.last() {
            let dx = x - last.x;
            let dy = y - last.y;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < MIN_DIST_SQ {
                return;
            }
        }

        self.points.push(Point { x, y, pressure });
        
        // Invalidate cache
        self.cached_path = None;
    }

    #[allow(dead_code)]
    pub fn get_alpha(&self, current_virtual_time: f32, lambda: f32) -> f32 {
        let age = current_virtual_time - self.virtual_time_created;
        if age < 0.0 {
            0.0
        } else {
            (-lambda * age).exp()
        }
    }
}
