//! Editable caption tracks and an atomic, functional caption-to-graphic transaction.
use super::caption_time::CaptionTime;
use super::{caption_graphic::CaptionStyle,composition::{CompositionClipId,TrackId,TransformSpec,AnimatableValue},graphic_timeline::GraphicTimelineClip};
use anyhow::{ensure,Result};
use serde::{Serialize,Deserialize};
use std::collections::BTreeSet;
#[derive(Debug,Clone,Copy,Default,PartialEq,Eq,Serialize,Deserialize)]
pub enum CaptionStreamFormat {
    #[default] Subtitle,
    #[serde(rename="CEA-608",alias="Cea608")] Cea608,
    #[serde(rename="CEA-708",alias="Cea708")] Cea708,
    Teletext,
}
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct CaptionCue {
    pub id:CompositionClipId,
    #[serde(default,skip_serializing_if="Option::is_none")] pub cue_id:Option<String>,
    pub timeline_start_tick:CaptionTime,
    pub timeline_end_tick:CaptionTime,
    pub text:String,
    #[serde(default)] pub speaker:Option<String>,
    #[serde(default)] pub settings:String,
}
impl CaptionCue {
    pub fn validate(&self)->Result<()> {
        self.timeline_start_tick.validate_timeline_point().map_err(anyhow::Error::msg)?;
        self.timeline_end_tick.validate_timeline_point().map_err(anyhow::Error::msg)?;
        ensure!(self.timeline_end_tick>self.timeline_start_tick,"invalid caption placement");
        ensure!(self.text.len()<=1_000_000,"caption text too large");Ok(())
    }
    pub fn to_graphic(&self,style:&CaptionStyle,canvas:(u32,u32))->Result<UpgradedCaption> {
        self.validate()?;
        let converted=super::caption_graphic::convert(&self.text,style,canvas,1)?;
        let clip=GraphicTimelineClip {
            concat_shape: None, concat_treatment: None, concat_shape_blend: None,
            id:CompositionClipId::parse(format!("caption-graphic-{}",uuid::Uuid::new_v4()))?,
            timeline_start_tick:self.timeline_start_tick.exact_microseconds().map_err(anyhow::Error::msg)?,timeline_end_tick:self.timeline_end_tick.exact_microseconds().map_err(anyhow::Error::msg)?,
            animation_offset_us:0,template:None,roll:None,responsive_time:None,
            width:canvas.0,height:canvas.1,graphic:converted.graphic,effects:vec![],transform:TransformSpec::default(),opacity:AnimatableValue::constant(1.),enabled:true,
        };
        clip.validate()?;Ok(UpgradedCaption{caption_id:self.id.clone(),name:converted.name,clip})
    }
}
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct CaptionTrack {
    pub id:TrackId,pub name:String,
    #[serde(default,skip_serializing_if="Option::is_none")] pub format:Option<CaptionStreamFormat>,
    #[serde(default,skip_serializing_if="Option::is_none")] pub language:Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")] pub sync_lock:Option<bool>,
    #[serde(default,skip_serializing_if="Vec::is_empty")] pub vtt_blocks:Vec<String>,
    #[serde(default)] pub hidden:bool,
    #[serde(default)] pub locked:bool,
    #[serde(default)] pub style:CaptionStyle,
    #[serde(default)] pub clips:Vec<CaptionCue>,
}
impl CaptionTrack {
    pub fn validate(&self)->Result<()> {
        self.style.validate()?;
        for cue in &self.clips {cue.validate()?;}
        ensure!(self.clips.windows(2).all(|pair|pair[0].timeline_end_tick<=pair[1].timeline_start_tick),"captions must be sorted and non-overlapping");
        Ok(())
    }
}
#[derive(Debug,Clone,PartialEq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct UpgradedCaption {pub caption_id:CompositionClipId,pub name:String,pub clip:GraphicTimelineClip}
#[derive(Debug,Clone,PartialEq,Serialize)]
#[serde(rename_all="camelCase")]
pub struct CaptionUpgrade {pub tracks:Vec<CaptionTrack>,pub graphics:Vec<UpgradedCaption>}
pub fn upgrade(tracks:&[CaptionTrack],ids:&[CompositionClipId],canvas:(u32,u32))->Result<CaptionUpgrade> {
    ensure!(!ids.is_empty(),"select captions to upgrade");
    let selected:BTreeSet<_>=ids.iter().collect();
    let mut track_ids=BTreeSet::new();let mut cue_ids=BTreeSet::new();let mut graphics=vec![];
    for track in tracks {
        ensure!(track_ids.insert(&track.id),"duplicate caption track");track.validate()?;
        for cue in &track.clips {
            cue.validate()?;ensure!(cue_ids.insert(&cue.id),"duplicate caption id");
            if selected.contains(&cue.id) {ensure!(!track.locked,"caption track is locked");graphics.push(cue.to_graphic(&track.style,canvas)?);}
        }
    }
    ensure!(!graphics.is_empty(),"no such captions");
    let upgraded:BTreeSet<_>=graphics.iter().map(|graphic|&graphic.caption_id).collect();
    let mut remaining=tracks.to_vec();
    for track in &mut remaining {track.clips.retain(|cue|!upgraded.contains(&cue.id));}
    Ok(CaptionUpgrade{tracks:remaining,graphics})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_caption_upgrade_placement_and_atomic_removal() {
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-caption-upgrade.json")).unwrap();
        for (index,case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            let source=&case["before"]["tracks"][0];let cue=&source["captions"][0];
            let start=cue["start"].as_u64().unwrap();let end=cue["end"].as_u64().unwrap();assert_eq!(start%254016,0);assert_eq!(end%254016,0);
            let id=CompositionClipId::parse(format!("caption-{index}")).unwrap();
            let tracks=vec![CaptionTrack{format:None,language:None,sync_lock:None,vtt_blocks:vec![], id:TrackId::parse("captions").unwrap(),name:"Captions".into(),hidden:false,locked:false,style:serde_json::from_value(source["style"].clone()).unwrap(),clips:vec![CaptionCue{cue_id:None, id:id.clone(),timeline_start_tick:(start/254016).into(),timeline_end_tick:(end/254016).into(),text:cue["text"].as_str().unwrap().into(),speaker:None,settings:String::new()}]}];
            let before=tracks.clone();let result=upgrade(&tracks,&[id.clone()],(1920,1080)).unwrap();
            assert!(result.tracks[0].clips.is_empty());assert_eq!(result.graphics.len(),1);
            assert_eq!(result.graphics[0].clip.timeline_start_tick*254016,case["start"].as_u64().unwrap());
            assert_eq!((result.graphics[0].clip.timeline_end_tick-result.graphics[0].clip.timeline_start_tick)*254016,case["duration"].as_u64().unwrap());
            assert_eq!(tracks,before);
            let mut locked=tracks.clone();locked[0].locked=true;assert!(upgrade(&locked,&[id],(1920,1080)).is_err());assert_eq!(locked[0].clips.len(),1);
        }
    }
    #[test]
    fn caption_metadata_roundtrips_and_survives_split_merge_and_upgrade(){
        let value=serde_json::json!({"id":"captions","name":"French","format":"CEA-608","language":"fr","syncLock":false,"vttBlocks":["NOTE retained","STYLE\n::cue { color: yellow }"],"clips":[{"id":"first","cueId":"file-cue-1","timelineStartTick":0,"timelineEndTick":2000000,"text":"Hello","speaker":"Ann","settings":"align:start"}]});
        let track:CaptionTrack=serde_json::from_value(value).unwrap();track.validate().unwrap();
        let restored:CaptionTrack=serde_json::from_str(&serde_json::to_string(&track).unwrap()).unwrap();assert_eq!(restored,track);
        let first=track.clips[0].id.clone();let right=CompositionClipId::parse("right").unwrap();
        let split=super::super::caption_edit::edit(&[track.clone()],super::super::caption_edit::CaptionEdit::Split{id:first.clone(),time:1000000.into(),new_id:right.clone()},40000).unwrap();
        assert_eq!(split.tracks[0].format,track.format);assert_eq!(split.tracks[0].vtt_blocks,track.vtt_blocks);
        assert_eq!(split.tracks[0].clips[0].cue_id.as_deref(),Some("file-cue-1"));
        assert!(split.tracks[0].clips[1].cue_id.is_none());
        let merged=super::super::caption_edit::edit(&split.tracks,super::super::caption_edit::CaptionEdit::Merge{ids:vec![first.clone(),right]},40000).unwrap();
        assert_eq!(merged.tracks[0].clips[0].cue_id.as_deref(),Some("file-cue-1"));
        let converted=upgrade(&merged.tracks,&[first],(320,180)).unwrap();
        assert_eq!(converted.tracks[0].language.as_deref(),Some("fr"));assert_eq!(converted.tracks[0].sync_lock,Some(false));assert_eq!(converted.tracks[0].vtt_blocks,track.vtt_blocks);
        let legacy:CaptionTrack=serde_json::from_value(serde_json::json!({"id":"legacy","name":"Legacy","clips":[]})).unwrap();
        let wire=serde_json::to_value(legacy).unwrap();
        for key in ["format","language","syncLock","vttBlocks"]{assert!(wire.get(key).is_none(),"{key}");}
    }

}
