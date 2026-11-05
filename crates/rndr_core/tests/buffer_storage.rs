//! Raw storage invariants, including deliberately split UTF-8 byte edits.
use rndr_core::Buffer;

fn check(buffer: &Buffer, bytes: &[u8]) {
    assert_eq!(buffer.len(), bytes.len());
    assert_eq!(buffer.text(), String::from_utf8_lossy(bytes));
    let starts: Vec<_> = std::iter::once(0)
        .chain(
            bytes
                .iter()
                .enumerate()
                .filter_map(|(i, b)| (*b == b'\n' && i + 1 < bytes.len()).then_some(i + 1)),
        )
        .collect();
    assert_eq!(buffer.line_count() as usize, starts.len());
    for (i, start) in starts.iter().enumerate() {
        assert_eq!(buffer.line_start(i as u32), *start);
    }
    assert_eq!(buffer.line_start(starts.len() as u32), bytes.len());
}

#[test]
fn gap_moves_and_line_index_match_raw_byte_model() {
    for seed in ["", "\n", "a\n", "\n\n", "a\nb\nc", "é\n👩‍💻\r\nالعربية"] {
        for insertion in ["x", "\n", "a\nb\n", "👩‍💻"] {
            for at in 0..=seed.len() {
                let mut buffer = Buffer::from_str(seed);
                let mut bytes = seed.as_bytes().to_vec();
                buffer.insert_str(at, insertion);
                bytes.splice(at..at, insertion.bytes());
                check(&buffer, &bytes);
                // Move the existing gap in both directions and through split
                // scalars without silently repairing the stored bytes.
                for pos in [0, bytes.len() / 2, bytes.len()] {
                    buffer.insert_str(pos, "\nZ");
                    bytes.splice(pos..pos, b"\nZ".iter().copied());
                    check(&buffer, &bytes);
                }
                for start in 0..=bytes.len() {
                    for end in start..=bytes.len() {
                        let mut deleted = buffer.clone();
                        let mut expected = bytes.clone();
                        deleted.delete_range(start, end);
                        expected.drain(start..end);
                        check(&deleted, &expected);
                    }
                }
            }
        }
    }
}
