//! Chatterbox training-text cleanup and reference RMS preparation.
//! The RMS target is an approximation, not an integrated LUFS measurement.
pub fn text(input: &str) -> String {
    let mut value = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some(first) = value.chars().next() {
        if first.is_lowercase() {
            value = first.to_uppercase().collect::<String>() + &value[first.len_utf8()..];
        }
    } else {
        return value;
    }
    for (from, to) in [
        ("…", ", "),
        (":", ","),
        ("—", "-"),
        ("–", "-"),
        (" ,", ","),
        ("“", "\""),
        ("”", "\""),
        ("‘", "'"),
        ("’", "'"),
    ] {
        value = value.replace(from, to);
    }
    value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if !value.ends_with(['.', '!', '?', '-', ',']) {
        value.push('.');
    }
    value
}

pub fn reference(samples: &[f32]) -> Vec<f32> {
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt();
    if rms <= 1e-6 {
        return samples.to_vec();
    }
    let gain = 10f32.powf((-27.0 - 20.0 * rms.log10()) / 20.0);
    if !gain.is_finite() || gain <= 0.0 {
        return samples.to_vec();
    }
    samples
        .iter()
        .map(|s| (s * gain).clamp(-1.0, 1.0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn narration_cleanup_preserves_tags_and_unicode() {
        for (input, expected) in [
            ("hello   world", "Hello world."),
            ("a list: one… two", "A list, one, two."),
            ("“quoted” — and ‘this’", "\"quoted\" - and 'this'."),
            ("ünïcode start", "Ünïcode start."),
            ("trailing ,", "Trailing,"),
            (" [laugh] hello [sigh] ", "[laugh] hello [sigh]."),
            ("   ", ""),
        ] {
            assert_eq!(text(input), expected);
            assert_eq!(text(expected), expected);
        }
    }
    #[test]
    fn reference_target_silence_and_peak_bound() {
        for amplitude in [0.001, 0.05, 0.9] {
            let output = reference(&vec![amplitude; 24000]);
            let rms = (output.iter().map(|s| s * s).sum::<f32>() / output.len() as f32).sqrt();
            assert!((20.0 * rms.log10() + 27.0).abs() < 0.01);
        }
        assert_eq!(reference(&[0.; 100]), vec![0.; 100]);
        assert!(reference(&[1., 0., 0.]).iter().all(|s| s.abs() <= 1.));
    }
}
