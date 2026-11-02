//! Explicit typed siblings of one source Concat lane.
use super::composition::{Composition, CompositionTrack};
use anyhow::{ensure, Result};
use std::collections::BTreeSet;
pub fn groups(content: &Composition) -> Result<Vec<(usize, usize)>> {
    let mut used = BTreeSet::new();
    let mut groups = vec![];
    if let Some(metadata) = &content.metadata {
        for (owner, members) in &metadata.concat_lane_groups {
            ensure!(
                members.len() == 2 && members.first() == Some(owner) && members[0] != members[1],
                "invalid Concat lane group identity"
            );
            let video = content
                .tracks
                .iter()
                .position(|track| track.id() == owner)
                .ok_or_else(|| anyhow::anyhow!("Concat video lane missing"))?;
            let image = content
                .tracks
                .iter()
                .position(|track| track.id() == &members[1])
                .ok_or_else(|| anyhow::anyhow!("Concat image lane missing"))?;
            ensure!(
                image + 1 == video && used.insert(image) && used.insert(video),
                "Concat lane group order or membership changed"
            );
            match (&content.tracks[video], &content.tracks[image]) {
                (
                    CompositionTrack::Video {
                        hidden: a,
                        locked: x,
                        ..
                    },
                    CompositionTrack::Image {
                        hidden: b,
                        locked: y,
                        ..
                    },
                ) => ensure!(
                    a == b && x == y,
                    "Concat lane group visibility/lock differs"
                ),
                _ => anyhow::bail!("Concat lane group must contain video and image tracks"),
            }
            groups.push((video, image));
        }
    }
    Ok(groups)
}
