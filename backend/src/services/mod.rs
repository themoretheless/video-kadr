//! Application services coordinating domain plans and execution policies.

pub mod composition;
pub mod media_indexer;
#[cfg(feature = "spec-contracts")]
pub mod preview;
pub mod render;

pub mod filmcraft_video_preview;

pub mod local_speech;

pub(crate) mod stabilizer_path_store;

pub mod local_tts;

pub(crate) mod tts_job_identity;

pub mod tts_model_catalog;

pub mod tts_model_download;

pub mod local_voice_reference;

pub mod local_chatterbox;
pub mod local_kokoro;
pub mod local_enhance;
pub mod enhance_model;

pub mod local_enhance_sequence;

pub mod local_enhance_video;

pub mod local_cutout;

pub mod cutout_model;
pub mod sam_model;

pub mod local_mask_edit;

pub mod local_sam;

pub mod local_selfie;

pub mod local_person;
pub mod local_person_sequence;

pub(crate) mod local_person_sequence_contract;

pub mod local_person_video;

pub(crate) mod person_mov_color;

pub mod local_person_correct;

pub mod local_person_mask_video;

pub mod local_person_source_frame;

pub mod local_sam_track;

pub mod local_sam_video;

pub mod local_person_regions;

pub mod local_person_video_regions;

pub mod local_region_frame;
pub mod concat_cutout_asset;
pub mod concat_cutout_selection;
pub mod concat_cutout_still;
pub mod local_person_still;
pub mod concat_video_masks;
pub mod concat_video_regions;
pub mod concat_video_sample;

pub mod concat_video_analysis;

pub mod concat_video_selection;
pub mod concat_video_checkpoint;
pub mod concat_video_cancel;

pub mod concat_video_sam_seed;

pub mod sam_video_clock;

pub mod concat_freeze_capture;

pub mod concat_freeze_publish;

pub mod local_sam_reidentify;

pub mod insta360_visual_drift;
