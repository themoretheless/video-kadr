use std::io::Cursor;
use video_kadr_backend::domain::insta360_metadata::inspect;
fn fixture() -> Vec<u8> {
    fixture_with_extra(&[])
}
fn fixture_with_extra(extra: &[u8]) -> Vec<u8> {
    let mut data = vec![0; 32];
    let base = data.len();
    let mut payload = b"\x0a\x06secret\x12\x0bInsta360 X6\x1a\x04v1.2".to_vec();
    payload.extend_from_slice(extra);
    data.extend_from_slice(&payload);
    data.extend_from_slice(&[1, 1]);
    data.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    // Reserved padding separates blocks: the index, not reverse adjacency, is authoritative.
    data.extend_from_slice(&[0; 128]);
    let second_offset = data.len() - base;
    data.extend_from_slice(&[1, 2, 3, 4, 0, 2, 4, 0, 0, 0]);
    let mut index = vec![0; 30];
    index[10] = 1;
    index[11] = 1;
    index[12..16].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    index[20] = 2;
    index[22..26].copy_from_slice(&4u32.to_le_bytes());
    index[26..30].copy_from_slice(&(second_offset as u32).to_le_bytes());
    data.extend(index);
    data.extend_from_slice(&[0, 0]);
    data.extend_from_slice(&30u32.to_le_bytes());
    let extra = (data.len() - base + 72) as u32;
    data.extend_from_slice(&[0; 32]);
    data.extend_from_slice(&extra.to_le_bytes());
    data.extend_from_slice(&3u32.to_le_bytes());
    data.extend_from_slice(b"8db42d694ccc418790edff439fe026bf");
    data
}
#[test]
fn insta360_metadata_window_crop_is_bounded_and_keeps_sensor_and_active_sizes() {
    let crop = b"\xda\x01\x10\x08\xc0\x3e\x10\xf0\x2e\x18\x80\x2d\x20\x80\x2d\x28\x00\x30\x00";
    let m = inspect(&mut Cursor::new(fixture_with_extra(crop)))
        .unwrap()
        .unwrap();
    let crop = m.window_crop_info.unwrap();
    assert_eq!(crop.source_size, [8000, 6000]);
    assert_eq!(crop.destination_size, [5760, 5760]);
    assert_eq!(crop.offset, [0, 0]);
    // Missing dimensions, duplicate dimensions and an oversized sensor must reject.
    for bad in [
        b"\xda\x01\x02\x08\x01".as_slice(),
        b"\xda\x01\x04\x08\x01\x08\x02".as_slice(),
        b"\xda\x01\x04\x08\x80\x80\x08".as_slice(),
    ] {
        assert!(inspect(&mut Cursor::new(fixture_with_extra(bad))).is_err());
    }
}
#[test]
fn insta360_metadata_indexed_blocks_and_private_field_exclusion() {
    let data = fixture();
    let result = inspect(&mut Cursor::new(data)).unwrap().unwrap();
    assert_eq!(result.camera_model.as_deref(), Some("Insta360 X6"));
    assert_eq!(result.firmware.as_deref(), Some("v1.2"));
    assert_eq!(result.blocks.len(), 2);
    let json = serde_json::to_string(&result).unwrap();
    assert!(!json.contains("secret"));
    assert!(!json.contains("serial"));
}
#[test]
fn insta360_metadata_rejects_malformed_bounds_and_headers() {
    let data = fixture();
    for at in [
        data.len() - 40,
        data.len() - 36,
        32 + 27,
        data.len() - 72 - 6 - 30 + 16,
    ] {
        let mut bad = data.clone();
        bad[at] = 255;
        assert!(inspect(&mut Cursor::new(bad)).is_err(), "corruption {at}");
    }
    assert!(inspect(&mut Cursor::new(vec![0; 128])).unwrap().is_none());
    for end in 0..72 {
        assert!(inspect(&mut Cursor::new(data[..end].to_vec()))
            .unwrap()
            .is_none())
    }
}
#[test]
fn insta360_metadata_accepts_sparse_nonordinal_index_but_rejects_duplicate_kinds() {
    // Official X4 original has index kinds128,1,2,3,4,9,10,11,12,22.
    // This structural fixture uses synthetic payloads and kind128 in row0.
    let mut data = fixture();
    let index = data.len() - 72 - 6 - 30;
    let old_second = data[index + 20..index + 30].to_vec();
    data[index..index + 10].copy_from_slice(&old_second);
    data[index] = 128;
    data[index + 20..index + 30].fill(0);
    let base = 32;
    let second_offset = u32::from_le_bytes(old_second[6..10].try_into().unwrap()) as usize;
    data[base + second_offset + 5] = 128;
    let m = inspect(&mut Cursor::new(&data)).unwrap().unwrap();
    assert_eq!(
        m.blocks.iter().map(|b| b.kind).collect::<Vec<_>>(),
        [128, 1]
    );
    let duplicate = data[index..index + 10].to_vec();
    data[index + 20..index + 30].copy_from_slice(&duplicate);
    assert!(inspect(&mut Cursor::new(data))
        .unwrap_err()
        .to_string()
        .contains("duplicate"));
}
#[test]
#[ignore = "requires an explicitly supplied local media inventory"]
fn insta360_metadata_local_footage() {
    let path = std::env::var("VIDEO_KADR_INSTA360_INVENTORY").expect("supply local inventory JSON");
    let rows: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut count = 0;
    let mut without_trailer = 0;
    let mut models = std::collections::BTreeMap::new();
    for row in rows["files"].as_array().unwrap() {
        if row["bytes"].as_u64().unwrap() == 0 {
            continue;
        }
        let mut file = std::fs::File::open(row["path"].as_str().unwrap()).unwrap();
        let Some(m) = inspect(&mut file).unwrap() else {
            without_trailer += 1;
            continue;
        };
        *models.entry((m.camera_model, m.firmware)).or_insert(0) += 1;
        count += 1;
    }
    assert!(count > 0);
    println!("validated {count} indexed INSV trailers; {without_trailer} nonempty files without a recognized trailer; model/firmware counts: {models:?}");
}
