//! Pure editor domain. Modules in this tree own media semantics and must not
//! depend on HTTP extractors, async runtimes, subprocesses, or persistence.

pub mod arithmetic;
pub mod artifact_graph;
pub mod audio_output;
pub mod filmcraft_dsp;
pub mod filmcraft_video_project;
pub mod filmcraft_project_file;
pub mod filmcraft_project_index;
pub mod filmcraft_sequence_source;
pub mod filmcraft_master_volume;
pub mod filmcraft_stereo_strip;
pub mod filmcraft_surround_route;
pub mod filmcraft_stereo_graph;
pub mod filmcraft_clip_audio;
pub mod filmcraft_clip_effects;
pub mod filmcraft_project_audio;
pub mod filmcraft_generator_audio;
pub mod filmcraft_generator_video;
pub mod filmcraft_video_clock;
pub mod filmcraft_wav_audio;
pub mod filmcraft_pcm_export;
pub mod filmcraft_clip_placement;
pub mod filmcraft_scalar_curve;
pub mod filmcraft_effect_scalar;
pub mod filmcraft_audio_inserts;
pub mod filmcraft_component_curve;
pub mod video_effect_timing;
pub mod bezier;
#[cfg(feature = "spec-contracts")]
pub mod audio_pipeline;
#[cfg(feature = "spec-contracts")]
pub mod color_pipeline;
pub mod composition;
pub mod composition_graph;
pub mod nested_composition;
pub mod group_transform;
pub mod group_surface;
pub mod group_media;
pub mod edit;
pub mod elastic;
pub mod filter_graph;
#[cfg(feature = "spec-contracts")]
pub mod geometry;
pub mod keyframes;
pub mod media_contract;
#[cfg(feature = "spec-contracts")]
pub mod media_pipeline;
pub mod media_probe;
pub mod output;
pub mod project_collaboration;
#[cfg(feature = "spec-contracts")]
pub mod project_version;
#[cfg(feature = "spec-contracts")]
pub mod still_container;
#[cfg(feature = "spec-contracts")]
pub mod timeline;
pub mod tangents;

pub mod motion_path;
pub mod animated_color;

pub mod flat_surface;
pub mod surface_masks;
pub mod chroma_surface;

pub mod video_clock;

pub mod tone_curves;
pub mod creative_grade;

pub mod nested_audio;

pub mod audio_pan;
pub mod nested_pcm;

pub mod lut_tables;
pub mod color_lut;

pub mod hsl_secondary;

pub mod color_vignette;

pub mod basic_grade;

pub mod creative_adjust;

pub mod color_sharpen;

pub mod linear_color_frame;

pub mod lumetri;

pub mod color_match;

pub mod builtin_luts;

pub mod project_luts;

pub mod scopes;

pub mod nested_dsp;

pub mod filmcraft_audio_mapping;
pub mod filmcraft_audio_project;

pub mod audio_transition;
pub mod transition_gap;
pub mod transition_timeline;
pub mod transition_pixels;
pub mod transition_audio;

pub mod video_limiter;

pub mod cineon_converter;

pub mod gamma_correction;
pub mod posterize;
pub mod color_invert;
pub mod color_levels;
pub mod color_extract;
pub mod black_white;
pub mod color_tint;

pub mod proc_amp;

pub mod color_balance;

pub mod leave_color;

pub mod change_to_color;

pub mod color_pass;
pub mod color_replace;

pub mod channel_mix;

pub mod alpha_adjust;

pub mod luma_key;

pub mod color_key;

pub mod ultra_key;

pub mod gaussian_blur;

pub mod directional_blur;

pub mod camera_blur;

pub mod median;

pub mod mosaic;

pub mod find_edges;

pub mod emboss;

pub mod replicate;

pub mod strobe;

pub mod noise;

pub mod twirl;
pub mod wave_warp;
pub mod lens_distortion;
pub mod basic_3d;
pub mod drop_shadow;
pub mod bevel_alpha;

pub mod four_color_gradient;
pub mod ramp;
pub mod circle;
pub mod grid;
pub mod lens_flare;
pub mod checkerboard;

