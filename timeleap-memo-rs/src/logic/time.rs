pub struct VirtualTime {
    current: f32,
    max: f32,
    is_playing: bool,
}

impl VirtualTime {
    pub fn new() -> Self {
        Self {
            current: 0.0,
            max: 0.0,
            is_playing: false,
        }
    }

    pub fn advance(&mut self, dt: f32) {
        if self.is_playing {
            self.current += dt;
            if self.current > self.max {
                self.max = self.current;
            }
        }
    }

    pub fn get_current(&self) -> f32 {
        self.current
    }

    pub fn set_current(&mut self, time: f32) {
        self.current = time;
    }

    pub fn update_max(&mut self, time: f32) {
        if time > self.max {
            self.max = time;
        }
    }

    pub fn get_max(&self) -> f32 {
        self.max
    }

    pub fn toggle_play(&mut self) {
        self.is_playing = !self.is_playing;
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing
    }

    pub fn set_playing(&mut self, playing: bool) {
        self.is_playing = playing;
    }

    pub fn reset(&mut self) {
        self.current = 0.0;
        self.max = 0.0;
        self.is_playing = false;
    }
}
