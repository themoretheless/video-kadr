// Read-only catalog emitter; compile against the pinned FilmCraft reference.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let commands:Vec<_>=filmcraft_engine::commands::command_specs().iter().map(|c|serde_json::json!({"id":c.id,"label":c.label,"menu":c.menu,"shortcut":c.shortcut,"params":c.params,"journal":c.journal})).collect();
    let formats: Vec<_> = filmcraft_export::Format::ALL
        .into_iter()
        .map(|f| f.id())
        .collect();
    let catalog = serde_json::json!({"schemaVersion":1,"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","commands":commands,"effects":filmcraft_project::effect::effect_defs(),"exportFormats":formats,"exportPresets":filmcraft_export::presets::builtin_presets(),"videoTransitionTree":filmcraft_project::vtransition::video_transition_tree()});
    let output = std::env::args().nth(1).ok_or("output path required")?;
    std::fs::write(output, serde_json::to_vec_pretty(&catalog)?)?;
    Ok(())
}
