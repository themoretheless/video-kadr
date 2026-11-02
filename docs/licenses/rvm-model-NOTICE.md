# Robust Video Matting model

RVM MobileNetV3 FP32 model by PeterL1n / RobustVideoMatting, release v1.0.0:
https://github.com/PeterL1n/RobustVideoMatting/releases/tag/v1.0.0

Artifact: rvm_mobilenetv3_fp32.onnx,14975696bytes,
SHA-256 `88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828`.
Concat's pinned models.rs identifies this model as GPL-3.0:
https://github.com/jub0t/concat/blob/78e4be101c2d3c41a7dc73c112c6b1f11b9ce170/src/crates/concat-vision/src/models.rs

Qualification weights remain generated local assets. No weights are embedded or installed into accounts by the native-core change.

Test-only reference_person.rs retains the original Concat AGPL-3.0-or-later SPDX and copyright attribution. It contains the pinned segment.rs person_mask method and preprocessing/probability helpers, with imports/visibility adapted and a test-only ORT adapter/inspection methods. Production does not call this oracle.
