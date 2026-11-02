//! Original audio package contracts pinned at Concat78e4be1; no DSP implementation is duplicated.
use super::{
    concat_effect_upgrade::AppliedFilter,
    filmcraft_dsp::{validate_specs, EffectSpec},
};
use anyhow::{ensure, Context, Result};
struct Contract {
    id: &'static str,
    aliases: &'static [&'static str],
    params: &'static [(&'static str, f64, f64, f64)],
}
static CONTRACTS: &[Contract] = &[
    Contract {
        id: "concat.bass",
        aliases: &["bass"],
        params: &[("gain", 5.0, 0.0, 12.0)],
    },
    Contract {
        id: "concat.bright",
        aliases: &["bright"],
        params: &[("gain", 5.0, 0.0, 12.0)],
    },
    Contract {
        id: "concat.cathedral",
        aliases: &["cathedral"],
        params: &[("size", 55.0, 0.0, 100.0)],
    },
    Contract {
        id: "concat.chipmunk",
        aliases: &["chipmunk"],
        params: &[("pitch", 7.0, 3.0, 12.0)],
    },
    Contract {
        id: "concat.deep",
        aliases: &["deep"],
        params: &[("pitch", -3.0, -8.0, -1.0), ("body", 3.0, 0.0, 8.0)],
    },
    Contract {
        id: "concat.denoise",
        aliases: &[],
        params: &[("strength", 50.0, 0.0, 100.0)],
    },
    Contract {
        id: "concat.distorted",
        aliases: &[],
        params: &[("drive", 50.0, 0.0, 100.0)],
    },
    Contract {
        id: "concat.echo",
        aliases: &["echo"],
        params: &[("delay", 0.25, 0.05, 1.0), ("decay", 0.4, 0.1, 0.9)],
    },
    Contract {
        id: "concat.enhance-voice",
        aliases: &[],
        params: &[("amount", 60.0, 0.0, 100.0)],
    },
    Contract {
        id: "concat.hall",
        aliases: &["hall"],
        params: &[("size", 50.0, 0.0, 100.0)],
    },
    Contract {
        id: "concat.lofi",
        aliases: &[],
        params: &[("bits", 8.0, 4.0, 12.0)],
    },
    Contract {
        id: "concat.loudness",
        aliases: &[],
        params: &[("target", -23.0, -30.0, -12.0)],
    },
    Contract {
        id: "concat.megaphone",
        aliases: &["bullhorn", "loudspeaker"],
        params: &[("drive", 5.0, 1.0, 10.0)],
    },
    Contract {
        id: "concat.pitch",
        aliases: &[],
        params: &[("semitones", 0.0, -12.0, 12.0)],
    },
    Contract {
        id: "concat.plate",
        aliases: &["plate"],
        params: &[("size", 40.0, 0.0, 100.0)],
    },
    Contract {
        id: "concat.radio",
        aliases: &["radio"],
        params: &[("noise", 30.0, 0.0, 100.0)],
    },
    Contract {
        id: "concat.robot",
        aliases: &["robot"],
        params: &[("depth", 5.0, 1.0, 10.0)],
    },
    Contract {
        id: "concat.room",
        aliases: &["room"],
        params: &[("size", 40.0, 0.0, 100.0)],
    },
    Contract {
        id: "concat.stadium",
        aliases: &["arena"],
        params: &[("size", 6.0, 1.0, 10.0)],
    },
    Contract {
        id: "concat.sweet",
        aliases: &["sweet"],
        params: &[("amount", 65.0, 0.0, 100.0), ("pitch", 0.0, 0.0, 8.0)],
    },
    Contract {
        id: "concat.telephone",
        aliases: &["telephone"],
        params: &[("drive", 3.0, 0.0, 10.0)],
    },
    Contract {
        id: "concat.treble",
        aliases: &["treble"],
        params: &[("gain", 4.0, 0.0, 12.0)],
    },
    Contract {
        id: "concat.underwater",
        aliases: &["submerged"],
        params: &[("depth", 5.0, 1.0, 10.0)],
    },
    Contract {
        id: "concat.vinyl",
        aliases: &["record", "gramophone"],
        params: &[("warmth", 5.0, 0.0, 10.0)],
    },
    Contract {
        id: "concat.whisper",
        aliases: &["breathy", "intimate"],
        params: &[("air", 5.0, 1.0, 10.0)],
    },
];
pub(crate) fn validate_source_keys(id:&str,keys:&std::collections::BTreeMap<String,Vec<super::concat_effect_upgrade::ParamKey>>)->Result<()>{
 if keys.is_empty(){return Ok(())}
 let contract=CONTRACTS.iter().find(|contract|contract.id==id||contract.aliases.contains(&id)).context("source keys require a known Concat audio effect")?;
 // Pinned Catalogue::audio_chain/compose reads params only. Source
 // filter keys are inactive metadata retained in the project and sourceDocument.
 ensure!(keys.len()<=128,"Concat audio source-key parameter budget exceeded");
 let mut total=0usize;
 for(name,keys)in keys{
     ensure!(contract.params.iter().any(|param|param.0==name),"unknown Concat audio source-key parameter");
     total=total.checked_add(keys.len()).context("Concat audio source-key count overflow")?;
     ensure!(keys.len()<=4096&&total<=8192,"Concat audio source-key budget exceeded");
     ensure!(keys.iter().all(|key|key.at.is_finite()&&(0.0..=1.0).contains(&key.at)&&key.value.is_finite()&&key.ease.iter().all(|value|value.is_finite())),"invalid Concat audio source key");
 }
 Ok(())
}
pub fn convert(links: &[AppliedFilter]) -> Result<Vec<EffectSpec>> {
    ensure!(links.len() <= 16, "audio filter chain exceeds16 effects");
    let result = links
        .iter()
        .map(|link| {
            let contract = CONTRACTS
                .iter()
                .find(|c| c.id == link.id || c.aliases.contains(&link.id.as_str()))
                .with_context(|| format!("unknown Concat audio effect {}", link.id))?;
            validate_source_keys(&link.id,&link.keys)?;
            ensure!(
                link.params
                    .keys()
                    .all(|key| contract.params.iter().any(|p| p.0 == key)),
                "unknown Concat audio parameter"
            );
            let params = contract
                .params
                .iter()
                .map(|&(name, default, lo, hi)| {
                    let value = link.params.get(name).copied().unwrap_or(default);
                    ensure!(
                        value.is_finite() && (lo..=hi).contains(&value),
                        "{}.{name}: original audio parameter out of range",
                        contract.id
                    );
                    Ok((name.to_owned(), value as f32))
                })
                .collect::<Result<_>>()?;
            Ok(EffectSpec { concat_source_keys: link.keys.clone(),
                id: contract.id.into(),
                params,
                bypass: !link.enabled,
                animation: Default::default(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    validate_specs(&result)?;
    Ok(result)
}
