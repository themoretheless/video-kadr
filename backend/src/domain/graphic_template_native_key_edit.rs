//! Edit existing host animation channels without changing authored curves or handles.
use anyhow::{ensure, Result};
use serde_json::{json, Value};

fn binding(channel: &str) -> (String, usize) {
    for group in ["fill", "stroke2", "stroke", "background", "shadow"] {
        if let Some(axis)=channel.strip_prefix(group) {
            if let Some(index)=["R","G","B","A"].iter().position(|candidate|*candidate==axis) {
                return (format!("{group}_color"),index);
            }
        }
    }
    for group in ["position","anchor","size"] {
        if let Some(axis)=channel.strip_prefix(group) {
            if let Some(index)=["X","Y"].iter().position(|candidate|*candidate==axis) {return (group.into(),index)}
        }
    }
    let mut snake=String::new();
    for character in channel.chars() {if character.is_ascii_uppercase(){snake.push('_');snake.push(character.to_ascii_lowercase())}else{snake.push(character)}}
    (snake,0)
}

pub fn edit_existing(content: &mut Value, param: &str, value: &Value, local_us: i128) -> Result<usize> {
    let Some(animation)=content.get_mut("animation").and_then(Value::as_object_mut) else {return Ok(0)};
    let mut changed=0;
    for (channel,curve) in animation {
        let (field,index)=binding(channel);
        if field!=param {continue}
        let component=if value.is_array(){value.get(index)}else{Some(value)}
            .and_then(Value::as_f64).ok_or_else(||anyhow::anyhow!("invalid animated control value"))?;
        ensure!(component.is_finite(),"non-finite animated value");
        match curve["mode"].as_str() {
            Some("constant")=>curve["value"]=json!(component),
            Some("keyframes")=>{
                let track=&mut curve["track"];
                let base=track["timeBase"].as_u64().filter(|base|*base>0).ok_or_else(||anyhow::anyhow!("invalid animation time base"))?;
                let scaled=local_us.checked_mul(i128::from(base)).ok_or_else(||anyhow::anyhow!("animation clock overflow"))?;
                let rounded=scaled/1_000_000+if (scaled%1_000_000).abs()>=500_000 {scaled.signum()}else{0};
                let tick=i64::try_from(rounded).map_err(|_|anyhow::anyhow!("animation key time overflow"))?;
                let keys=track["keyframes"].as_array_mut().ok_or_else(||anyhow::anyhow!("invalid animation keys"))?;
                let ticks=keys.iter().map(|key|key["tick"].as_i64().ok_or_else(||anyhow::anyhow!("invalid key time"))).collect::<Result<Vec<_>>>()?;
                ensure!(!ticks.is_empty()&&ticks.windows(2).all(|pair|pair[0]<pair[1]),"animation keys must increase");
                match ticks.binary_search(&tick) {
                    Ok(index)=>keys[index]["value"]=json!(component),
                    Err(index)=>keys.insert(index,json!({"tick":tick,"value":component})),
                }
            },
            _=>anyhow::bail!("invalid animation mode"),
        }
        changed+=1;
    }
    Ok(changed)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_edit_keeps_track_metadata_and_existing_key_handles() {
        let mut content=json!({"animation":{"positionX":{"mode":"keyframes","track":{"timeBase":3,"interpolation":"linear","extrapolation":{"before":"hold","after":"hold"},"keyframes":[{"tick":-1,"value":10,"interpolation":"hold","tangents":{"sentinel":true}},{"tick":4,"value":30}]}},"positionY":{"mode":"constant","value":20},"opacity":{"mode":"constant","value":50}}});
        let original=content.clone();
        assert_eq!(edit_existing(&mut content,"position",&json!([12,22]),-333333).unwrap(),2);
        assert_eq!(content["animation"]["positionX"]["track"]["keyframes"][0],json!({"tick":-1,"value":12.0,"interpolation":"hold","tangents":{"sentinel":true}}));
        assert_eq!(content["animation"]["positionY"]["value"],22.0);
        assert_eq!(content["animation"]["opacity"],original["animation"]["opacity"]);
        assert_eq!(edit_existing(&mut content,"position",&json!([14,24]),500000).unwrap(),2);
        assert_eq!(content["animation"]["positionX"]["track"]["keyframes"][1],json!({"tick":2,"value":14.0}));
        assert_eq!(content["animation"]["positionX"]["track"]["extrapolation"],original["animation"]["positionX"]["track"]["extrapolation"]);
        assert!(edit_existing(&mut content,"position",&json!([1,2]),i128::MAX).is_err());
    }
}
