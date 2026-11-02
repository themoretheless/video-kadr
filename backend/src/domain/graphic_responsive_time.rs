//! Filmcraft protected intro/outro timing. Attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ResponsiveTime {
    pub intro: i64,
    pub outro: i64,
    pub design_in: i64,
    pub design_duration: i64,
}
impl ResponsiveTime {
    pub fn active(&self) -> bool {
        (self.intro > 0 || self.outro > 0) && self.design_duration > 0
    }
    /// Signed authored ticks; all calculations use widened integers before checked admission.
    pub fn remap(&self, u: i64, d: i64) -> Result<i64> {
        let (u, d) = (u as i128, d as i128);
        let local = if !self.active() {
            u
        } else {
            let (intro, outro, dd) = (
                self.intro.max(0) as i128,
                self.outro.max(0) as i128,
                self.design_duration as i128,
            );
            if u < intro {
                u
            } else if d - u <= outro {
                dd - (d - u)
            } else {
                let (a, b) = (intro, d - outro);
                let (aa, bb) = (intro, dd - outro);
                if b <= a {
                    aa
                } else {
                    (u - a)
                        .checked_mul(bb - aa)
                        .and_then(|value| aa.checked_add(value / (b - a)))
                        .ok_or_else(|| anyhow::anyhow!("responsive graphic clock overflow"))?
                }
            }
        };
        let mapped = self.design_in as i128 + local;
        ensure!(
            mapped >= i64::MIN as i128 && mapped <= i64::MAX as i128,
            "responsive graphic clock overflow"
        );
        Ok(mapped as i64)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_protected_time_all_boundaries() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-graphic-responsive-time.json"
        ))
        .unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 12672);
        for (index, case) in cases.iter().enumerate() {
            let meta: ResponsiveTime = serde_json::from_value(case["meta"].clone()).unwrap();
            assert_eq!(
                meta.remap(case["u"].as_i64().unwrap(), case["d"].as_i64().unwrap())
                    .unwrap(),
                case["expected"].as_i64().unwrap(),
                "case{index}"
            );
        }
    }
    #[test]
    fn overflow_is_a_checked_error() {
        assert!(
            ResponsiveTime {
                design_in: i64::MAX,
                ..Default::default()
            }
            .remap(1, 10)
            .is_err()
        );
    }
}