pub mod cell_pattern;

pub mod ellipse;

pub mod paint_bucket;

pub mod gradient;

pub mod color_emboss;

pub mod stroke;

pub mod roughen_edges;

pub mod brush_strokes;

pub mod long_shadow;

pub mod alpha_glow;

pub mod block_dissolve;

pub mod gradient_wipe;

pub mod linear_wipe;

pub mod magnify;

pub mod spherize;

pub mod corner_pin;

pub mod rotate_3d;

pub mod grow_shrink;

pub mod spin;

pub mod move_effect;

pub mod wiggle;

pub mod camera_shake;

pub mod spacer;

pub mod rounded_crop;

pub mod clone_effect;

pub mod auto_align;

pub mod turbulent_displace;

pub mod channel_blur;

pub mod reduce_interlace_flicker;

pub mod focus_blur;
pub mod bokeh_blur;
pub mod compound_blur;
pub mod sharpen;
pub mod unsharp_mask;
pub mod brightness_contrast;
pub mod crop;
pub mod edge_feather;
pub mod flip;
pub mod mirror;

pub mod offset;

pub mod asc_cdl;

pub mod transform_effect;

pub mod vignette_effect;

pub mod lighting_effects;

pub mod logo_cutout;

pub mod rgb_split;

pub mod edge_glow;

pub mod echo_glow;

pub mod wonder_glow;

pub mod glint;

pub mod volumetric_rays;

pub mod posterize_time;

pub mod echo;

pub mod auto_reframe;

pub mod warp_stabilizer;

pub mod motion_tracker;

pub mod warp_stabilizer_render;

pub mod warp_stabilizer_animation;

pub mod light_leaks;

pub mod vr_sphere;
pub mod spherical_video;
pub mod insta360_metadata;
pub mod insta360_camera_profile;

pub mod vr_filter;

pub mod vr_denoise;

pub mod vr_chromatic;

pub mod vr_color_gradients;

pub mod vr_digital_glitch;

pub mod vr_fractal_noise;

pub mod vr_glow;

pub mod vr_plane_to_sphere;

pub mod text_burnin;

pub mod simple_text;

pub mod metadata_burnin;

pub mod clock_label;

pub mod write_on;

pub mod write_on_animation;

#[cfg(test)]
mod write_on_contract;
pub mod lightning;
pub mod lightning_animation;

pub mod track_matte;
#[cfg(test)]
mod lightning_contract;

pub mod graphic_shape_path;

pub mod graphic_shape_render;

pub mod graphic_shape;

pub mod graphic_shape_animation;

pub mod graphic_shape_effect;

#[cfg(test)]
mod graphic_shape_animation_contract;

pub mod graphic_text_layout;

pub mod graphic_text_render;

#[cfg(test)]
mod graphic_shape_original_animation_tests;

#[cfg(test)]
mod graphic_text_original_tests;

pub mod graphic_text_runs;

pub mod graphic_text;

#[cfg(test)]
mod graphic_text_authored_tests;

pub mod graphic_text_animation;

pub mod graphic_source_text_animation;

pub mod graphic_text_effect;

#[cfg(test)]
mod graphic_text_original_animation_tests;

pub mod graphic_text_editing;

pub mod graphic_text_edit_command;

pub mod graphic_clip;

#[cfg(test)]
mod graphic_clip_original_tests;

pub mod graphic_timeline;

pub mod graphic_timeline_render;

#[cfg(test)]
mod graphic_working_effect_tests;

pub mod graphic_pin;

mod graphic_pin_layers;

#[cfg(test)]
mod graphic_pin_layers_original_tests;

pub mod graphic_pin_command;

pub mod graphic_responsive_time;

#[cfg(test)]
mod graphic_signed_time_tests;

pub mod graphic_roll;
mod graphic_roll_layers;
#[cfg(test)]
mod graphic_roll_layers_original_tests;

pub mod graphic_templates;
pub mod graphic_template_file;
pub mod graphic_template_library;
pub mod graphic_template_animation;
pub mod graphic_template_key_edit;
pub mod graphic_template_native_key_edit;
pub mod graphic_template_color;
pub mod graphic_template_fonts;
pub mod graphic_template_export;
pub mod graphic_template_thumbnail;

