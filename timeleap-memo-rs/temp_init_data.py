import struct

# bincode format for (f32, Vec<Stroke>, Option<Vec<ImpressionTag>>)
# f32 (4 bytes)
# Vec<Stroke>: len (8 bytes) + data
# Option<Vec<ImpressionTag>>: tag (1 byte, 0 for None, 1 for Some) + [len (8 bytes) + data]

with open('data_empty.bin', 'wb') as f:
    # virtual_time: 0.0 (f32)
    f.write(struct.pack('<f', 0.0))
    # strokes: len 0 (u64)
    f.write(struct.pack('<Q', 0))
    # tags: Option::None (u8: 0)
    # Wait, the adapter uses SaveData which has tags as Option<&[ImpressionTag]>
    # If I use Some(Vec::new()), it's tag 1 + len 0.
    # In LoadData, it's Option<Vec<ImpressionTag>>, so 1 followed by 0 (u64) is empty Some.
    f.write(struct.pack('<B', 1))
    f.write(struct.pack('<Q', 0))

print("Created data_empty.bin")
