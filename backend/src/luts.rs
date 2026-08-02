//! Strict parser and public metadata for immutable 3D `.cube` LUT assets.
//!
//! Uploaded input is never published verbatim. The parser accepts the small,
//! well-defined subset consumed by the renderer and emits a canonical file.

use std::fmt;

use serde::Serialize;

pub const LUT_SCHEMA_VERSION: u32 = 1;
pub const MAX_LUT_FILE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CUBE_SIZE: usize = 65;
const MAX_LINE_BYTES: usize = 4096;
const MAX_TITLE_CHARS: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LutAsset {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub kind: String,
    pub cube_size: u32,
    pub size_bytes: u64,
    pub sha256: String,
    pub created_at: u64,
    /// Server-generated basename used only inside the private LUT directory.
    #[serde(skip_serializing)]
    pub filename: String,
}

impl LutAsset {
    pub fn new(
        id: String,
        name: String,
        filename: String,
        cube_size: u32,
        size_bytes: u64,
        sha256: String,
        created_at: u64,
    ) -> Self {
        Self {
            schema_version: LUT_SCHEMA_VERSION,
            id,
            name,
            kind: "cube3d".into(),
            cube_size,
            size_bytes,
            sha256,
            created_at,
            filename,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedCube {
    pub cube_size: u32,
    pub canonical: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CubeError {
    TooLarge,
    InvalidUtf8,
    LineTooLong,
    UnsupportedOneDimensional,
    UnknownHeader,
    DuplicateHeader,
    HeaderAfterData,
    InvalidTitle,
    InvalidSize,
    MissingSize,
    InvalidDomain,
    InvalidTriplet,
    NonFiniteNumber,
    WrongEntryCount,
}

impl fmt::Display for CubeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid 3D cube LUT: {self:?}")
    }
}

impl std::error::Error for CubeError {}

/// Parse a strict, bounded 3D CUBE file and return canonical UTF-8 bytes.
pub fn parse_cube(bytes: &[u8]) -> Result<ParsedCube, CubeError> {
    if bytes.len() > MAX_LUT_FILE_BYTES {
        return Err(CubeError::TooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| CubeError::InvalidUtf8)?;
    if text.contains('\0') {
        return Err(CubeError::InvalidUtf8);
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);

    let mut size = None;
    let mut title_seen = false;
    let mut domain_min: Option<[f64; 3]> = None;
    let mut domain_max: Option<[f64; 3]> = None;
    let mut values = Vec::new();
    let mut expected_entries = None;
    let mut data_started = false;

    for raw_line in text.lines() {
        if raw_line.len() > MAX_LINE_BYTES {
            return Err(CubeError::LineTooLong);
        }
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let keyword = line.split_whitespace().next().unwrap_or_default();
        match keyword {
            "TITLE" => {
                reject_late_header(data_started)?;
                if title_seen {
                    return Err(CubeError::DuplicateHeader);
                }
                validate_title(&line["TITLE".len()..])?;
                title_seen = true;
            }
            "LUT_1D_SIZE" => return Err(CubeError::UnsupportedOneDimensional),
            "LUT_3D_SIZE" => {
                reject_late_header(data_started)?;
                if size.is_some() {
                    return Err(CubeError::DuplicateHeader);
                }
                let value = parse_single_usize(&line["LUT_3D_SIZE".len()..])?;
                if !(2..=MAX_CUBE_SIZE).contains(&value) {
                    return Err(CubeError::InvalidSize);
                }
                size = Some(value);
                let expected = value
                    .checked_mul(value)
                    .and_then(|entries| entries.checked_mul(value))
                    .ok_or(CubeError::InvalidSize)?;
                values.reserve_exact(expected);
                expected_entries = Some(expected);
            }
            "DOMAIN_MIN" => {
                reject_late_header(data_started)?;
                if domain_min.is_some() {
                    return Err(CubeError::DuplicateHeader);
                }
                domain_min = Some(parse_triplet(&line["DOMAIN_MIN".len()..])?);
            }
            "DOMAIN_MAX" => {
                reject_late_header(data_started)?;
                if domain_max.is_some() {
                    return Err(CubeError::DuplicateHeader);
                }
                domain_max = Some(parse_triplet(&line["DOMAIN_MAX".len()..])?);
            }
            _ if keyword.parse::<f64>().is_err()
                && keyword
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_') =>
            {
                return Err(CubeError::UnknownHeader);
            }
            _ => {
                if size.is_none() {
                    return Err(CubeError::MissingSize);
                }
                data_started = true;
                if values.len() >= expected_entries.expect("size establishes entry count") {
                    return Err(CubeError::WrongEntryCount);
                }
                values.push(parse_triplet(line)?);
            }
        }
    }

    let size = size.ok_or(CubeError::MissingSize)?;
    let expected = size
        .checked_mul(size)
        .and_then(|value| value.checked_mul(size))
        .ok_or(CubeError::InvalidSize)?;
    if values.len() != expected {
        return Err(CubeError::WrongEntryCount);
    }
    let domain_min = domain_min.unwrap_or([0.0; 3]);
    let domain_max = domain_max.unwrap_or([1.0; 3]);
    if !(0..3).all(|index| domain_min[index] < domain_max[index]) {
        return Err(CubeError::InvalidDomain);
    }

    let mut canonical = String::new();
    canonical.push_str(&format!("LUT_3D_SIZE {size}\n"));
    canonical.push_str(&format!("DOMAIN_MIN {}\n", format_triplet(domain_min)));
    canonical.push_str(&format!("DOMAIN_MAX {}\n", format_triplet(domain_max)));
    for value in values {
        canonical.push_str(&format_triplet(value));
        canonical.push('\n');
    }
    finish_cube(size, canonical)
}

fn finish_cube(cube_size: usize, canonical: String) -> Result<ParsedCube, CubeError> {
    if canonical.len() > MAX_LUT_FILE_BYTES {
        return Err(CubeError::TooLarge);
    }

    Ok(ParsedCube {
        cube_size: cube_size as u32,
        canonical: canonical.into_bytes(),
    })
}

fn reject_late_header(data_started: bool) -> Result<(), CubeError> {
    if data_started {
        Err(CubeError::HeaderAfterData)
    } else {
        Ok(())
    }
}

fn validate_title(rest: &str) -> Result<(), CubeError> {
    let value = rest.trim();
    if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
        return Err(CubeError::InvalidTitle);
    }
    let title = &value[1..value.len() - 1];
    if title.chars().count() > MAX_TITLE_CHARS
        || title.chars().any(char::is_control)
        || title.contains('"')
    {
        return Err(CubeError::InvalidTitle);
    }
    Ok(())
}

fn parse_single_usize(rest: &str) -> Result<usize, CubeError> {
    let mut tokens = rest.split_whitespace();
    let value = tokens
        .next()
        .ok_or(CubeError::InvalidSize)?
        .parse()
        .map_err(|_| CubeError::InvalidSize)?;
    if tokens.next().is_some() {
        return Err(CubeError::InvalidSize);
    }
    Ok(value)
}

fn parse_triplet(value: &str) -> Result<[f64; 3], CubeError> {
    let mut tokens = value.split_whitespace();
    let mut parsed = [0.0; 3];
    for parsed_value in &mut parsed {
        let token = tokens.next().ok_or(CubeError::InvalidTriplet)?;
        let number = token
            .parse::<f64>()
            .map_err(|_| CubeError::InvalidTriplet)?;
        if !number.is_finite() {
            return Err(CubeError::NonFiniteNumber);
        }
        *parsed_value = number;
    }
    if tokens.next().is_some() {
        return Err(CubeError::InvalidTriplet);
    }
    Ok(parsed)
}

fn format_triplet(value: [f64; 3]) -> String {
    value.map(format_number).join(" ")
}

fn format_number(value: f64) -> String {
    if value == 0.0 {
        "0".into()
    } else {
        value.to_string()
    }
}

/// Derive a bounded, harmless display label without trusting it as a path.
pub fn display_name(original: Option<&str>) -> String {
    let basename = original
        .unwrap_or_default()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default();
    let stem = basename
        .strip_suffix(".cube")
        .or_else(|| basename.strip_suffix(".CUBE"))
        .unwrap_or(basename);
    let cleaned: String = stem
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_TITLE_CHARS)
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "LUT".into()
    } else {
        cleaned.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_cube() -> Vec<u8> {
        b"# comment\nTITLE \"Identity\"\nLUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n".to_vec()
    }

    #[test]
    fn valid_cube_is_canonical_and_roundtrips() {
        let parsed = parse_cube(&identity_cube()).unwrap();
        assert_eq!(parsed.cube_size, 2);
        assert!(!String::from_utf8_lossy(&parsed.canonical).contains("TITLE"));
        assert_eq!(parse_cube(&parsed.canonical).unwrap(), parsed);
    }

    #[test]
    fn rejects_unsupported_or_malformed_cubes() {
        assert_eq!(
            parse_cube(b"LUT_1D_SIZE 2\n0 0 0\n"),
            Err(CubeError::UnsupportedOneDimensional)
        );
        assert_eq!(
            parse_cube(b"LUT_3D_SIZE 1\n0 0 0\n"),
            Err(CubeError::InvalidSize)
        );
        assert_eq!(
            parse_cube(b"LUT_3D_SIZE 2\n0 0 0\n"),
            Err(CubeError::WrongEntryCount)
        );
        assert_eq!(
            parse_cube(b"LUT_3D_SIZE 2\nNaN 0 0\n"),
            Err(CubeError::NonFiniteNumber)
        );
        assert_eq!(
            parse_cube(b"LUT_3D_SIZE 2\nVENDOR_EXTENSION 1\n"),
            Err(CubeError::UnknownHeader)
        );
        assert_eq!(parse_cube(&[0xff, 0xfe]), Err(CubeError::InvalidUtf8));
    }

    #[test]
    fn client_name_is_display_only_and_bounded() {
        assert_eq!(display_name(Some("../../looks/My Look.cube")), "My Look");
        assert_eq!(display_name(Some("..\\private\\Film.CUBE")), "Film");
        assert_eq!(display_name(Some("\n.cube")), "LUT");
    }

    #[test]
    fn parser_has_an_independent_byte_limit() {
        let oversized = vec![b' '; MAX_LUT_FILE_BYTES + 1];
        assert_eq!(parse_cube(&oversized), Err(CubeError::TooLarge));
    }

    #[test]
    fn parser_rejects_an_entry_beyond_the_declared_cube_before_parsing_it() {
        let mut overfilled = identity_cube();
        overfilled.extend_from_slice(b"0 0\n");
        assert_eq!(parse_cube(&overfilled), Err(CubeError::WrongEntryCount));
    }

    #[test]
    fn canonical_output_keeps_the_same_independent_byte_limit() {
        let oversized = "x".repeat(MAX_LUT_FILE_BYTES + 1);
        assert!(matches!(
            finish_cube(2, oversized),
            Err(CubeError::TooLarge)
        ));
    }
}