#[cfg(test)]
mod graphic_template_import_original_tests;

pub mod composition_fonts;

pub mod composition_font_replace;

pub mod project_fonts;

pub mod font_inventory_status;

pub mod graphic_template_control_defaults;

pub mod graphic_template_document_thumbnail;

pub mod graphic_template_control_authoring;

pub mod graphic_template_style_runs;

pub mod caption_graphic;

pub mod caption_timeline;

pub mod composition_captions;

pub mod caption_render;

#[cfg(test)]
pub(crate) mod caption_render_integration_tests;

pub mod caption_edit;

pub mod caption_file_timing;

pub mod caption_time;

pub mod caption_frame_clock;

pub mod caption_command_time;
pub mod transition_dissolves;

pub mod transition_wipe;
pub mod transition_export;

pub mod transition_kernel_pack;
pub mod transition_spec;

pub mod transition_procedural;

pub mod transition_preview_sources;

pub mod effect_frame_clock;

#[cfg(test)]
mod signed_generator_tests;

pub mod filmcraft_timing_bezier;
pub mod animation_sample_time;

#[cfg(test)]
mod graphic_color_import_tests;

#[cfg(test)]
mod graphic_point_import_tests;
#[cfg(test)]
mod graphic_key_auto_point_tests;

pub mod filmcraft_motion;

pub mod filmcraft_trim;

pub mod filmcraft_transition;

pub mod filmcraft_dynamic_trim;

pub mod filmcraft_trim_sessions;

pub mod filmcraft_video_plan;

pub mod filmcraft_video_appearance;

pub mod filmcraft_blend;

pub mod filmcraft_video_compositor;

pub mod filmcraft_video_jobs;

pub mod filmcraft_source_pixels;

pub mod filmcraft_source_frame;
pub mod filmcraft_generator_source_provider;
pub mod filmcraft_frame_blend;
pub mod filmcraft_source_captions;

pub(crate) mod filmcraft_source_effects;
pub(crate) mod filmcraft_source_masks;

pub mod filmcraft_source_color;

pub mod nested_clip_edit;

pub mod legacy_text_surface;

pub mod speech_alignment;

pub(crate) mod filmcraft_source_lumetri;

pub(crate) mod filmcraft_source_graphic;

pub mod stabilizer_analysis;

pub mod filmcraft_analysis_cache;

pub mod cutout;

pub mod video_cutout;

pub mod video_region;

pub mod concat_contrast;

pub mod concat_working;
pub mod concat_exposure;

pub mod concat_chroma;

pub mod concat_white_balance;

pub mod concat_color_wheels;

pub mod concat_curves;

pub mod concat_adjust;

pub mod concat_spatial_filters;

pub(crate) mod concat_hidden_rgb;

pub mod concat_contrast_pop;

pub mod concat_bleach_bypass;

pub mod concat_clean;

pub mod concat_green_looks;

pub mod concat_matte_looks;

pub mod concat_toned_looks;

pub mod concat_detail_looks;

pub mod concat_atmosphere_looks;

pub mod concat_weather_looks;

pub mod concat_portrait_looks;

pub mod concat_retro_looks;

pub mod concat_scenery_looks;

pub mod concat_light_looks;

pub mod concat_autumn;

pub mod concat_fixed_looks;

pub mod concat_scene_looks;

pub mod concat_color_presets;

pub mod concat_low_light_looks;

pub mod concat_neon_looks;

pub mod concat_super8;

pub mod concat_retro_cartoon;

pub mod concat_marquee;

pub mod concat_timed_looks;

pub mod concat_film_camera_looks;

pub mod concat_falling_dust;

pub mod concat_vhs;

pub mod concat_audio;

pub(crate) mod concat_audio_filter;

pub(crate) mod concat_limiter;
pub(crate) mod concat_distorted;

pub(crate) mod concat_tone;
pub(crate) mod concat_tone_filter;

