// Independent original Project ancestry/range/size semantics.
use filmcraft_project::{Project,ItemKind,MediaClip,MediaRef,Label,SequenceSettings};
use filmcraft_media::{Generator,MediaSource,generators::GeneratorSource};
use filmcraft_time::{Tick,FrameRate,TimeRange};
use serde_json::json;
fn main(){let mut p=Project::new("Source relationships");let rate=FrameRate::new(24,1);
let generator=Generator::ColorMatte{color:[1.,0.,0.,1.]};let src=GeneratorSource::new(generator.clone(),720,480,rate,Tick(254016000000));
let media=MediaClip{media:MediaRef::Generator(generator),info:src.info().clone(),interpret:Default::default(),mark_in:None,mark_out:None,markers:vec![],offline:false,proxy:None,identity:None};
let root=p.add_item("Media",Label::Iris,ItemKind::Media(media.clone()),None);
let mut audio=media;audio.info.video=None;p.add_item("No video",Label::Iris,ItemKind::Media(audio),None);
p.new_sequence("Nested",SequenceSettings{width:123,height:57,..Default::default()},1,1,None);
p.add_item("Graphic",Label::Iris,ItemKind::Graphic{width:111,height:77,rate},None);
p.add_item("Adjustment",Label::Iris,ItemKind::AdjustmentLayer{width:91,height:63,rate,duration:Tick(1000)},None);
let mut parent=root;for depth in 1..=17 {parent=p.add_item("Subclip",Label::Iris,ItemKind::Subclip{parent,range:TimeRange::new(Tick(-depth),Tick(1000+depth)),restrict_trims:depth%2==0},None);}
let a=p.add_item("Cycle A",Label::Iris,ItemKind::Subclip{parent:root,range:TimeRange::new(Tick(5),Tick(10)),restrict_trims:false},None);
let b=p.add_item("Cycle B",Label::Iris,ItemKind::Subclip{parent:a,range:TimeRange::new(Tick(15),Tick(20)),restrict_trims:false},None);
if let ItemKind::Subclip{parent,..}=&mut p.items.get_mut(&a).unwrap().kind{*parent=b;}
let missing=serde_json::from_value(json!(9999)).unwrap();p.add_item("Missing parent",Label::Iris,ItemKind::Subclip{parent:missing,range:TimeRange::new(Tick(9007199254740993),Tick(10)),restrict_trims:false},None);
let mut ids:Vec<_>=p.items.keys().copied().collect();ids.push(missing);
let cases:Vec<_>=ids.into_iter().map(|id|json!({"id":id,"size":p.source_size(id),"resolved":p.resolve_media(id).map(|(item,media,range)|json!({"item":item,"media":media,"range":range}))})).collect();
let sequences:Vec<_>=p.sequences().map(|item|item.id).collect();
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","document":{"format":"filmcraft.project","schema_version":12,"project":p},"cases":cases,"sequences":sequences}));}
