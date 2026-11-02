use serde_json::{json,Value};
fn main() {
 let root=std::path::PathBuf::from(std::env::var("VIDEOKADR_CONTROL_OUTPUT").unwrap());
 let texts=["Plain caption","Hello <i>there</i>","<b>Bold <i>both</i></b>","<u>Underlined</u>","<b>A</b><i>B</i><u>C</u>","&amp; &lt; &gt; &quot;","Привет 🙂<b>мир</b>","Line one\nLine <i>two</i>","<unknown>visible</unknown>","<b>unclosed","<b>outer<b>inner</b> tail</b>","< I >space< /I >","literal < without close","", "\nSecond line", "🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂🙂"];
 let mut cases=Vec::new();
 for (index,text) in texts.iter().enumerate() {
  let mut session=filmcraft_engine::Session::default();
  session.execute("file.openDemoProject",json!({})).unwrap();
  session.execute("playhead.set",json!({"seconds":600})).unwrap();
  session.execute("captions.add",json!({"text":text,"seconds":600,"durationSeconds":2})).unwrap();
  let style=json!({"size":42+index,"align":(["left","center","right"][index%3]),"anchor":(["top","middle","bottom"][index%3]),"background":index%2==0,"backgroundColor":"#12345680","outline":2,"outlineColor":"#234567ff","color":"#abcdef80","lineSpacing":1.25,"margin":0.08});
  session.execute("captions.setStyle",style.clone()).unwrap();
  let before=session.execute("captions.list",json!({})).unwrap();
  let result=session.execute("graphics.upgradeCaption",json!({})).unwrap();
  assert_eq!(result["upgraded"],1);
  let clip=result["clips"][0].as_u64().unwrap();
  let item=session.active_sequence().unwrap().find_item(filmcraft_project::ClipId(clip)).unwrap().1.clone();
  let path=root.join(format!("caption-{index}.fcgt"));
  session.execute("graphics.template.export",json!({"clip":clip,"name":"Upgraded","path":path,"controls":[]})).unwrap();
  let document:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
  let after=session.execute("captions.list",json!({})).unwrap();
  session.execute("edit.undo",json!({})).unwrap();
  let restored=session.execute("captions.list",json!({})).unwrap();
  assert_eq!(restored["tracks"],before["tracks"]);
  assert!(session.active_sequence().unwrap().find_item(filmcraft_project::ClipId(clip)).is_none());
  cases.push(json!({"text":text,"styleRequest":style,"before":before,"after":after,"document":document,"liveLayers":item.effects.iter().filter(|effect|filmcraft_project::graphic::is_layer(effect)).collect::<Vec<_>>(),"start":item.start.0,"duration":item.duration.0,"name":item.name,"undoRestored":true}));
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
