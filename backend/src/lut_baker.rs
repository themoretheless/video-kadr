//! Deterministic, point-local color-recipe baker.
//!
//! This deliberately accepts a much smaller contract than a video edit: a LUT
//! cannot represent geometry, time, audio, denoise, grain, or spatial effects.

use serde::Deserialize;

use crate::luts::{parse_cube, ParsedCube};

pub const BAKED_CUBE_SIZE: u32 = 33;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BakeRequest {
    pub edit: PointColorRecipe,
    pub size: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PointColorRecipe {
    #[serde(default)]
    pub brightness: f64,
    #[serde(default = "one")]
    pub contrast: f64,
    #[serde(default = "one")]
    pub saturation: f64,
    #[serde(default)]
    pub filter: Option<BakeLook>,
    #[serde(default)]
    pub curves: Option<BakeCurves>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BakeLook {
    Grayscale,
    Sepia,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BakeCurves {
    #[serde(default)]
    pub master: Option<Vec<BakePoint>>,
    #[serde(default)]
    pub red: Option<Vec<BakePoint>>,
    #[serde(default)]
    pub green: Option<Vec<BakePoint>>,
    #[serde(default)]
    pub blue: Option<Vec<BakePoint>>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BakePoint {
    pub x: f64,
    pub y: f64,
}

fn one() -> f64 {
    1.0
}

pub fn bake(request: &BakeRequest) -> Result<ParsedCube, &'static str> {
    if request.size != BAKED_CUBE_SIZE {
        return Err("поддерживается только LUT 33×33×33");
    }
    let recipe = &request.edit;
    if !recipe.brightness.is_finite()
        || !(-1.0..=1.0).contains(&recipe.brightness)
        || !recipe.contrast.is_finite()
        || !(0.0..=3.0).contains(&recipe.contrast)
        || !recipe.saturation.is_finite()
        || !(0.0..=3.0).contains(&recipe.saturation)
    {
        return Err("параметры цветокоррекции вне допустимого диапазона");
    }
    for curve in recipe
        .curves
        .iter()
        .flat_map(|curves| [&curves.master, &curves.red, &curves.green, &curves.blue])
        .flatten()
    {
        validate_curve(curve)?;
    }

    let size = request.size as usize;
    let denominator = (size - 1) as f64;
    let mut text = format!("LUT_3D_SIZE {size}\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1\n");
    for blue in 0..size {
        for green in 0..size {
            for red in 0..size {
                let mut rgb = [
                    red as f64 / denominator,
                    green as f64 / denominator,
                    blue as f64 / denominator,
                ];
                rgb = apply_eq(rgb, recipe.brightness, recipe.contrast, recipe.saturation);
                rgb = match recipe.filter {
                    Some(BakeLook::Grayscale) => grayscale(rgb),
                    Some(BakeLook::Sepia) => sepia(rgb),
                    None => rgb,
                };
                if let Some(curves) = &recipe.curves {
                    for value in &mut rgb {
                        *value = sample_curve(curves.master.as_deref(), *value);
                    }
                    rgb[0] = sample_curve(curves.red.as_deref(), rgb[0]);
                    rgb[1] = sample_curve(curves.green.as_deref(), rgb[1]);
                    rgb[2] = sample_curve(curves.blue.as_deref(), rgb[2]);
                }
                text.push_str(&format!(
                    "{} {} {}\n",
                    rgb[0].clamp(0.0, 1.0),
                    rgb[1].clamp(0.0, 1.0),
                    rgb[2].clamp(0.0, 1.0)
                ));
            }
        }
    }
    parse_cube(text.as_bytes()).map_err(|_| "не удалось канонизировать LUT")
}

fn validate_curve(points: &[BakePoint]) -> Result<(), &'static str> {
    if points.len() < 2 || points.len() > 64 {
        return Err("кривая должна содержать от 2 до 64 точек");
    }
    let mut previous = -1.0;
    for point in points {
        if !point.x.is_finite()
            || !point.y.is_finite()
            || !(0.0..=1.0).contains(&point.x)
            || !(0.0..=1.0).contains(&point.y)
            || point.x <= previous
        {
            return Err("некорректные точки кривой");
        }
        previous = point.x;
    }
    if points.first().unwrap().x != 0.0 || points.last().unwrap().x != 1.0 {
        return Err("кривая должна начинаться в 0 и заканчиваться в 1");
    }
    Ok(())
}

fn apply_eq(rgb: [f64; 3], brightness: f64, contrast: f64, saturation: f64) -> [f64; 3] {
    let adjusted = rgb.map(|value| ((value - 0.5) * contrast + 0.5 + brightness).clamp(0.0, 1.0));
    let luma = 0.2126 * adjusted[0] + 0.7152 * adjusted[1] + 0.0722 * adjusted[2];
    adjusted.map(|value| (luma + (value - luma) * saturation).clamp(0.0, 1.0))
}
fn grayscale(rgb: [f64; 3]) -> [f64; 3] {
    let y = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
    [y, y, y]
}
fn sepia(rgb: [f64; 3]) -> [f64; 3] {
    [
        0.393 * rgb[0] + 0.769 * rgb[1] + 0.189 * rgb[2],
        0.349 * rgb[0] + 0.686 * rgb[1] + 0.168 * rgb[2],
        0.272 * rgb[0] + 0.534 * rgb[1] + 0.131 * rgb[2],
    ]
    .map(|v| v.clamp(0.0, 1.0))
}
fn sample_curve(points: Option<&[BakePoint]>, value: f64) -> f64 {
    let Some(points) = points else {
        return value;
    };
    let upper = points
        .partition_point(|point| point.x < value)
        .min(points.len() - 1);
    if upper == 0 {
        return points[0].y;
    }
    let index = upper - 1;
    let (a, b) = (points[index], points[upper]);
    let width = b.x - a.x;
    let t = (value - a.x) / width;
    let slopes = pchip_slopes(points);
    let h00 = (2.0 * t - 3.0) * t * t + 1.0;
    let h10 = ((t - 2.0) * t + 1.0) * t;
    let h01 = (-2.0 * t + 3.0) * t * t;
    let h11 = (t - 1.0) * t * t;
    (h00 * a.y + h10 * width * slopes[index] + h01 * b.y + h11 * width * slopes[upper])
        .clamp(0.0, 1.0)
}

/// Shape-preserving cubic Hermite slopes (Fritsch-Carlson/PCHIP).
fn pchip_slopes(points: &[BakePoint]) -> Vec<f64> {
    if points.len() == 2 {
        let slope = (points[1].y - points[0].y) / (points[1].x - points[0].x);
        return vec![slope, slope];
    }
    let widths = points
        .windows(2)
        .map(|pair| pair[1].x - pair[0].x)
        .collect::<Vec<_>>();
    let secants = points
        .windows(2)
        .zip(&widths)
        .map(|(pair, width)| (pair[1].y - pair[0].y) / width)
        .collect::<Vec<_>>();
    let mut slopes = vec![0.0; points.len()];
    for index in 1..points.len() - 1 {
        let before = secants[index - 1];
        let after = secants[index];
        if before == 0.0 || after == 0.0 || before.signum() != after.signum() {
            slopes[index] = 0.0;
        } else {
            let first_weight = 2.0 * widths[index] + widths[index - 1];
            let second_weight = widths[index] + 2.0 * widths[index - 1];
            slopes[index] =
                (first_weight + second_weight) / (first_weight / before + second_weight / after);
        }
    }
    slopes[0] = endpoint_slope(widths[0], widths[1], secants[0], secants[1]);
    let last = widths.len() - 1;
    slopes[points.len() - 1] = endpoint_slope(
        widths[last],
        widths[last - 1],
        secants[last],
        secants[last - 1],
    );
    slopes
}

fn endpoint_slope(first_width: f64, second_width: f64, first: f64, second: f64) -> f64 {
    let candidate = ((2.0 * first_width + second_width) * first - first_width * second)
        / (first_width + second_width);
    if candidate.signum() != first.signum() {
        0.0
    } else if first.signum() != second.signum() && candidate.abs() > 3.0 * first.abs() {
        3.0 * first
    } else {
        candidate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn neutral_bake_is_exact_33_cube() {
        let cube = bake(&BakeRequest {
            size: 33,
            edit: PointColorRecipe {
                brightness: 0.0,
                contrast: 1.0,
                saturation: 1.0,
                filter: None,
                curves: None,
            },
        })
        .unwrap();
        assert_eq!(cube.cube_size, 33);
        assert_eq!(
            String::from_utf8_lossy(&cube.canonical).lines().count(),
            33usize.pow(3) + 3
        );
    }
    #[test]
    fn rejects_wrong_size_and_non_point_fields_at_json_boundary() {
        assert!(
            serde_json::from_str::<BakeRequest>(r#"{"size":17,"edit":{"crop":{"x":0}}}"#).is_err()
        );
        let request: BakeRequest = serde_json::from_str(r#"{"size":17,"edit":{}}"#).unwrap();
        assert!(bake(&request).is_err());
    }

    #[test]
    fn shared_browser_server_point_color_recipe_has_stable_sha() {
        let cube = bake(&BakeRequest {
            size: 33,
            edit: PointColorRecipe {
                brightness: 0.1,
                contrast: 1.2,
                saturation: 0.8,
                filter: None,
                curves: None,
            },
        })
        .unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&cube.canonical)),
            "62f55a9e0fac2cf9eca2ca11833c015e270ab601078978c1541554a6db5f9a21"
        );
        let cube = bake(&BakeRequest {
            size: 33,
            edit: PointColorRecipe {
                brightness: -0.05,
                contrast: 1.1,
                saturation: 0.9,
                filter: Some(BakeLook::Sepia),
                curves: Some(BakeCurves {
                    master: Some(vec![
                        BakePoint { x: 0.0, y: 0.0 },
                        BakePoint { x: 0.5, y: 0.65 },
                        BakePoint { x: 1.0, y: 1.0 },
                    ]),
                    red: Some(vec![
                        BakePoint { x: 0.0, y: 0.0 },
                        BakePoint { x: 1.0, y: 1.0 },
                    ]),
                    green: Some(vec![
                        BakePoint { x: 0.0, y: 0.0 },
                        BakePoint { x: 1.0, y: 1.0 },
                    ]),
                    blue: Some(vec![
                        BakePoint { x: 0.0, y: 0.0 },
                        BakePoint { x: 1.0, y: 1.0 },
                    ]),
                }),
            },
        })
        .unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&cube.canonical)),
            "7ee43a4a96d1699eee8598964055fa4fb6147fdcd6d1704c162d8adc7d7b424a"
        );
    }
}
