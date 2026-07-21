//! FFmpeg look preset catalog shared by command compilation and capabilities.

use crate::domain::edit::LookPreset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LookPresetDefinition {
    pub(crate) preset: LookPreset,
    pub(crate) label: &'static str,
    pub(crate) ffmpeg_filter_chain: &'static str,
    pub(crate) required_filters: &'static [&'static str],
}

impl LookPresetDefinition {
    pub(crate) const fn id(&self) -> &'static str {
        self.preset.wire_id()
    }
}

const LOOK_PRESET_CATALOG: [LookPresetDefinition; LookPreset::ALL.len()] = [
    LookPresetDefinition {
        preset: LookPreset::Grayscale,
        label: "Ч/Б",
        ffmpeg_filter_chain: "hue=s=0",
        required_filters: &["hue"],
    },
    LookPresetDefinition {
        preset: LookPreset::Sepia,
        label: "Сепия",
        ffmpeg_filter_chain: "colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131",
        required_filters: &["colorchannelmixer"],
    },
    LookPresetDefinition {
        preset: LookPreset::Warm,
        label: "Тёплый",
        ffmpeg_filter_chain: "colorbalance=rs=0.2:gs=0.05:bs=-0.2",
        required_filters: &["colorbalance"],
    },
    LookPresetDefinition {
        preset: LookPreset::Cold,
        label: "Холодный",
        ffmpeg_filter_chain: "colorbalance=rs=-0.2:gs=0:bs=0.2",
        required_filters: &["colorbalance"],
    },
    LookPresetDefinition {
        preset: LookPreset::TealOrange,
        label: "Teal-Orange",
        ffmpeg_filter_chain: "colorbalance=rs=-0.15:bs=0.15:rm=0.1:bm=-0.05:rh=0.15:bh=-0.15",
        required_filters: &["colorbalance"],
    },
    LookPresetDefinition {
        preset: LookPreset::Faded,
        label: "Выцветший",
        ffmpeg_filter_chain: "curves=all='0/0.08 1/0.92'",
        required_filters: &["curves"],
    },
    LookPresetDefinition {
        preset: LookPreset::Noir,
        label: "Нуар",
        ffmpeg_filter_chain: "hue=s=0,eq=contrast=1.4",
        required_filters: &["hue", "eq"],
    },
    LookPresetDefinition {
        preset: LookPreset::Vintage,
        label: "Винтаж",
        ffmpeg_filter_chain: "curves=all='0/0.06 1/0.95',colorbalance=rs=0.15:gs=0.05:bs=-0.1",
        required_filters: &["curves", "colorbalance"],
    },
];

pub(crate) fn look_preset_catalog() -> &'static [LookPresetDefinition] {
    &LOOK_PRESET_CATALOG
}

pub(crate) fn look_preset_definition(preset: LookPreset) -> &'static LookPresetDefinition {
    look_preset_catalog()
        .iter()
        .find(|definition| definition.preset == preset)
        .expect("every look preset has an FFmpeg definition")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_every_preset_and_declares_every_chain_filter() {
        let expected = [
            (LookPreset::Grayscale, "Ч/Б"),
            (LookPreset::Sepia, "Сепия"),
            (LookPreset::Warm, "Тёплый"),
            (LookPreset::Cold, "Холодный"),
            (LookPreset::TealOrange, "Teal-Orange"),
            (LookPreset::Faded, "Выцветший"),
            (LookPreset::Noir, "Нуар"),
            (LookPreset::Vintage, "Винтаж"),
        ];

        assert_eq!(look_preset_catalog().len(), LookPreset::ALL.len());
        for (definition, (preset, label)) in look_preset_catalog().iter().zip(expected) {
            assert_eq!(definition.preset, preset);
            assert_eq!(definition.label, label);

            let filters_in_chain = definition
                .ffmpeg_filter_chain
                .split(',')
                .map(|filter| filter.split_once('=').map_or(filter, |(name, _)| name))
                .collect::<Vec<_>>();
            assert_eq!(
                filters_in_chain.as_slice(),
                definition.required_filters,
                "preset {} must declare every filter in its FFmpeg chain",
                definition.id()
            );
        }
    }
}
