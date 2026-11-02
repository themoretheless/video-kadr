fn atom(kind: &[u8; 4], payload: Vec<u8>) -> Vec<u8> {
    let mut b = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
    b.extend(kind);
    b.extend(payload);
    b
}
pub fn movie(
    scale: u32,
    runs: &[(u32, u32)],
    composition: Option<&[(u32, i32)]>,
    edit: Option<i32>,
) -> Vec<u8> {
    let mut mdhd = vec![0; 12];
    mdhd.extend(scale.to_be_bytes());
    mdhd.extend([0; 8]);
    let mut handler = vec![0; 8];
    handler.extend(b"vide");
    handler.extend([0; 12]);
    let mut stts = vec![0; 4];
    stts.extend((runs.len() as u32).to_be_bytes());
    for (n, delta) in runs {
        stts.extend(n.to_be_bytes());
        stts.extend(delta.to_be_bytes());
    }
    let mut stbl = atom(b"stts", stts);
    if let Some(entries) = composition {
        let mut ctts = vec![0; 4];
        ctts.extend((entries.len() as u32).to_be_bytes());
        for (n, offset) in entries {
            ctts.extend(n.to_be_bytes());
            ctts.extend(offset.to_be_bytes());
        }
        stbl.extend(atom(b"ctts", ctts));
    }
    let mut media = atom(b"mdhd", mdhd);
    media.extend(atom(b"hdlr", handler));
    media.extend(atom(b"minf", atom(b"stbl", stbl)));
    let mut track = atom(b"mdia", media);
    if let Some(media_time) = edit {
        let mut elst = vec![0; 4];
        elst.extend(1u32.to_be_bytes());
        elst.extend(1000u32.to_be_bytes());
        elst.extend(media_time.to_be_bytes());
        elst.extend([0, 1, 0, 0]);
        track.extend(atom(b"edts", atom(b"elst", elst)));
    }
    let mut out = atom(b"ftyp", b"isom\0\0\0\0".to_vec());
    out.extend(atom(b"moov", atom(b"trak", track)));
    out
}

#[allow(dead_code)]
pub fn add_audio_edit(movie: &mut Vec<u8>, media_time: i32) {
    let mut handler = vec![0; 8];
    handler.extend(b"soun");
    handler.extend([0; 12]);
    let mut elst = vec![0; 4];
    elst.extend(1u32.to_be_bytes());
    elst.extend(1000u32.to_be_bytes());
    elst.extend(media_time.to_be_bytes());
    elst.extend([0, 1, 0, 0]);
    let mut track = atom(b"mdia", atom(b"hdlr", handler));
    track.extend(atom(b"edts", atom(b"elst", elst)));
    let track = atom(b"trak", track);
    let old = u32::from_be_bytes(movie[16..20].try_into().unwrap());
    movie[16..20].copy_from_slice(&(old + track.len() as u32).to_be_bytes());
    movie.extend(track);
}
#[allow(dead_code)]
pub fn add_video_track(movie: &mut Vec<u8>, other: Vec<u8>) {
    let track = &other[24..];
    let old = u32::from_be_bytes(movie[16..20].try_into().unwrap());
    movie[16..20].copy_from_slice(&(old + track.len() as u32).to_be_bytes());
    movie.extend(track);
}
