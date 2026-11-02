#[path = "../src/domain/concat_format_plan.rs"]
mod native;
use native::{Filter, Format, Plan};
use std::collections::BTreeMap;
#[test]
#[ignore = "requires actual FFmpeg per-stage format negotiation"]
fn original_stage_formats_match_linear_plan() {
    let mut chains: Vec<Vec<&str>> = vec![
        vec!["aresample=48000"],
        vec!["asetrate=48000"],
        vec!["atempo=0.66741993"],
        vec!["highpass=f=120"],
        vec!["lowpass=f=3400"],
        vec!["equalizer=f=140:t=q:w=1:g=3"],
        vec!["treble=g=4"],
        vec!["bass=g=5"],
        vec!["aecho=0.8:0.85:100:0.4"],
        vec!["alimiter=limit=0.94"],
        vec!["compand"],
        vec!["deesser=i=0.3"],
        vec!["afftdn=nr=12:nf=-50"],
        vec!["acrusher=bits=8:mix=0.5"],
        vec!["acompressor=threshold=0.1:ratio=2"],
        vec!["aphaser"],
        vec!["flanger"],
        vec!["volume=1.2"],
        vec!["loudnorm=I=-16:TP=-1.5:LRA=11"],
    ];
    let pitch = vec![
        "aresample=48000",
        "asetrate=71919",
        "aresample=48000",
        "atempo=0.66741993",
    ];
    chains.push(pitch.clone());
    for suffix in [
        vec!["highpass=f=120", "alimiter=limit=0.94"],
        vec!["aecho=0.8:0.85:100:0.4"],
        vec![
            "compand",
            "deesser=i=0.3",
            "aecho=0.8:0.85:100:0.4",
            "alimiter=limit=0.94",
        ],
        vec!["afftdn=nr=12:nf=-50"],
        vec!["volume=1.2"],
        vec!["aphaser"],
        vec!["flanger"],
    ] {
        let mut chain = pitch.clone();
        chain.extend(suffix.clone());
        chains.push(chain);
        let mut chain = suffix;
        chain.extend(pitch.clone());
        chains.push(chain);
    }
    let mut cases = 0;
    let mut stages = 0;
    for source in [
        "u8", "s16", "s32", "flt", "dbl", "u8p", "s16p", "s32p", "fltp", "dblp", "s64", "s64p",
    ] {
        for (sink, codec) in [
            ("u8", "pcm_u8"),
            ("s16", "pcm_s16le"),
            ("s32", "pcm_s32le"),
            ("flt", "pcm_f32le"),
            ("dbl", "pcm_f64le"),
        ] {
            for (index, chain) in chains.iter().enumerate() {
                let kinds: Vec<Filter> = chain
                    .iter()
                    .map(|s| Filter::from_name(s.split('=').next().unwrap()).unwrap())
                    .collect();
                let plan = Plan::build(
                    Format::parse(source).unwrap(),
                    &kinds,
                    Format::parse(sink).unwrap(),
                )
                .unwrap();
                let mut graph = format!("aformat=sample_fmts={source}");
                for filter in chain {
                    graph.push(',');
                    graph.push_str(filter);
                    graph.push_str(",ashowinfo");
                }
                let output = std::process::Command::new("ffmpeg")
                    .args([
                        "-nostdin",
                        "-v",
                        "debug",
                        "-filter_threads",
                        "1",
                        "-f",
                        "lavfi",
                        "-i",
                        "anullsrc=r=44100:cl=stereo:d=0.25",
                        "-af",
                        &graph,
                        "-c:a",
                        codec,
                        "-f",
                        "null",
                        "-",
                    ])
                    .output()
                    .unwrap();
                let log = String::from_utf8_lossy(&output.stderr);
                assert!(
                    output.status.success(),
                    "{source}->{sink} chain{index}: {log}"
                );
                let mut actual = BTreeMap::new();
                for line in log.lines().filter(|line| {
                    line.contains("Parsed_ashowinfo_")
                        && line.contains(" n:")
                        && line.contains("fmt:")
                }) {
                    let id: usize = line
                        .split("Parsed_ashowinfo_")
                        .nth(1)
                        .unwrap()
                        .split_whitespace()
                        .next()
                        .unwrap()
                        .parse()
                        .unwrap();
                    let format = line
                        .split("fmt:")
                        .nth(1)
                        .unwrap()
                        .split_whitespace()
                        .next()
                        .unwrap();
                    if let Some(previous) = actual.insert(id, format.to_owned()) {
                        assert_eq!(previous, format, "format changed midstream");
                    }
                }
                assert_eq!(
                    actual.len(),
                    chain.len(),
                    "{source}->{sink} chain{index}: missing output frames {log}"
                );
                for (stage, (actual, expected)) in actual.values().zip(&plan.stages).enumerate() {
                    assert_eq!(
                        actual,
                        expected.output.name(),
                        "{source}->{sink} chain{index} stage{stage} {:?}: plan {:?}\n{log}",
                        chain[stage],
                        plan.stages
                    );
                }
                assert!(plan
                    .conversions
                    .iter()
                    .all(|c| c.before <= chain.len() && c.input != c.output));
                cases += 1;
                stages += chain.len();
            }
        }
        println!("source {source}: all stage-format profiles passed");
    }
    println!("qualified {cases} real graphs and {stages} stage formats");
}
#[test]
fn graph_budget_and_unknown_filters_are_explicit() {
    assert!(Filter::from_name("not-a-filter").is_none());
    assert!(Format::parse("not-a-format").is_none());
    assert!(Plan::build(Format::F32, &vec![Filter::Pass; 257], Format::F32).is_err());
    let p = Plan::build(
        Format::S16,
        &[
            Filter::Resample,
            Filter::Pass,
            Filter::Resample,
            Filter::Tempo,
        ],
        Format::F32,
    )
    .unwrap();
    assert_eq!(p.stages[0].output, Format::S16);
    assert_eq!(p.stages[3].output, Format::F32);
}
