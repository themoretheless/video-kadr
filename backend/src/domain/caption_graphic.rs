//! Filmcraft caption markup and caption-style conversion to an editable graphic layer.
//! Attribution: docs/licenses/filmcraft-MIT.txt.
use super::{graphic_clip::{GraphicClip,GraphicLayer,GraphicContent},graphic_text::GraphicTextParameters,graphic_text_layout::GraphicCharStyle,graphic_text_runs::CharacterStyleRun};
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,Default,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="lowercase")]
pub enum CaptionAlign {Left,#[default] Center,Right}
#[derive(Debug,Clone,Copy,Default,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="lowercase")]
pub enum CaptionAnchor {Top,Middle,#[default] Bottom}
fn color<'de,D:serde::Deserializer<'de>>(deserializer:D)->std::result::Result<[u8;4],D::Error> {
    let value=serde_json::Value::deserialize(deserializer)?;
    if let Some(text)=value.as_str() {
        let text=text.trim().trim_start_matches('#');
        if !matches!(text.len(),6|8) {return Err(serde::de::Error::custom("invalid caption color"))}
        let byte=|at|text.get(at..at+2).and_then(|pair|u8::from_str_radix(pair,16).ok()).ok_or_else(||serde::de::Error::custom("invalid caption color"));
        Ok([byte(0)?,byte(2)?,byte(4)?,if text.len()==8 {byte(6)?}else{255}])
    }else{serde_json::from_value(value).map_err(serde::de::Error::custom)}
}
#[derive(Debug,Clone,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct CaptionStyle {
    pub font:String,pub size:f32,
    #[serde(deserialize_with="color")] pub color:[u8;4],
    pub background:bool,
    #[serde(deserialize_with="color")] pub background_color:[u8;4],
    pub align:CaptionAlign,pub anchor:CaptionAnchor,pub margin:f32,pub line_spacing:f32,pub outline:f32,
    #[serde(deserialize_with="color")] pub outline_color:[u8;4],
}
impl Default for CaptionStyle {
    fn default()->Self {Self{font:"Inter".into(),size:54.,color:[255;4],background:true,background_color:[0,0,0,191],align:CaptionAlign::Center,anchor:CaptionAnchor::Bottom,margin:0.08,line_spacing:1.25,outline:0.,outline_color:[0,0,0,255]}}
}
impl CaptionStyle {
    pub fn validate(&self)->Result<()> {
        ensure!(!self.font.trim().is_empty(),"empty caption font");
        for (value,min,max) in [(self.size,4.,400.),(self.margin,0.,0.45),(self.line_spacing,0.8,4.),(self.outline,0.,40.)] {ensure!(value.is_finite()&&(min..=max).contains(&value),"invalid caption style range");}
        Ok(())
    }
}
pub fn caption_runs(text:&str)->(String,Vec<CharacterStyleRun>) {
    let mut out=String::new();let mut runs:Vec<CharacterStyleRun>=vec![];
    let (mut italic,mut bold,mut underline)=(false,false,false);let mut rest=text;let mut index=0;
    while let Some(ch)=rest.chars().next() {
        if ch=='<' {if let Some(end)=rest.find('>') {
            match rest[1..end].trim().to_ascii_lowercase().as_str() {"i"=>italic=true,"/i"=>italic=false,"b"=>bold=true,"/b"=>bold=false,"u"=>underline=true,"/u"=>underline=false,_=>{}}
            rest=&rest[end+1..];continue;
        }}
        let (ch,bytes)=match ch {'&' if rest.starts_with("&amp;")=>('&',5),'&' if rest.starts_with("&lt;")=>('<',4),'&' if rest.starts_with("&gt;")=>('>',4),ch=>(ch,ch.len_utf8())};
        out.push(ch);
        let style=GraphicCharStyle{faux_italic:italic.then_some(true),faux_bold:bold.then_some(true),underline:underline.then_some(true),..Default::default()};
        if !style.is_empty() {
            if let Some(previous)=runs.last_mut().filter(|run|run.end==index&&run.style==style) {previous.end=index+1;}
            else {runs.push(CharacterStyleRun{start:index,end:index+1,style});}
        }
        index+=1;rest=&rest[bytes..];
    }
    (out,runs)
}
#[derive(Debug,Clone,PartialEq,Serialize)]
pub struct CaptionGraphic {pub name:String,pub graphic:GraphicClip}
pub fn convert(text:&str,style:&CaptionStyle,canvas:(u32,u32),uid:u64)->Result<CaptionGraphic> {
    style.validate()?;ensure!(canvas.0>0&&canvas.1>0&&uid>0,"invalid caption canvas or layer identity");
    let (text,runs)=caption_runs(text);let (w,h)=(canvas.0 as f64,canvas.1 as f64);let scale=h/1080.;let size=style.size as f64*scale;
    let lines=text.lines().count().max(1) as f64;let line_h=size*style.line_spacing as f64;let margin=style.margin as f64*h;
    let x=match style.align {CaptionAlign::Left=>w*0.1,CaptionAlign::Center=>w/2.,CaptionAlign::Right=>w*0.9};
    let y=match style.anchor {CaptionAnchor::Bottom=>h-margin-(lines-1.)*line_h-size*0.25,CaptionAnchor::Top=>margin+size,CaptionAnchor::Middle=>h/2.-(lines-1.)*line_h/2.+size*0.35};
    let col=|c:[u8;4]|[c[0] as f32/255.,c[1] as f32/255.,c[2] as f32/255.,1.];
    let mut parameters=GraphicTextParameters{text:text.clone(),font:style.font.clone(),font_style:"SemiBold".into(),size:size as f32,position:Some([x,y]),align:match style.align {CaptionAlign::Left=>0,CaptionAlign::Center=>1,CaptionAlign::Right=>2},fill_color:col(style.color),leading:(line_h-size*1.21) as f32,runs,..Default::default()};
    if style.background {parameters.background=true;parameters.background_color=col(style.background_color);parameters.background_opacity=(style.background_color[3] as f64/2.55) as f32;parameters.background_size=(size*0.2) as f32;}
    if style.outline>0. {parameters.stroke=true;parameters.stroke_color=col(style.outline_color);parameters.stroke_width=(style.outline as f64*scale) as f32;}
    let graphic=GraphicClip{layers:vec![GraphicLayer{uid,enabled:true,original_parameters:None,pin:None,content:GraphicContent::Text{parameters,animation:None,source_text_animation:None}}]};graphic.validate()?;
    Ok(CaptionGraphic{name:text.lines().next().unwrap_or("Caption").chars().take(40).collect(),graphic})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn graphic_template_caption_upgrade_parameters_styles_and_names() {
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-caption-upgrade.json")).unwrap();
        for (index,case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            let track=&case["before"]["tracks"][0];let style:CaptionStyle=serde_json::from_value(track["style"].clone()).unwrap();
            let mut live_document=case["document"].clone();
            for (index,layer) in case["liveLayers"].as_array().unwrap().iter().enumerate() {live_document["layers"][index]["params"]=layer["params"].clone();}
            let expected=super::super::graphic_templates::from_original(&live_document).unwrap();
            let actual=convert(case["text"].as_str().unwrap(),&style,(expected.canvas[0],expected.canvas[1]),expected.graphic.layers[0].uid).unwrap();
            assert_eq!(actual.name,case["name"].as_str().unwrap(),"name{index}");
            assert_eq!(actual.graphic,expected.graphic,"graphic{index}");
        }
    }
}
