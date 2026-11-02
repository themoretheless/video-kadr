//! SPDX-License-Identifier: AGPL-3.0-or-later
//! Native retired-effect link migration from pinned Concat catalogue/manifests.
//! This preserves original links; it does not import a whole Concat project.
use anyhow::{ensure, Result};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

fn yes() -> bool {
    true
}
fn linear_ease() -> [f64; 4] {
    [0., 0., 1., 1.]
}
fn ease<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<[f64; 4], D::Error> {
    let v = Value::deserialize(d)?;
    Ok(match v {
        Value::String(s) => match s.as_str() {
            "in" => [0.42, 0., 1., 1.],
            "out" => [0., 0., 0.58, 1.],
            "inOut" => [0.42, 0., 0.58, 1.],
            _ => linear_ease(),
        },
        Value::Array(a) if a.len() == 4 => {
            let p: Option<Vec<f64>> = a.iter().map(Value::as_f64).collect();
            match p {
                Some(p) if p.iter().all(|v| v.is_finite()) => {
                    [p[0].clamp(0., 1.), p[1], p[2].clamp(0., 1.), p[3]]
                }
                _ => linear_ease(),
            }
        }
        _ => linear_ease(),
    })
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamKey {
    /// Computed by clip windowing; finite sampled values may exceed authored slider bounds.
    #[serde(default, skip_serializing_if="std::ops::Not::not")]
    pub derived: bool,
    pub at: f64,
    pub value: f64,
    #[serde(default = "linear_ease", deserialize_with = "ease")]
    pub ease: [f64; 4],
}
fn runs<'de, D: Deserializer<'de>>(
    d: D,
) -> std::result::Result<BTreeMap<String, Vec<ParamKey>>, D::Error> {
    let v = Value::deserialize(d)?;
    let Value::Object(map) = v else {
        return Ok(BTreeMap::new());
    };
    Ok(map
        .into_iter()
        .filter_map(|(name, value)| {
            let keys: Vec<ParamKey> = value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();
            (!keys.is_empty()).then_some((name, keys))
        })
        .collect())
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedFilter {
    pub id: String,
    #[serde(default)]
    pub params: BTreeMap<String, f64>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(
        default,
        deserialize_with = "runs",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub keys: BTreeMap<String, Vec<ParamKey>>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Upgrade {
    pub upgraded: bool,
    pub link: AppliedFilter,
}

fn mapped(
    link: &AppliedFilter,
    old: &str,
    default: f64,
    convert: impl Fn(f64) -> f64,
) -> Result<(f64, Vec<ParamKey>)> {
    let held = convert(*link.params.get(old).unwrap_or(&default));
    ensure!(
        held.is_finite(),
        "retired effect parameter cannot be converted"
    );
    let keys = link
        .keys
        .get(old)
        .into_iter()
        .flatten()
        .map(|key| {
            let value = convert(key.value);
            ensure!(value.is_finite(), "retired effect key cannot be converted");
            Ok(ParamKey {derived:key.derived,
                at: key.at,
                value,
                ease: key.ease,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((held, keys))
}
pub fn upgrade(link: AppliedFilter) -> Result<Upgrade> {
    ensure!(
        !link.id.is_empty() && link.id.len() <= 256,
        "invalid effect id"
    );
    ensure!(
        link.params.len() <= 128 && link.keys.len() <= 128,
        "too many effect parameters"
    );
    ensure!(
        link.params.values().all(|v| v.is_finite()),
        "nonfinite effect parameter"
    );
    ensure!(
        link.keys.values().map(Vec::len).sum::<usize>() <= 4096,
        "too many effect keys"
    );
    ensure!(
        link.keys.values().flatten().all(|k| k.at.is_finite()
            && k.value.is_finite()
            && k.ease.iter().all(|v| v.is_finite())),
        "nonfinite effect key"
    );
    let mut params = BTreeMap::new();
    let mut keys = BTreeMap::new();
    let id = match link.id.as_str() {
        "concat.box-blur" | "box-blur" => {
            let (value, run) = mapped(&link, "radius", 6., |r| {
                (r * (r + 1.) / 3.).sqrt().clamp(1., 50.)
            })?;
            params.insert("radius".into(), value);
            if !run.is_empty() {
                keys.insert("radius".into(), run);
            }
            "concat.gaussian-blur"
        }
        "concat.green-screen" | "green-screen" | "concat.blue-screen" | "blue-screen" => {
            let color = if link.id.ends_with("green-screen") {
                0x00ff00ffu32
            } else {
                0x0000ffffu32
            };
            params.insert("color".into(), color as f64);
            params.insert("spill".into(), 0.);
            for (name, default, min, max) in
                [("similarity", 25., 1., 60.), ("softness", 10., 0., 50.)]
            {
                let (value, run) = mapped(&link, name, default, |v| (v * 2.).clamp(min, max))?;
                params.insert(name.into(), value);
                if !run.is_empty() {
                    keys.insert(name.into(), run);
                }
            }
            "concat.chroma-key"
        }
        _ => {
            return Ok(Upgrade {
                upgraded: false,
                link,
            })
        }
    };
    Ok(Upgrade {
        upgraded: true,
        link: AppliedFilter {
            id: id.into(),
            params,
            keys,
            enabled: link.enabled,
        },
    })
}

/// Explicit native admission for the supported Gaussian link; canonical data remains returned separately.
pub fn native_gaussian_effect(
    link: &AppliedFilter,
) -> Result<Option<super::composition::VideoEffect>> {
    if !link.enabled || link.id != "concat.gaussian-blur" {
        return Ok(None);
    }
    let parameters = super::concat_gaussian_blur::GaussianBlur {
        radius: link.params.get("radius").copied().unwrap_or(10.) as f32,
    };
    parameters.validate()?;
    let concat_keys = link
        .keys
        .get("radius")
        .filter(|keys| !keys.is_empty())
        .cloned();
    if let Some(keys) = &concat_keys {
        super::concat_gaussian_blur::validate_concat_keys(keys)?;
    }
    Ok(Some(super::composition::VideoEffect::ConcatGaussianBlur {
        parameters,
        animation: None,
        concat_keys,
    }))
}

/// Validate static source contracts before float narrowing; never discard source keys.
fn static_filter_parameters(link:&AppliedFilter, fields:&[(&str,f64,f64,f64)])->Result<Value>{
    ensure!(link.params.keys().all(|key|fields.iter().any(|field|field.0==key.as_str())),"unknown {} parameter",link.id);
    ensure!(link.keys.values().all(Vec::is_empty),"{} source animation is not yet supported by native conversion",link.id);
    let mut parameters=serde_json::Map::new();
    for &(key,default,low,high) in fields {
        let value=link.params.get(key).copied().unwrap_or(default);
        ensure!(value.is_finite()&&(low..=high).contains(&value),"{} parameter {key} out of range",link.id);
        parameters.insert(key.into(),serde_json::json!(value));
    }
    Ok(Value::Object(parameters))
}

fn source_curve_key(key:&str)->bool{
    if ["luma","red","green","blue"].contains(&key){return true;}
    let Some((root,rest))=key.split_once('.')else{return false;};
    ["luma","red","green","blue"].contains(&root)&&rest.split_once('.').is_some_and(|(index,axis)|index.parse::<usize>().is_ok_and(|index|index<8)&&matches!(axis,"x"|"y"))
}
fn source_curve_points(link:&AppliedFilter,root:&str)->Vec<[f64;2]>{
    let mut points:Vec<[f64;2]>=(0..8).filter_map(|index|{
        let x=*link.params.get(&format!("{root}.{index}.x"))?;
        let y=*link.params.get(&format!("{root}.{index}.y"))?;
        Some([x.clamp(0.,1.),y.clamp(0.,1.)])
    }).collect();
    points.sort_by(|a,b|a[0].total_cmp(&b[0]));
    points.dedup_by(|later,earlier|{let same=(later[0]-earlier[0]).abs()<1e-6;if same{*earlier=*later;}same});
    if points.len()<2{vec![[0.,0.],[1.,1.]]}else{points}
}

/// Native descriptors are explicit; canonical source links always remain available.
pub fn native_effect(link: &AppliedFilter) -> Result<Option<super::composition::VideoEffect>> {
    if !link.enabled {
        return Ok(None);
    }
    if matches!(link.id.as_str(),"concat.pixelate"|"concat.posterize"){
        let (channel,default,high,kind)=if link.id=="concat.pixelate"{("size",16.,64.,"concat_pixelate")}else{("levels",4.,8.,"concat_posterize")};
        ensure!(link.params.keys().all(|name|name==channel)&&link.keys.values().all(Vec::is_empty),"Unsupported integer source effect parameter/animation");
        let value=link.params.get(channel).copied().unwrap_or(default);
        ensure!(value.is_finite()&&value.fract()==0.&&(2. ..=high).contains(&value),"{} requires an integer {channel} in range 2..={high}",link.id);
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":kind,"parameters":{channel:value as u32}}))?;
        effect.validate().map_err(anyhow::Error::msg)?;return Ok(Some(effect));
    }
    if link.id=="concat.neon" {
        let parameters=static_filter_parameters(link,&[("glow",60.0,0.0,100.0),("punch",50.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_neon","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.pink-fuzz" {
        let parameters=static_filter_parameters(link,&[("haze",60.0,0.0,100.0),("blush",50.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_pink_fuzz","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.night" {
        let parameters=static_filter_parameters(link,&[("lift",60.0,0.0,100.0),("clarity",40.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_night","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.spooky" {
        let parameters=static_filter_parameters(link,&[("dread",60.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_spooky","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.cool" {
        let parameters=static_filter_parameters(link,&[("temperature",8500.0,7000.0,11000.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_cool","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.warm" {
        let parameters=static_filter_parameters(link,&[("temperature",4600.0,3000.0,6000.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_warm","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.teal-orange" {
        let parameters=static_filter_parameters(link,&[("amount",70.0,0.0,100.0),("contrast",25.0,0.0,100.0),("skin",80.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_teal_orange","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.matcha" {
        let parameters=static_filter_parameters(link,&[("amount",60.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_matcha","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.sage" {
        let parameters=static_filter_parameters(link,&[("amount",60.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_sage","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.ivory" {
        let parameters=static_filter_parameters(link,&[("matte",60.0,0.0,100.0),("warmth",50.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_ivory","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.milk" {
        let parameters=static_filter_parameters(link,&[("matte",60.0,0.0,100.0),("cool",40.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_milk","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.bronze" {
        let parameters=static_filter_parameters(link,&[("tone",80.0,0.0,100.0),("contrast",30.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_bronze","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.cool-blue" {
        let parameters=static_filter_parameters(link,&[("chill",60.0,0.0,100.0),("contrast",30.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_cool_blue","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.fresh" {
        let parameters=static_filter_parameters(link,&[("pop",50.0,0.0,100.0),("brightness",40.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_fresh","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.vivid" {
        let parameters=static_filter_parameters(link,&[("colour",60.0,0.0,100.0),("punch",50.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_vivid","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.soft-light" {
        let parameters=static_filter_parameters(link,&[("lift",50.0,0.0,100.0),("warmth",40.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_soft_light","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.moody" {
        let parameters=static_filter_parameters(link,&[("depth",60.0,0.0,100.0),("warmth",50.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_moody","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.rain" {
        let parameters=static_filter_parameters(link,&[("haze",60.0,0.0,100.0),("chill",50.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_rain","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.umber" {
        let parameters=static_filter_parameters(link,&[("warmth",60.0,0.0,100.0),("soften",40.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_umber","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.fair" {
        let parameters=static_filter_parameters(link,&[("brighten",50.0,0.0,100.0),("even",50.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_fair","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.peach" {
        let parameters=static_filter_parameters(link,&[("glow",50.0,0.0,100.0),("softness",40.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_peach","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.polaroid" {
        let parameters=static_filter_parameters(link,&[("fade",60.0,0.0,100.0),("warmth",50.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_polaroid","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.vintage" {
        let parameters=static_filter_parameters(link,&[("age",60.0,0.0,100.0),("tint",50.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_vintage","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.cloudless-sky" {
        let parameters=static_filter_parameters(link,&[("sky",60.0,0.0,100.0),("greens",40.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_cloudless_sky","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.lagoon" {
        let parameters=static_filter_parameters(link,&[("water",60.0,0.0,100.0),("brightness",40.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_lagoon","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.golden-hour" {
        let parameters=static_filter_parameters(link,&[("glow",50.0,0.0,100.0),("warmth",4600.0,3500.0,6500.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_golden_hour","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.daydream" {
        let parameters=static_filter_parameters(link,&[("haze",50.0,0.0,100.0),("warmth",50.0,0.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_daydream","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.contrast-pop" {
        let parameters=static_filter_parameters(link,&[("contrast",1.25,1.0,2.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_contrast_pop","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.sharpen" {
        let parameters=static_filter_parameters(link,&[("amount",1.0,0.2,3.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_sharpen","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.vignette" {
        let parameters=static_filter_parameters(link,&[("strength",50.0,10.0,100.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_vignette","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.contrast" {
        ensure!(link.keys.keys().all(|key|key=="amount"),"unknown Concat contrast source channel");
        let concat_keys=link.keys.get("amount").filter(|keys|!keys.is_empty()).cloned();
        if let Some(keys)=&concat_keys{super::concat_contrast::validate_concat_keys(keys)?;}
        let mut static_link=link.clone();static_link.keys.clear();
        let parameters:super::concat_contrast::ConcatContrast=serde_json::from_value(static_filter_parameters(&static_link,&[("amount",0.,-100.,100.)])?)?;
        parameters.validate()?;
        return Ok(Some(super::composition::VideoEffect::ConcatContrast{parameters,animation:None,concat_keys}));
    }
    if link.id=="concat.exposure" {
        ensure!(link.keys.keys().all(|key|key=="stops"),"unknown Concat exposure source channel");
        let concat_keys=link.keys.get("stops").filter(|keys|!keys.is_empty()).cloned();
        if let Some(keys)=&concat_keys{super::concat_exposure::validate_concat_keys(keys)?;}
        let mut static_link=link.clone();static_link.keys.clear();
        let parameters:super::concat_exposure::ConcatExposure=serde_json::from_value(static_filter_parameters(&static_link,&[("stops",0.,-4.,4.)])?)?;
        parameters.validate()?;
        return Ok(Some(super::composition::VideoEffect::ConcatExposure{parameters,animation:None,concat_keys}));
    }
    if link.id=="concat.saturation" {
        ensure!(link.keys.keys().all(|key|key=="amount"),"unknown Concat saturation source channel");
        let concat_keys=link.keys.get("amount").filter(|keys|!keys.is_empty()).cloned();
        if let Some(keys)=&concat_keys{super::concat_chroma::validate_concat_keys(keys)?;}
        let mut static_link=link.clone();static_link.keys.clear();
        let parameters:super::concat_chroma::Saturation=serde_json::from_value(static_filter_parameters(&static_link,&[("amount",0.,-100.,100.)])?)?;
        parameters.validate()?;
        return Ok(Some(super::composition::VideoEffect::ConcatSaturation{parameters,animation:None,concat_keys}));
    }
    if link.id=="concat.vibrance" {
        let parameters=static_filter_parameters(link,&[("amount",0.7,0.1,2.0)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_vibrance","parameters":parameters}))?;
        effect.validate().map_err(anyhow::Error::msg)?;
        return Ok(Some(effect));
    }
    if link.id=="concat.white-balance" {
        super::concat_white_balance::validate_keys(&link.keys)?;
        let concat_keys=(!link.keys.is_empty()).then(||link.keys.clone());
        let mut static_link=link.clone();static_link.keys.clear();
        let parameters=static_filter_parameters(&static_link,&[("temperature",6500.,2000.,11000.),("tint",0.,-100.,100.)])?;
        let effect:super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_white_balance","parameters":parameters,"concatKeys":concat_keys}))?;
        effect.validate().map_err(anyhow::Error::msg)?;return Ok(Some(effect));
    }
    if link.id=="concat.color-wheels" {
        super::concat_color_wheels::validate_keys(&link.keys)?;
        let concat_keys=(!link.keys.is_empty()).then(||link.keys.clone());
        let mut static_link=link.clone();static_link.keys.clear();
        let mut fields=Vec::new();
        for wheel in ["lift","gamma","gain"]{fields.push((wheel,0.,-1.,1.));}
        fields.extend([("lift.x",0.,-1.,1.),("lift.y",0.,-1.,1.),("lift.m",0.,-1.,1.),("gamma.x",0.,-1.,1.),("gamma.y",0.,-1.,1.),("gamma.m",0.,-1.,1.),("gain.x",0.,-1.,1.),("gain.y",0.,-1.,1.),("gain.m",0.,-1.,1.)]);
        let values=static_filter_parameters(&static_link,&fields)?;
        let wheel=|name:&str| -> [f32;3] {std::array::from_fn(|i|values[format!("{name}.{}",["x","y","m"][i])].as_f64().unwrap() as f32)};
        // Original uniform packing ignores bare wheel keys and uses x/y/m only.
        let parameters=super::concat_color_wheels::ColorWheels{lift:wheel("lift"),gamma:wheel("gamma"),gain:wheel("gain")};parameters.validate()?;
        return Ok(Some(super::composition::VideoEffect::ConcatColorWheels{parameters,animation:None,concat_keys}));
    }
    if link.id=="concat.curves" {
        ensure!(link.params.keys().all(|key|source_curve_key(key)),"unknown Concat Curves parameter");
        ensure!(link.params.values().all(|value|value.is_finite()),"nonfinite Concat curve coordinate");
        ensure!(link.keys.values().all(Vec::is_empty),"Concat Curves source animation is not yet supported by native conversion");
        let parameters=super::concat_curves::Curves{luma:source_curve_points(link,"luma"),red:source_curve_points(link,"red"),green:source_curve_points(link,"green"),blue:source_curve_points(link,"blue")};parameters.validate()?;
        return Ok(Some(super::composition::VideoEffect::ConcatCurves{parameters}));
    }
    if link.id=="concat.adjust" {
        let fields=&[("temperature",6500.0,2000.0,11000.0),("tint",0.0,-100.0,100.0),("saturation",0.0,-100.0,100.0),("exposure",0.0,-3.0,3.0),("brightness",0.0,-100.0,100.0),("contrast",0.0,-100.0,100.0),("highlights",0.0,-100.0,100.0),("shadows",0.0,-100.0,100.0),("whites",0.0,-100.0,100.0),("blacks",0.0,-100.0,100.0),("sharpen",0.0,0.0,100.0),("fade",0.0,0.0,100.0),("vignette",0.0,0.0,100.0)];
        let wheel_key=|key:&str|{["lift","gamma","gain"].contains(&key)||key.split_once('.').is_some_and(|(root,axis)|["lift","gamma","gain"].contains(&root)&&matches!(axis,"x"|"y"|"m"))};
        ensure!(link.params.keys().all(|key|fields.iter().any(|field|field.0==key.as_str())||wheel_key(key)||source_curve_key(key)),"unknown Concat Adjust parameter");
        ensure!(link.params.values().all(|value|value.is_finite()),"nonfinite Concat Adjust parameter");
        super::concat_color_wheels::validate_keys(&link.keys)?;
        let concat_keys=(!link.keys.is_empty()).then(||link.keys.clone());
        let mut scalar_link=link.clone();scalar_link.keys.clear();scalar_link.params.retain(|key,_|fields.iter().any(|field|field.0==key.as_str()));
        let mut parameters=static_filter_parameters(&scalar_link,fields)?;
        let mut wheel_link=link.clone();wheel_link.keys.clear();wheel_link.id="concat.color-wheels".into();wheel_link.params.retain(|key,_|wheel_key(key));
        let Some(super::composition::VideoEffect::ConcatColorWheels{parameters:wheels,..})=native_effect(&wheel_link)?else{anyhow::bail!("missing native wheel conversion");};
        for(key,value)in [("lift",wheels.lift),("gamma",wheels.gamma),("gain",wheels.gain)]{parameters[key]=serde_json::json!(value);}
        for channel in ["luma","red","green","blue"]{parameters[channel]=serde_json::json!(source_curve_points(link,channel));}
        let parameters:super::concat_adjust::Adjust=serde_json::from_value(parameters)?;parameters.validate()?;
        return Ok(Some(super::composition::VideoEffect::ConcatAdjust{parameters,animation:None,concat_keys}));
    }
    let fixed_kind=match link.id.as_str(){
        "concat.autumn"=>"concat_autumn",
        "concat.bleach-bypass"=>"concat_bleach_bypass",
        "concat.classic-chrome"=>"concat_classic_chrome",
        "concat.cyberpunk"=>"concat_cyberpunk",
        "concat.delicious"=>"concat_delicious",
        "concat.dreamy"=>"concat_dreamy",
        "concat.high-contrast-mono"=>"concat_high_contrast_mono",
        "concat.landscape-pop"=>"concat_landscape_pop",
        "concat.natural"=>"concat_natural",
        "concat.noir"=>"concat_noir",
        "concat.retro-70s"=>"concat_retro_70s",
        "concat.rosy"=>"concat_rosy",
        "concat.sci-fi"=>"concat_sci_fi",
        "concat.silver"=>"concat_silver",
        "concat.tropical"=>"concat_tropical",
        "concat.vaporwave"=>"concat_vaporwave",
        "concat.winter-cool"=>"concat_winter_cool",
        _=>"",
    };
    if !fixed_kind.is_empty(){
        let parameters=static_filter_parameters(link,&[])?;
        return Ok(Some(serde_json::from_value(serde_json::json!({"kind":fixed_kind,"parameters":parameters}))?));
    }
    if link.id=="concat.mono" {
        let parameters:super::concat_timed_looks::Mono=serde_json::from_value(static_filter_parameters(link,&[("colour",40.,-100.,100.),("contrast",40.,0.,100.),("grain",0.,0.,100.)])?)?;
        parameters.validate()?;
        return Ok(Some(super::composition::VideoEffect::ConcatMono{parameters}));
    }
    if link.id=="concat.midnight" {
        let parameters:super::concat_timed_looks::Midnight=serde_json::from_value(static_filter_parameters(link,&[("depth",60.,0.,100.),("grain",25.,0.,100.)])?)?;
        parameters.validate()?;
        return Ok(Some(super::composition::VideoEffect::ConcatMidnight{parameters}));
    }
    if link.id=="concat.marquee" {
        let parameters:super::concat_marquee::Marquee=serde_json::from_value(static_filter_parameters(link,&[("lights",60.,0.,100.),("depth",40.,0.,100.)])?)?;
        parameters.validate()?;
        return Ok(Some(super::composition::VideoEffect::ConcatMarquee{parameters}));
    }
    if link.id=="concat.retro-cartoon" {
        let parameters:super::concat_retro_cartoon::RetroCartoon=serde_json::from_value(static_filter_parameters(link,&[("levels",5.,2.,8.),("ink",60.,0.,100.)])?)?;
        parameters.validate()?;
        return Ok(Some(super::composition::VideoEffect::ConcatRetroCartoon{parameters}));
    }
    if link.id=="concat.super-8" {
        let parameters:super::concat_super8::Super8=serde_json::from_value(static_filter_parameters(link,&[])?)?;
        
        return Ok(Some(super::composition::VideoEffect::ConcatSuper8{parameters}));
    }
    if link.id == "concat.vhs" {
        ensure!(link.params.keys().all(|key|matches!(key.as_str(),"bleed"|"lines")),"unknown concat.vhs parameter");
        ensure!(link.keys.values().all(Vec::is_empty),"concat.vhs source animation is not yet supported by native conversion");
        let first=link.params.get("bleed").copied().unwrap_or(60.);
        let second=link.params.get("lines").copied().unwrap_or(50.);
        ensure!([first,second].iter().all(|value|value.is_finite()&&(0. ..=100.).contains(value)),"concat.vhs parameter out of range");
        let parameters=super::concat_vhs::Vhs{bleed:first as f32,lines:second as f32};parameters.validate()?;
        return Ok(Some(super::composition::VideoEffect::ConcatVhs{parameters}));
    }
    if link.id == "concat.falling-dust" {
        ensure!(link.params.keys().all(|key|matches!(key.as_str(),"dust"|"leak")),"unknown concat.falling-dust parameter");
        ensure!(link.keys.values().all(Vec::is_empty),"concat.falling-dust source animation is not yet supported by native conversion");
        let first=link.params.get("dust").copied().unwrap_or(60.);
        let second=link.params.get("leak").copied().unwrap_or(50.);
        ensure!([first,second].iter().all(|value|value.is_finite()&&(0. ..=100.).contains(value)),"concat.falling-dust parameter out of range");
        let parameters=super::concat_falling_dust::FallingDust{dust:first as f32,leak:second as f32};parameters.validate()?;
        return Ok(Some(super::composition::VideoEffect::ConcatFallingDust{parameters}));
    }
    if link.id == "concat.film" {
        ensure!(link.params.keys().all(|key| matches!(key.as_str(), "grain" | "warmth")), "unknown Concat Film parameter");
        ensure!(link.keys.values().all(Vec::is_empty), "Concat Film source animation is not yet supported by native conversion");
        let grain=link.params.get("grain").copied().unwrap_or(40.);
        let warmth=link.params.get("warmth").copied().unwrap_or(50.);
        ensure!([grain,warmth].iter().all(|value|value.is_finite()&&(0. ..=100.).contains(value)), "Concat Film parameter out of range");
        let parameters=super::concat_film_camera_looks::Film{grain:grain as f32,warmth:warmth as f32};
        parameters.validate()?;
        return Ok(Some(super::composition::VideoEffect::ConcatFilm{parameters}));
    }
    if link.id == "concat.digicam" {
        ensure!(link.params.keys().all(|key| matches!(key.as_str(), "flash" | "noise")), "unknown Concat Digicam parameter");
        ensure!(link.keys.values().all(Vec::is_empty), "Concat Digicam source animation is not yet supported by native conversion");
        let flash=link.params.get("flash").copied().unwrap_or(60.);
        let noise=link.params.get("noise").copied().unwrap_or(40.);
        ensure!([flash,noise].iter().all(|value|value.is_finite()&&(0. ..=100.).contains(value)), "Concat Digicam parameter out of range");
        let parameters=super::concat_film_camera_looks::Digicam{flash:flash as f32,noise:noise as f32};
        parameters.validate()?;
        return Ok(Some(super::composition::VideoEffect::ConcatDigicam{parameters}));
    }
    if matches!(link.id.as_str(), "concat.motion-blur" | "motion-blur") {
        use super::concat_motion_blur::{Channel, MotionBlur, OriginalKeys};
        let defaults = MotionBlur::default();
        let parameters = MotionBlur {
            length: link
                .params
                .get("length")
                .copied()
                .unwrap_or(defaults.length as f64) as f32,
            angle: link
                .params
                .get("angle")
                .copied()
                .unwrap_or(defaults.angle as f64) as f32,
        };
        parameters.validate()?;
        let keys: OriginalKeys = link
            .keys
            .iter()
            .filter_map(|(name, run)| {
                Channel::from_name(name)
                    .filter(|_| !run.is_empty())
                    .map(|channel| (channel, run.clone()))
            })
            .collect();
        super::concat_motion_blur::validate_keys(&keys)?;
        return Ok(Some(super::composition::VideoEffect::ConcatMotionBlur {
            parameters,
            concat_keys: (!keys.is_empty()).then_some(keys),
        }));
    }
    if link.id == "concat.bloom-pulse" {
        use super::concat_bloom_pulse::{Channel, BloomPulse, OriginalKeys};
        let defaults = BloomPulse::default();
        let parameters = BloomPulse {
            amount: link
                .params
                .get("amount")
                .copied()
                .unwrap_or(defaults.amount as f64) as f32,
            speed: link
                .params
                .get("speed")
                .copied()
                .unwrap_or(defaults.speed as f64) as f32,
        };
        parameters.validate()?;
        let keys: OriginalKeys = link
            .keys
            .iter()
            .filter_map(|(name, run)| {
                Channel::from_name(name)
                    .filter(|_| !run.is_empty())
                    .map(|channel| (channel, run.clone()))
            })
            .collect();
        super::concat_bloom_pulse::validate_keys(&keys)?;
        return Ok(Some(super::composition::VideoEffect::ConcatBloomPulse {
            parameters,
            concat_keys: (!keys.is_empty()).then_some(keys),
        }));
    }
    if link.id == "concat.zoom-blur" {
        use super::concat_zoom_blur::{Channel, OriginalKeys, ZoomBlur};
        let defaults = ZoomBlur::default();
        let parameters = ZoomBlur {
            amount: link
                .params
                .get("amount")
                .copied()
                .unwrap_or(defaults.amount as f64) as f32,
            x: link.params.get("x").copied().unwrap_or(defaults.x as f64) as f32,
            y: link.params.get("y").copied().unwrap_or(defaults.y as f64) as f32,
        };
        parameters.validate()?;
        let keys: OriginalKeys = link
            .keys
            .iter()
            .filter_map(|(name, run)| {
                Channel::from_name(name)
                    .filter(|_| !run.is_empty())
                    .map(|channel| (channel, run.clone()))
            })
            .collect();
        super::concat_zoom_blur::validate_keys(&keys)?;
        return Ok(Some(super::composition::VideoEffect::ConcatZoomBlur {
            parameters,
            concat_keys: (!keys.is_empty()).then_some(keys),
        }));
    }
    if matches!(link.id.as_str(), "concat.film-grain" | "film-grain") {
        let parameters = super::concat_film_grain::FilmGrain {
            amount: link.params.get("amount").copied().unwrap_or(12.) as f32,
        };
        parameters.validate()?;
        let concat_keys = link
            .keys
            .get("amount")
            .filter(|run| !run.is_empty())
            .cloned();
        if let Some(keys) = &concat_keys {
            super::concat_film_grain::validate_concat_keys(keys)?;
        }
        return Ok(Some(super::composition::VideoEffect::ConcatFilmGrain {
            parameters,
            concat_keys,
        }));
    }
    if matches!(link.id.as_str(), "concat.glow" | "glow") {
        let parameters = super::concat_glow::Glow {
            amount: link.params.get("amount").copied().unwrap_or(45.) as f32,
        };
        parameters.validate()?;
        let concat_keys = link
            .keys
            .get("amount")
            .filter(|run| !run.is_empty())
            .cloned();
        if let Some(keys) = &concat_keys {
            super::concat_glow::validate_concat_keys(keys)?;
        }
        return Ok(Some(super::composition::VideoEffect::ConcatGlow {
            parameters,
            concat_keys,
        }));
    }
    if link.id == "concat.halftone" {
        let parameters = super::concat_halftone::Halftone {
            size: link.params.get("size").copied().unwrap_or(6.) as f32,
        };
        parameters.validate()?;
        let concat_keys = link.keys.get("size").filter(|run| !run.is_empty()).cloned();
        if let Some(keys) = &concat_keys {
            super::concat_halftone::validate_concat_keys(keys)?;
        }
        return Ok(Some(super::composition::VideoEffect::ConcatHalftone {
            parameters,
            concat_keys,
        }));
    }
    if matches!(link.id.as_str(), "concat.fisheye" | "fisheye") {
        let parameters = super::concat_fisheye::Fisheye {
            strength: link.params.get("strength").copied().unwrap_or(50.) as f32,
        };
        parameters.validate()?;
        let concat_keys = link
            .keys
            .get("strength")
            .filter(|keys| !keys.is_empty())
            .cloned();
        if let Some(keys) = &concat_keys {
            super::concat_fisheye::validate_concat_keys(keys)?;
        }
        return Ok(Some(super::composition::VideoEffect::ConcatFisheye {
            parameters,
            concat_keys,
        }));
    }
    if matches!(
        link.id.as_str(),
        "concat.chromatic-aberration" | "chromatic-aberration"
    ) {
        let parameters = super::concat_chromatic_aberration::ChromaticAberration {
            shift: link.params.get("shift").copied().unwrap_or(4.) as f32,
        };
        parameters.validate()?;
        let concat_keys = link
            .keys
            .get("shift")
            .filter(|keys| !keys.is_empty())
            .cloned();
        if let Some(keys) = &concat_keys {
            super::concat_chromatic_aberration::validate_concat_keys(keys)?;
        }
        return Ok(Some(
            super::composition::VideoEffect::ConcatChromaticAberration {
                parameters,
                concat_keys,
            },
        ));
    }
    if link.id != "concat.chroma-key" {
        return native_gaussian_effect(link);
    }
    use super::concat_chroma_key::{Channel, ChromaKey, OriginalKeys};
    let defaults = ChromaKey::default();
    let parameters = ChromaKey {
        color: link
            .params
            .get("color")
            .map(|v| v.max(0.) as u32)
            .unwrap_or(defaults.color),
        similarity: link
            .params
            .get("similarity")
            .copied()
            .unwrap_or(defaults.similarity as f64) as f32,
        softness: link
            .params
            .get("softness")
            .copied()
            .unwrap_or(defaults.softness as f64) as f32,
        spill: link
            .params
            .get("spill")
            .copied()
            .unwrap_or(defaults.spill as f64) as f32,
    };
    parameters.validate()?;
    let keys: OriginalKeys = link
        .keys
        .iter()
        .filter_map(|(name, run)| {
            Channel::from_name(name)
                .filter(|_| !run.is_empty())
                .map(|channel| (channel, run.clone()))
        })
        .collect();
    super::concat_chroma_key::validate_keys(&keys)?;
    Ok(Some(super::composition::VideoEffect::ConcatChromaKey {
        parameters,
        concat_keys: (!keys.is_empty()).then_some(keys),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_bypass_and_key_easing_with_range_clamping() {
        let link:AppliedFilter=serde_json::from_value(serde_json::json!({"id":"green-screen","enabled":false,"params":{"similarity":40},"keys":{"similarity":[{"at":0.7,"value":100,"ease":"inOut"},{"at":0.2,"value":-5,"ease":[-1,-2,2,3]}]}})).unwrap();
        let result = upgrade(link).unwrap();
        assert!(result.upgraded);
        assert!(!result.link.enabled);
        assert_eq!(result.link.params["similarity"], 60.);
        assert_eq!(result.link.params["softness"], 20.);
        assert_eq!(result.link.params["color"], 0x00ff00ffu32 as f64);
        let k = &result.link.keys["similarity"];
        assert_eq!(k[0].at, 0.7);
        assert_eq!(k[0].value, 60.);
        assert_eq!(k[0].ease, [0.42, 0., 0.58, 1.]);
        assert_eq!(k[1].value, 1.);
        assert_eq!(k[1].ease, [0., -2., 1., 3.]);
    }
    #[test]
    fn gaussian_mapping_defaults_unknown_and_invalid_domain() {
        let link = |id: &str, params: Value| -> AppliedFilter {
            serde_json::from_value(serde_json::json!({"id":id,"params":params})).unwrap()
        };
        assert_eq!(
            upgrade(link("box-blur", serde_json::json!({})))
                .unwrap()
                .link
                .params["radius"],
            14f64.sqrt()
        );
        assert_eq!(
            upgrade(link("box-blur", serde_json::json!({"radius":0})))
                .unwrap()
                .link
                .params["radius"],
            1.
        );
        assert_eq!(
            upgrade(link("box-blur", serde_json::json!({"radius":1000})))
                .unwrap()
                .link
                .params["radius"],
            50.
        );
        let unknown = link("custom.filter", serde_json::json!({"amount":7}));
        assert_eq!(upgrade(unknown.clone()).unwrap().link, unknown);
        assert!(upgrade(link("box-blur", serde_json::json!({"radius":-0.5}))).is_err());
    }
    #[test]
    #[ignore = "requires independent CONCAT_EFFECT_UPGRADE_ORACLE"]
    fn original_catalogue_links_match() {
        let path = std::env::var("CONCAT_EFFECT_UPGRADE_ORACLE")
            .expect("set CONCAT_EFFECT_UPGRADE_ORACLE");
        let data: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            data["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        let cases = data["cases"].as_array().unwrap();
        for (index, c) in cases.iter().enumerate() {
            let got = upgrade(serde_json::from_value(c["input"].clone()).unwrap()).unwrap();
            assert_eq!(
                serde_json::to_value(got).unwrap(),
                c["expected"],
                "case{index}"
            );
        }
        eprintln!(
            "original Concat catalogue migration: {} exact links",
            cases.len()
        );
    }
}

/// Placement-free transition descriptor. The caller supplies its actual clip IDs
/// and explicit transition interval; this converter never invents timeline data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeTransition {
    pub kind: super::composition::TransitionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel: Option<super::transition_spec::TransitionSpec>,
}
pub fn native_transition(link: &AppliedFilter) -> Result<Option<NativeTransition>> {
    use super::composition::TransitionKind;
    if !link.enabled {return Ok(None);}
    let kind=match link.id.as_str(){
        "concat.dissolve"=>TransitionKind::ConcatDissolve,
        "concat.fade-black"=>TransitionKind::ConcatFadeBlack,
        "concat.fade-white"=>TransitionKind::ConcatFadeWhite,
        "concat.glitch-warp"|"concat.glitch-transition"|"concat.cube"|"concat.film-burn"|"concat.zoom-blur-transition"|"concat.light-leak-transition"|"concat.light-leak-flash"|"concat.zoom-transition"|"concat.whip-pan"|"concat.blur-dissolve"|"concat.linear-wipe"|"concat.clock-wipe"|"concat.iris"|"concat.shape-wipe"|"concat.push"|"concat.slide"|"concat.rgb-split-wipe"|"concat.spin"|"concat.swirl"=>TransitionKind::Procedural,
        _=>return Ok(None),
    };
    ensure!(link.keys.values().all(Vec::is_empty),"Concat transition parameter animation is not yet supported by native conversion");
    let kernel=if link.id=="concat.glitch-warp" {
        ensure!(link.params.keys().all(|k|k=="amount"),"unknown Concat Glitch Warp parameter");
        let amount=link.params.get("amount").copied().unwrap_or(75.);
        let spec=super::transition_spec::TransitionSpec{effect:link.id.clone(),params:BTreeMap::from([("amount".into(),serde_json::json!({"Float":amount}))])};spec.validate().map_err(anyhow::Error::msg)?;Some(spec)
    }else if link.id=="concat.glitch-transition" {
        ensure!(link.params.keys().all(|k|k=="amount"),"unknown Concat Glitch parameter");
        let amount=link.params.get("amount").copied().unwrap_or(70.);
        let spec=super::transition_spec::TransitionSpec{effect:link.id.clone(),params:BTreeMap::from([("amount".into(),serde_json::json!({"Float":amount}))])};spec.validate().map_err(anyhow::Error::msg)?;Some(spec)
    }else if link.id=="concat.cube" {
        ensure!(link.params.is_empty(),"Concat Cube takes no parameters");
        Some(super::transition_spec::TransitionSpec{effect:link.id.clone(),params:BTreeMap::new()})
    }else if link.id=="concat.film-burn" {
        ensure!(link.params.keys().all(|k|k=="heat"),"unknown Concat Film Burn parameter");
        let heat=link.params.get("heat").copied().unwrap_or(70.);
        let spec=super::transition_spec::TransitionSpec{effect:link.id.clone(),params:BTreeMap::from([("heat".into(),serde_json::json!({"Float":heat}))])};spec.validate().map_err(anyhow::Error::msg)?;Some(spec)
    }else if link.id=="concat.zoom-blur-transition" {
        ensure!(link.params.keys().all(|k|k=="strength"),"unknown Concat Zoom Blur parameter");
        let strength=link.params.get("strength").copied().unwrap_or(70.);
        let spec=super::transition_spec::TransitionSpec{effect:link.id.clone(),params:BTreeMap::from([("strength".into(),serde_json::json!({"Float":strength}))])};spec.validate().map_err(anyhow::Error::msg)?;Some(spec)
    }else if link.id=="concat.light-leak-transition" {
        ensure!(link.params.keys().all(|k|k=="warmth"),"unknown Concat Light Leak parameter");
        let warmth=link.params.get("warmth").copied().unwrap_or(70.);
        let spec=super::transition_spec::TransitionSpec{effect:link.id.clone(),params:BTreeMap::from([("warmth".into(),serde_json::json!({"Float":warmth}))])};spec.validate().map_err(anyhow::Error::msg)?;Some(spec)
    }else if link.id=="concat.light-leak-flash" {
        ensure!(link.params.keys().all(|k|k=="strength"),"unknown Concat Light Leak Flash parameter");
        let strength=link.params.get("strength").copied().unwrap_or(80.);
        let spec=super::transition_spec::TransitionSpec{effect:link.id.clone(),params:BTreeMap::from([("strength".into(),serde_json::json!({"Float":strength}))])};spec.validate().map_err(anyhow::Error::msg)?;Some(spec)
    }else if link.id=="concat.zoom-transition" {
        ensure!(link.params.keys().all(|k|k=="direction"),"unknown Concat Zoom parameter");
        let direction=link.params.get("direction").copied().unwrap_or(0.);ensure!(direction.is_finite()&&(0. ..=1.).contains(&direction),"Concat Zoom direction out of range");
        let choice=if (direction as f32)<0.5{0}else{1};
        let spec=super::transition_spec::TransitionSpec{effect:link.id.clone(),params:BTreeMap::from([("direction".into(),serde_json::json!({"Choice":choice}))])};spec.validate().map_err(anyhow::Error::msg)?;Some(spec)
    }else if link.id=="concat.whip-pan" {
        ensure!(link.params.keys().all(|k|k=="speed"),"unknown Concat Whip Pan parameter");
        let speed=link.params.get("speed").copied().unwrap_or(80.);
        let spec=super::transition_spec::TransitionSpec{effect:link.id.clone(),params:BTreeMap::from([("speed".into(),serde_json::json!({"Float":speed}))])};spec.validate().map_err(anyhow::Error::msg)?;Some(spec)
    }else if link.id=="concat.blur-dissolve" {
        ensure!(link.params.keys().all(|k|k=="strength"),"unknown Concat Blur Dissolve parameter");
        let strength=link.params.get("strength").copied().unwrap_or(60.);
        let spec=super::transition_spec::TransitionSpec{effect:link.id.clone(),params:BTreeMap::from([("strength".into(),serde_json::json!({"Float":strength}))])};spec.validate().map_err(anyhow::Error::msg)?;Some(spec)
    }else if matches!(link.id.as_str(),"concat.spin"|"concat.swirl") {
        let(key,default)=if link.id=="concat.spin"{("turns",1.)}else{("twist",70.)};
        ensure!(link.params.keys().all(|k|k==key),"unknown Concat rotation parameter");let value=link.params.get(key).copied().unwrap_or(default);
        let spec=super::transition_spec::TransitionSpec{effect:link.id.clone(),params:BTreeMap::from([(key.into(),serde_json::json!({"Float":value}))])};spec.validate().map_err(anyhow::Error::msg)?;Some(spec)
    }else if link.id=="concat.rgb-split-wipe" {
        ensure!(link.params.keys().all(|k|k=="spread"),"unknown Concat RGB Split parameter");
        let spread=link.params.get("spread").copied().unwrap_or(60.);
        let spec=super::transition_spec::TransitionSpec{effect:link.id.clone(),params:BTreeMap::from([("spread".into(),serde_json::json!({"Float":spread}))])};spec.validate().map_err(anyhow::Error::msg)?;Some(spec)
    }else if matches!(link.id.as_str(),"concat.push"|"concat.slide") {
        ensure!(link.params.keys().all(|k|k=="direction"),"unknown Concat movement parameter");
        let direction=link.params.get("direction").copied().unwrap_or(0.);ensure!(direction.is_finite()&&(0. ..=3.).contains(&direction),"Concat direction out of range");
        let direction=direction as f32;let choice=if direction<0.5{0}else if direction<1.5{1}else if direction<2.5{2}else{3};
        let spec=super::transition_spec::TransitionSpec{effect:link.id.clone(),params:BTreeMap::from([("direction".into(),serde_json::json!({"Choice":choice}))])};spec.validate().map_err(anyhow::Error::msg)?;Some(spec)
    }else if link.id=="concat.shape-wipe" {
        ensure!(link.params.keys().all(|k|k=="shape"||k=="softness"),"unknown Concat Shape Wipe parameter");
        let shape=link.params.get("shape").copied().unwrap_or(0.);ensure!(shape.is_finite()&&(0. ..=2.).contains(&shape),"Concat shape out of range");
        let shape=shape as f32;let choice=if shape<0.5{0}else if shape<1.5{1}else{2};
        let softness=link.params.get("softness").copied().unwrap_or(15.);
        let spec=super::transition_spec::TransitionSpec{effect:link.id.clone(),params:BTreeMap::from([("shape".into(),serde_json::json!({"Choice":choice})),("softness".into(),serde_json::json!({"Float":softness}))])};spec.validate().map_err(anyhow::Error::msg)?;Some(spec)
    }else if link.id=="concat.iris" {
        ensure!(link.params.keys().all(|k|k=="softness"),"unknown Concat Iris parameter");
        let softness=link.params.get("softness").copied().unwrap_or(15.);
        let spec=super::transition_spec::TransitionSpec{effect:link.id.clone(),params:BTreeMap::from([("softness".into(),serde_json::json!({"Float":softness}))])};
        spec.validate().map_err(anyhow::Error::msg)?;Some(spec)
    }else if link.id=="concat.clock-wipe" {
        ensure!(link.params.is_empty(),"Concat Clock Wipe takes no parameters");
        Some(super::transition_spec::TransitionSpec{effect:link.id.clone(),params:BTreeMap::new()})
    }else if kind==TransitionKind::Procedural {
        ensure!(link.params.keys().all(|k|k=="direction"||k=="softness"),"unknown Concat linear wipe parameter");
        let direction=link.params.get("direction").copied().unwrap_or(0.);
        let softness=link.params.get("softness").copied().unwrap_or(20.);
        ensure!(direction.is_finite()&&(0. ..=3.).contains(&direction)&&softness.is_finite()&&(0. ..=100.).contains(&softness),"Concat linear wipe parameter out of range");
        let direction=direction as f32;
        let choice=if direction<0.5{0}else if direction<1.5{1}else if direction<2.5{2}else{3};
        let spec=super::transition_spec::TransitionSpec{effect:"concat.linear-wipe".into(),params:BTreeMap::from([("direction".into(),serde_json::json!({"Choice":choice})),("softness".into(),serde_json::json!({"Float":softness}))])};
        spec.validate().map_err(anyhow::Error::msg)?;
        Some(spec)
    }else{
        ensure!(link.params.is_empty(),"unexpected parameter for parameterless Concat transition");
        None
    };
    Ok(Some(NativeTransition{kind,kernel}))
}


/// Opaque disabled source links never execute shader/package code.
pub(crate) fn validate_bypassed(link:&AppliedFilter)->Result<()>{
 ensure!(!link.enabled&&!link.id.is_empty()&&link.id.len()<=128&&link.params.len()<=128&&link.keys.len()<=128,"invalid disabled source effect");
 ensure!(link.params.values().all(|value|value.is_finite()),"nonfinite disabled effect parameter");
 let mut count=0usize;
 for run in link.keys.values(){count=count.checked_add(run.len()).ok_or_else(||anyhow::anyhow!("disabled effect key overflow"))?;ensure!(run.len()<=4096&&count<=8192,"disabled effect key budget exceeded");ensure!(run.iter().all(|key|key.at.is_finite()&&(0.0..=1.0).contains(&key.at)&&key.value.is_finite()&&key.ease.iter().all(|value|value.is_finite())),"invalid disabled effect key");}
 Ok(())
}
