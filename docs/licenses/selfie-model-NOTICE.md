# MediaPipe Selfie Segmentation model

The qualification model is Google's general 256x256 MediaPipe Selfie Segmentation model converted to ONNX by ONNX Community. Concat's pinned model notice attributes both the model and conversion to Apache-2.0:
https://github.com/jub0t/concat/blob/78e4be101c2d3c41a7dc73c112c6b1f11b9ce170/src/crates/concat-vision/models/NOTICE.md

Upstream conversion: https://huggingface.co/onnx-community/mediapipe_selfie_segmentation

Pinned qualification artifact from Concat commit78e4be101c2d3c41a7dc73c112c6b1f11b9ce170:
`src/crates/concat-vision/models/selfie-segmentation.onnx`,462352bytes,
SHA-256 `3241ac4ad8aa35bdaf33946776db29f7c283a413aa0b0dacb9483594b4531aad`.

The test-only reference_selfie.rs retains the original AGPL-3.0-or-later SPDX/copyright notice from Concat segment.rs. It contains original preprocessing/postprocessing helpers with only imports and visibility adapted. Production uses a separate native runtime implementation. Model binaries remain generated qualification assets and are not embedded by this change.
