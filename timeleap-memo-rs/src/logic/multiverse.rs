use crate::logic::Stroke;

#[derive(Debug, Clone)]
pub struct Segment {
    pub id: usize,
    pub stroke_indices: Vec<usize>,
}

#[derive(Debug)]
pub struct WorldLineManager {
    pub segments: Vec<Segment>,
    next_segment_id: usize,
}

impl WorldLineManager {
    pub fn new() -> Self {
        Self {
            segments: vec![Segment {
                id: 0,
                stroke_indices: Vec::new(),
            }],
            next_segment_id: 1,
        }
    }

    /// ストロークリストからセグメントを計算（Python版calc_segments()の実装）
    pub fn calculate_segments(&mut self, strokes: &[Stroke]) {
        self.segments.clear();
        self.next_segment_id = 0;

        if strokes.is_empty() {
            self.segments.push(Segment {
                id: 0,
                stroke_indices: Vec::new(),
            });
            return;
        }

        let mut current_segment = Segment {
            id: 0,
            stroke_indices: Vec::new(),
        };

        let mut prev_time = strokes[0].virtual_time_created;

        for (idx, stroke) in strokes.iter().enumerate() {
            let current_time = stroke.virtual_time_created;

            // 時間が減少 = 新しい世界線（セグメント）が分岐
            if current_time < prev_time && !current_segment.stroke_indices.is_empty() {
                self.segments.push(current_segment);
                self.next_segment_id += 1;
                current_segment = Segment {
                    id: self.next_segment_id,
                    stroke_indices: vec![idx],
                };
            } else {
                current_segment.stroke_indices.push(idx);
            }

            prev_time = current_time;
        }

        // 最後のセグメントを追加
        self.segments.push(current_segment);
    }


    /// セグメントに属するストロークインデックスリストを取得
    pub fn get_segment_strokes(&self, segment_id: usize) -> Vec<usize> {
        self.segments
            .iter()
            .find(|s| s.id == segment_id)
            .map(|s| s.stroke_indices.clone())
            .unwrap_or_default()
    }

    /// 全セグメント数を取得
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }
}

impl Default for WorldLineManager {
    fn default() -> Self {
        Self::new()
    }
}
