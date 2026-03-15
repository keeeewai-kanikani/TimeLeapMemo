use crate::logic::Stroke;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

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
