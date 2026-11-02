//! Native character formatting command boundary.
use super::{
    graphic_text_editing::{adjust_runs, apply_char_style, clear_char_style, normalize_runs},
    graphic_text_layout::{GraphicCharStyle, GraphicText},
    graphic_text_runs::{CharacterStyleRun, byte_runs},
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextEditOperation {
    Normalize,
    Apply {
        start: usize,
        end: usize,
        style: GraphicCharStyle,
    },
    Clear {
        start: usize,
        end: usize,
    },
    Adjust {
        #[serde(rename = "newText")]
        new_text: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextEditRequest {
    pub text: String,
    pub runs: Vec<CharacterStyleRun>,
    pub operation: TextEditOperation,
}
impl TextEditRequest {
    pub fn execute(&self) -> Result<Vec<CharacterStyleRun>> {
        let validate_style = |style: &GraphicCharStyle| {
            GraphicText {
                runs: vec![(0..0, style.clone())],
                ..Default::default()
            }
            .validate()
        };
        for r in &self.runs {
            validate_style(&r.style)?;
        }
        let n = self.text.chars().count();
        let (text, runs) = match &self.operation {
            TextEditOperation::Normalize => (&self.text, normalize_runs(&self.runs, n)),
            TextEditOperation::Apply { start, end, style } => {
                validate_style(style)?;
                (
                    &self.text,
                    apply_char_style(&self.runs, n, *start, *end, style),
                )
            }
            TextEditOperation::Clear { start, end } => {
                (&self.text, clear_char_style(&self.runs, n, *start, *end))
            }
            TextEditOperation::Adjust { new_text } => {
                (new_text, adjust_runs(&self.text, new_text, &self.runs))
            }
        };
        GraphicText {
            text: text.clone(),
            runs: byte_runs(text, &runs),
            ..Default::default()
        }
        .validate()?;
        Ok(runs)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_character_editing_commands() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-graphic-text-editing.json"
        ))
        .unwrap();
        let cases = f["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 192);
        for (i, c) in cases.iter().enumerate() {
            let start = c["start"].as_u64().unwrap() as usize;
            let end = c["end"].as_u64().unwrap() as usize;
            let operation = match c["operation"].as_str().unwrap() {
                "normalize" => TextEditOperation::Normalize,
                "apply" => TextEditOperation::Apply {
                    start,
                    end,
                    style: serde_json::from_value(c["style"].clone()).unwrap(),
                },
                "clear" => TextEditOperation::Clear { start, end },
                _ => TextEditOperation::Adjust {
                    new_text: c["newText"].as_str().unwrap().into(),
                },
            };
            let request = TextEditRequest {
                text: c["text"].as_str().unwrap().into(),
                runs: serde_json::from_value(c["runs"].clone()).unwrap(),
                operation,
            };
            let expected: Vec<CharacterStyleRun> =
                serde_json::from_value(c["expected"].clone()).unwrap();
            assert_eq!(request.execute().unwrap(), expected, "case{i}");
        }
    }
}
