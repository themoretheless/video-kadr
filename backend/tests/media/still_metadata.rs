use std::collections::BTreeSet;

use video_kadr_backend::domain::still_container::{
    ColorPrimaries, ImageOrientation, MetadataKind, StillBrand, StillCodec, StillContainer,
    StillContainerSpec, StillImageItem, StillImageMetadata, StillItemRole,
};

fn metadata(values: &[MetadataKind]) -> BTreeSet<MetadataKind> {
    values.iter().copied().collect()
}

#[test]
fn still_metadata_round_trip_is_explicit_about_loss_and_introduction() {
    let spec = StillContainerSpec {
        container: StillContainer::Avif,
        codec: StillCodec::Av1,
        major_brand: StillBrand::Avif,
        items: vec![StillImageItem {
            id: 1,
            role: StillItemRole::Primary,
            codec: StillCodec::Av1,
        }],
        metadata: StillImageMetadata {
            color_primaries: ColorPrimaries::DisplayP3,
            has_icc_profile: true,
            alpha: false,
            orientation: ImageOrientation::Rotate90,
            blocks: metadata(&[MetadataKind::Exif, MetadataKind::Icc]),
        },
    };
    spec.validate().unwrap();

    let mut decoded = spec.metadata.clone();
    decoded.blocks = metadata(&[MetadataKind::Icc, MetadataKind::Exif]);
    let exact = spec.metadata_round_trip(&decoded);
    assert!(exact.is_lossless());

    decoded.blocks = metadata(&[MetadataKind::Icc, MetadataKind::Xmp]);
    decoded.orientation = ImageOrientation::Identity;
    decoded.color_primaries = ColorPrimaries::Bt709;
    decoded.has_icc_profile = false;
    decoded.alpha = true;
    let changed = spec.metadata_round_trip(&decoded);
    assert_eq!(changed.lost, metadata(&[MetadataKind::Exif]));
    assert_eq!(changed.introduced, metadata(&[MetadataKind::Xmp]));
    assert!(!changed.color_preserved);
    assert!(!changed.icc_preserved);
    assert!(!changed.alpha_preserved);
    assert!(!changed.orientation_preserved);
}
