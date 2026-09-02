//! Advisory full-reference quality reports for opt-in export analysis.
//!
//! Quality scores never gate an export: they explain likely degradation and
//! provide scene-level evidence that callers may display after rendering.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityModel {
    VmafV0_6_1,
    Vmaf4kV0_6_1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewingCondition {
    Desktop,
    Television,
    Mobile,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneQuality {
    pub start_ms: u64,
    pub end_ms: u64,
    pub vmaf_mean: f64,
    pub vmaf_p01: f64,
    pub psnr_mean_db: f64,
    pub ssim_mean: f64,
}

impl SceneQuality {
    pub fn validate(&self) -> Result<(), QualityReportError> {
        if self.end_ms <= self.start_ms {
            return Err(QualityReportError::InvalidSceneRange);
        }
        validate_score(self.vmaf_mean)?;
        validate_score(self.vmaf_p01)?;
        if !self.psnr_mean_db.is_finite() || self.psnr_mean_db < 0.0 {
            return Err(QualityReportError::InvalidPsnr);
        }
        if !self.ssim_mean.is_finite() || !(0.0..=1.0).contains(&self.ssim_mean) {
            return Err(QualityReportError::InvalidSsim);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QualityReport {
    pub model: QualityModel,
    pub viewing_condition: ViewingCondition,
    pub vmaf_mean: f64,
    pub vmaf_p01: f64,
    pub psnr_mean_db: f64,
    pub ssim_mean: f64,
    pub scenes: Vec<SceneQuality>,
}

impl QualityReport {
    pub fn new(
        model: QualityModel,
        viewing_condition: ViewingCondition,
        vmaf_mean: f64,
        vmaf_p01: f64,
        psnr_mean_db: f64,
        ssim_mean: f64,
        scenes: Vec<SceneQuality>,
    ) -> Result<Self, QualityReportError> {
        let report = Self {
            model,
            viewing_condition,
            vmaf_mean,
            vmaf_p01,
            psnr_mean_db,
            ssim_mean,
            scenes,
        };
        report.validate()?;
        Ok(report)
    }

    pub fn validate(&self) -> Result<(), QualityReportError> {
        validate_score(self.vmaf_mean)?;
        validate_score(self.vmaf_p01)?;
        if !self.psnr_mean_db.is_finite() || self.psnr_mean_db < 0.0 {
            return Err(QualityReportError::InvalidPsnr);
        }
        if !self.ssim_mean.is_finite() || !(0.0..=1.0).contains(&self.ssim_mean) {
            return Err(QualityReportError::InvalidSsim);
        }
        for (index, scene) in self.scenes.iter().enumerate() {
            scene.validate()?;
            if index > 0 && self.scenes[index - 1].end_ms > scene.start_ms {
                return Err(QualityReportError::OverlappingScenes);
            }
        }
        Ok(())
    }

    /// Human-readable, deliberately advisory assessment for the export UI.
    pub fn advisory(&self) -> QualityAdvisory {
        let floor = self.vmaf_p01.min(self.vmaf_mean);
        if floor >= 90.0 {
            QualityAdvisory::Excellent
        } else if floor >= 80.0 {
            QualityAdvisory::Good
        } else if floor >= 70.0 {
            QualityAdvisory::VisibleLoss
        } else {
            QualityAdvisory::SevereLoss
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityAdvisory {
    Excellent,
    Good,
    VisibleLoss,
    SevereLoss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityReportError {
    InvalidScore,
    InvalidPsnr,
    InvalidSsim,
    InvalidSceneRange,
    OverlappingScenes,
}

fn validate_score(value: f64) -> Result<(), QualityReportError> {
    if value.is_finite() && (0.0..=100.0).contains(&value) {
        Ok(())
    } else {
        Err(QualityReportError::InvalidScore)
    }
}

impl fmt::Display for QualityReportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid advisory quality report: {self:?}")
    }
}

impl std::error::Error for QualityReportError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_percentile_drives_advisory_without_gating_export() {
        let report = QualityReport::new(
            QualityModel::VmafV0_6_1,
            ViewingCondition::Desktop,
            94.2,
            78.4,
            42.1,
            0.97,
            vec![SceneQuality {
                start_ms: 0,
                end_ms: 1_000,
                vmaf_mean: 94.2,
                vmaf_p01: 78.4,
                psnr_mean_db: 42.1,
                ssim_mean: 0.97,
            }],
        )
        .unwrap();
        assert_eq!(report.advisory(), QualityAdvisory::VisibleLoss);
    }

    #[test]
    fn invalid_scores_and_overlapping_scenes_are_rejected() {
        assert_eq!(
            validate_score(f64::NAN),
            Err(QualityReportError::InvalidScore)
        );
        let error = QualityReport::new(
            QualityModel::VmafV0_6_1,
            ViewingCondition::Television,
            90.0,
            80.0,
            40.0,
            0.95,
            vec![
                SceneQuality {
                    start_ms: 0,
                    end_ms: 100,
                    vmaf_mean: 90.0,
                    vmaf_p01: 80.0,
                    psnr_mean_db: 40.0,
                    ssim_mean: 0.95,
                },
                SceneQuality {
                    start_ms: 99,
                    end_ms: 200,
                    vmaf_mean: 91.0,
                    vmaf_p01: 81.0,
                    psnr_mean_db: 41.0,
                    ssim_mean: 0.96,
                },
            ],
        )
        .unwrap_err();
        assert_eq!(error, QualityReportError::OverlappingScenes);
    }
}
