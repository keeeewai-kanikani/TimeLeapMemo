use crate::logic::Stroke;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;
use serde::{Serialize, Deserialize};

#[allow(dead_code)]
pub fn save_to_binary<P: AsRef<Path>>(path: P, strokes: &[Stroke], virtual_time: f32) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    
    // We can wrap strokes and metadata in a single serializable struct
    #[derive(serde::Serialize)]
    struct SaveData<'a> {
        virtual_time: f32,
        strokes: &'a [Stroke],
    }
    
    let data = SaveData {
        virtual_time,
        strokes,
    };
    
    bincode::serialize_into(writer, &data)?;
    Ok(())
}

#[allow(dead_code)]
pub fn load_from_binary<P: AsRef<Path>>(path: P) -> Result<(f32, Vec<Stroke>), Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    
    #[derive(serde::Deserialize)]
    struct LoadData {
        virtual_time: f32,
        strokes: Vec<Stroke>,
    }
    
    let data: LoadData = bincode::deserialize_from(reader)?;
    Ok((data.virtual_time, data.strokes))
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppSettings {
    pub last_vt: f32,
    pub is_maximized: bool,
    pub macro_ratio: f32,
    pub middle_ratio: f32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            last_vt: 0.0,
            is_maximized: true,
            macro_ratio: 0.0,
            middle_ratio: 0.0,
        }
    }
}

pub fn save_settings<P: AsRef<Path>>(path: P, settings: &AppSettings) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, settings)?;
    Ok(())
}

pub fn load_settings<P: AsRef<Path>>(path: P) -> Result<AppSettings, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let settings = serde_json::from_reader(reader)?;
    Ok(settings)
}
