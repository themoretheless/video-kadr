//! Caption-to-graphic editing transactions preserve the authored caption document.
use super::{composition::{Composition,CompositionClipId,CompositionTrack,TrackId},caption_timeline::CaptionUpgrade};
use anyhow::Result;
impl Composition {
    pub fn upgrade_captions(&self,ids:&[CompositionClipId])->Result<Composition> {
        self.validate()?;
        let CaptionUpgrade{tracks,graphics}=super::caption_timeline::upgrade(&self.caption_tracks,ids,(self.canvas.width,self.canvas.height))?;
        let mut next=self.clone();next.caption_tracks=tracks;
        let mut clips=vec![];
        for graphic in graphics {
            next.metadata.get_or_insert_with(Default::default).clip_names.insert(graphic.clip.id.clone(),graphic.name);
            clips.push(graphic.clip);
        }
        next.tracks.insert(0,CompositionTrack::Graphic{id:TrackId::parse(format!("caption-graphics-{}",uuid::Uuid::new_v4()))?,name:"Captions as Graphics".into(),hidden:false,locked:false,clips});
        next.validate()?;Ok(next)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{caption_timeline::{CaptionCue,CaptionTrack},caption_graphic::CaptionStyle,composition::CanvasSpec};
    #[test]
    fn caption_document_roundtrip_render_upgrade_and_lock() {
        let canvas:CanvasSpec=serde_json::from_value(serde_json::json!({"width":320,"height":180,"fpsMilli":24000,"background":{"red":0,"green":0,"blue":0,"alpha":1}})).unwrap();
        let mut document=Composition::new(canvas);
        let id=CompositionClipId::parse("caption-one").unwrap();
        document.caption_tracks.push(CaptionTrack{format:None,language:None,sync_lock:None,vtt_blocks:vec![], id:TrackId::parse("captions").unwrap(),name:"Captions".into(),hidden:false,locked:false,style:CaptionStyle::default(),clips:vec![CaptionCue{cue_id:None, id:id.clone(),timeline_start_tick:(0_u64).into(),timeline_end_tick:(2_000_000_u64).into(),text:"Hello <i>there</i>".into(),speaker:Some("Narrator".into()),settings:"line:90%".into()}]});
        document.validate().unwrap();assert_eq!(document.duration_ticks().unwrap(),2_000_000);
        let before=document.clone();let saved=serde_json::to_value(&document).unwrap();
        assert_eq!(serde_json::from_value::<Composition>(saved).unwrap(),document);
        let original=super::super::flat_surface::render_generated(&document,1_000_000,&mut||Ok(())).unwrap().unwrap();
        let upgraded=document.upgrade_captions(&[id.clone()]).unwrap();assert!(upgraded.caption_tracks[0].clips.is_empty());
        let after=super::super::flat_surface::render_generated(&upgraded,1_000_000,&mut||Ok(())).unwrap().unwrap();
        assert!(original.rgba.chunks_exact(4).any(|p|p[0]>0));assert!(after.rgba.chunks_exact(4).any(|p|p[0]>0));
        // Source caption burn-in and editable graphic conversion intentionally use different layouts.
        assert_ne!(original,after);assert_eq!(document,before);
        document.caption_tracks[0].locked=true;assert!(document.upgrade_captions(&[id]).is_err());assert_eq!(document.caption_tracks[0].clips.len(),1);
    }
}
