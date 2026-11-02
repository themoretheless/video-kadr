#[path = "../src/services/person_mov_color.rs"]
mod person_mov_color;

fn atom(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut v = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
    v.extend_from_slice(tag);
    v.extend_from_slice(payload);
    v
}
fn movie(colors: usize) -> Vec<u8> {
    let mut sample = vec![0; 78];
    for _ in 0..colors {
        sample.extend(atom(b"colr", b"nclc\0\x02\0\x02\0\0"));
    }
    let mut stsd = vec![0; 8];
    stsd[7] = 1;
    stsd.extend(atom(b"rle ", &sample));
    let mut tree = atom(b"stsd", &stsd);
    for tag in [b"stbl", b"minf", b"mdia", b"trak", b"moov"] {
        tree = atom(tag, &tree);
    }
    let mut result = atom(b"mdat", b"media bytes must stay at the same offset");
    result.extend(tree);
    result
}
#[test]
fn inserts_replaces_without_moving_media_and_is_idempotent() {
    for colors in [0, 1] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("movie.mov");
        let original = movie(colors);
        std::fs::write(&path, &original).unwrap();
        person_mov_color::preserve(&path, Some("bt709"), Some("bt709")).unwrap();
        let output = std::fs::read(&path).unwrap();
        let media_end = u32::from_be_bytes(original[..4].try_into().unwrap()) as usize;
        assert_eq!(&output[..media_end], &original[..media_end]);
        assert_eq!(
            output.len(),
            original.len() + if colors == 0 { 18 } else { 0 }
        );
        assert!(output.windows(10).any(|w| w == b"nclc\0\x01\0\x01\0\0"));
        person_mov_color::preserve(&path, Some("bt709"), Some("bt709")).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), output);
    }
}
#[test]
fn refuses_malformed_duplicate_and_nonfinal_metadata_without_writing() {
    let mut malformed = movie(0);
    malformed[0..4].copy_from_slice(&u32::MAX.to_be_bytes());
    let mut nonfinal = movie(0);
    nonfinal.extend(atom(b"free", b""));
    for original in [malformed, nonfinal, movie(2), atom(b"moov", b"bad")] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.mov");
        std::fs::write(&path, &original).unwrap();
        assert!(person_mov_color::preserve(&path, Some("bt709"), Some("bt709")).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}
