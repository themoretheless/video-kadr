// Transition parameter definitions from pinned Filmcraft (MIT).
use filmcraft_project::EffectKind;
fn main() {
 let definitions: Vec<_> = filmcraft_project::effect_defs().iter()
  .filter(|definition| definition.kind == EffectKind::VideoTransition).collect();
 println!("{}", serde_json::json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","definitions":definitions}));
}