pub(crate) mod concat_log_crusher;
pub(crate) mod concat_robot;
pub(crate) mod concat_deesser;
pub(crate) mod concat_compressor;
pub(crate) mod concat_denoise;
pub(crate) mod concat_resample;
pub(crate) mod concat_tempo;
pub(crate) mod concat_pitch;
pub(crate) mod concat_enhance_voice;

pub(crate) mod concat_pitch_timeline;

pub(crate) mod concat_format_plan;

pub(crate) mod concat_echo_kernel;

pub(crate) mod concat_biquad_stream;

pub(crate) mod concat_compand;

pub(crate) mod concat_ebur128;

pub(crate) mod concat_loudness_gain;

pub(crate) mod concat_loudness_limiter;

pub(crate) mod concat_loudness;

pub mod concat_chroma_key;

pub mod concat_luma_key;

pub mod concat_pixelate;

pub mod concat_posterize;

pub mod concat_gaussian_blur;

pub mod concat_effect_upgrade;

pub mod concat_animation;

pub mod concat_chromatic_aberration;

pub mod concat_fisheye;

pub mod concat_zoom_blur;

pub mod concat_motion_blur;

pub mod concat_halftone;

pub mod concat_film_grain;

pub mod concat_glow;

pub mod concat_bloom_pulse;

pub mod concat_transitions;

#[cfg(feature = "concat-qualification")]
pub mod concat_qualification;

pub mod concat_project_import;

pub mod concat_audio_import;
pub mod concat_clip_animation;
pub mod concat_picture;
pub mod concat_cutout_import;
pub mod concat_cutout_mask;
pub mod concat_cutout_clock;
pub mod concat_cutout_resolve;
pub mod concat_cutout_binding;
pub mod concat_video_pacer;

pub mod concat_video_binding;
pub mod concat_video_select;

mod held_video_clock;

pub mod concat_title;
pub mod concat_shape;
pub mod concat_treatment;

pub mod concat_project_fonts;

pub mod concat_effect_export;

pub mod concat_project_export;

pub mod concat_project_transitions;

pub mod concat_retime;
pub mod concat_retime_edit;

pub mod concat_freeze_splice;

pub mod concat_freeze_native;

pub mod concat_lane_groups;

pub mod native_video_freeze;

pub mod native_keyframe_clock;

pub mod filmcraft_parameter_edit;

pub mod filmcraft_image_sequence;
pub mod filmcraft_h264_pixels;
pub mod filmcraft_mask_track;

pub mod filmcraft_mask_monitor;

pub mod filmcraft_export_geometry;
pub mod filmcraft_export_text;
pub mod filmcraft_export_image;
pub mod filmcraft_export_clock;
pub mod filmcraft_export_h264;
pub mod filmcraft_export_movie;

pub mod filmcraft_export_metadata;

pub mod filmcraft_export_limiter;

pub mod filmcraft_yuv422_pixels;

pub mod insta360_gyro;

pub mod spherical_tracking;

pub mod insta360_video_clock;

pub mod spherical_optical_flow;

pub mod gyro_drift;

pub mod orientation_track;

pub mod visual_gyro_drift;

pub mod rolling_shutter;

pub mod insta360_time_map;
pub mod filmcraft_export_presets;
pub mod filmcraft_export_preset_resolve;
pub mod filmcraft_export_preset_overrides;
pub mod filmcraft_export_preset_settings;
pub mod filmcraft_export_caption_sidecar;
pub mod filmcraft_export_range;
pub mod filmcraft_quick_export_range;
pub mod filmcraft_export_queue;

pub mod filmcraft_export_pace;

pub mod filmcraft_export_command_range;

pub mod filmcraft_export_formats;

pub mod filmcraft_project_inspect;

pub mod filmcraft_sequence_inspect;

pub mod filmcraft_source_inspect;

pub mod filmcraft_effect_catalog;

pub mod filmcraft_effect_edit;

pub mod filmcraft_effect_apply;

pub mod filmcraft_effect_set_param;

pub mod filmcraft_parameter_sample;

pub mod filmcraft_mask_commands;

pub mod filmcraft_mask_track_batch;

pub mod filmcraft_clip_rename;
