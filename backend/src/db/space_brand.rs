use std::collections::HashSet;

use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use super::Db;
use crate::library::now_secs;

pub const MAX_BRAND_COLORS: usize = 32;
pub const MAX_BRAND_FONTS: usize = 4;
pub const MAX_BRAND_LOGOS: usize = 16;
const ALLOWED_FONTS: [&str; 4] = ["Noto Sans", "Arial Unicode MS", "DejaVu Sans", "Arial"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrandColor {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrandKitPayload {
    #[serde(default)]
    pub colors: Vec<BrandColor>,
    #[serde(default)]
    pub fonts: Vec<String>,
    #[serde(default)]
    pub logo_source_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceBrandKit {
    pub space_id: String,
    pub kit: BrandKitPayload,
    pub revision: u64,
    pub updated_by: Option<String>,
    pub updated_at: Option<u64>,
}

pub(super) async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::query("CREATE TABLE IF NOT EXISTS collaboration_space_brand_kits (space_id TEXT PRIMARY KEY, kit_json TEXT NOT NULL, revision INTEGER NOT NULL, updated_by TEXT NOT NULL, updated_at INTEGER NOT NULL, FOREIGN KEY(space_id) REFERENCES collaboration_spaces(id) ON DELETE CASCADE);")
        .execute(pool).await?;
    Ok(())
}

impl Db {
    pub async fn get_space_brand_kit_for(
        &self,
        space_id: &str,
        actor: &str,
    ) -> Result<SpaceBrandKit> {
        ensure!(
            self.is_space_member(space_id, actor).await?,
            "actor is not a space member"
        );
        let row = sqlx::query("SELECT kit_json, revision, updated_by, updated_at FROM collaboration_space_brand_kits WHERE space_id = ?")
            .bind(space_id).fetch_optional(&self.pool).await?;
        match row {
            Some(row) => Ok(SpaceBrandKit {
                space_id: space_id.into(),
                kit: serde_json::from_str(&row.try_get::<String, _>("kit_json")?)?,
                revision: u64::try_from(row.try_get::<i64, _>("revision")?)?,
                updated_by: Some(row.try_get("updated_by")?),
                updated_at: Some(u64::try_from(row.try_get::<i64, _>("updated_at")?)?),
            }),
            None => Ok(SpaceBrandKit {
                space_id: space_id.into(),
                kit: BrandKitPayload::default(),
                revision: 0,
                updated_by: None,
                updated_at: None,
            }),
        }
    }

    pub async fn update_space_brand_kit(
        &self,
        space_id: &str,
        actor: &str,
        base_revision: u64,
        kit: BrandKitPayload,
    ) -> Result<Option<SpaceBrandKit>> {
        if !self.can_edit_space(space_id, actor).await? {
            return Ok(None);
        }
        validate_brand_kit(&kit)?;
        let owned = self.space_source_ids(space_id).await?;
        ensure!(
            kit.logo_source_ids.iter().all(|id| owned.contains(id)),
            "brand logo is not owned by the space"
        );
        let kit_json = serde_json::to_string(&kit)?;
        let now = now_secs();
        let result = sqlx::query("INSERT INTO collaboration_space_brand_kits (space_id, kit_json, revision, updated_by, updated_at) SELECT ?, ?, 1, ?, ? WHERE ? = 0 ON CONFLICT(space_id) DO UPDATE SET kit_json = excluded.kit_json, revision = collaboration_space_brand_kits.revision + 1, updated_by = excluded.updated_by, updated_at = excluded.updated_at WHERE collaboration_space_brand_kits.revision = ?")
            .bind(space_id).bind(kit_json).bind(actor).bind(i64::try_from(now)?)
            .bind(i64::try_from(base_revision)?).bind(i64::try_from(base_revision)?)
            .execute(&self.pool).await?;
        if result.rows_affected() == 0 {
            anyhow::bail!("space brand revision conflict");
        }
        Ok(Some(SpaceBrandKit {
            space_id: space_id.into(),
            kit,
            revision: base_revision + 1,
            updated_by: Some(actor.into()),
            updated_at: Some(now),
        }))
    }
}

fn validate_brand_kit(kit: &BrandKitPayload) -> Result<()> {
    ensure!(
        kit.colors.len() <= MAX_BRAND_COLORS,
        "brand color limit exceeded"
    );
    ensure!(
        kit.fonts.len() <= MAX_BRAND_FONTS,
        "brand font limit exceeded"
    );
    ensure!(
        kit.logo_source_ids.len() <= MAX_BRAND_LOGOS,
        "brand logo limit exceeded"
    );
    let unique_colors: HashSet<_> = kit.colors.iter().map(|color| color.name.as_str()).collect();
    let unique_fonts: HashSet<_> = kit.fonts.iter().map(String::as_str).collect();
    let unique_logos: HashSet<_> = kit.logo_source_ids.iter().map(String::as_str).collect();
    ensure!(
        unique_colors.len() == kit.colors.len(),
        "duplicate brand color"
    );
    ensure!(
        unique_fonts.len() == kit.fonts.len(),
        "duplicate brand font"
    );
    ensure!(
        unique_logos.len() == kit.logo_source_ids.len(),
        "duplicate brand logo"
    );
    for color in &kit.colors {
        ensure!(
            !color.name.trim().is_empty()
                && color.name.len() <= 64
                && matches!(color.value.len(), 7 | 9)
                && color.value.starts_with('#')
                && color.value[1..]
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit()),
            "invalid brand color"
        );
    }
    ensure!(
        kit.fonts
            .iter()
            .all(|font| ALLOWED_FONTS.contains(&font.as_str())),
        "unsupported brand font"
    );
    ensure!(
        kit.logo_source_ids.iter().all(|id| !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))),
        "invalid brand logo"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::SpaceRole;

    #[tokio::test]
    async fn brand_kit_is_member_visible_editor_writable_and_revision_protected() {
        let directory = tempfile::tempdir().unwrap();
        let db = Db::open(directory.path()).await.unwrap();
        let space = db.create_space("owner", "Studio").await.unwrap();
        db.set_space_member(&space.id, "owner", "editor", SpaceRole::Editor)
            .await
            .unwrap();
        db.set_space_member(&space.id, "owner", "viewer", SpaceRole::Viewer)
            .await
            .unwrap();
        sqlx::query("INSERT INTO collaboration_space_media (space_id, source_id, created_by, created_at) VALUES (?, 'logo-source', 'owner', 1)")
            .bind(&space.id)
            .execute(&db.pool)
            .await
            .unwrap();
        let kit = BrandKitPayload {
            colors: vec![BrandColor {
                name: "Primary".into(),
                value: "#3366ff".into(),
            }],
            fonts: vec!["Noto Sans".into()],
            logo_source_ids: vec!["logo-source".into()],
        };

        assert!(db
            .update_space_brand_kit(&space.id, "viewer", 0, kit.clone())
            .await
            .unwrap()
            .is_none());
        let created = db
            .update_space_brand_kit(&space.id, "editor", 0, kit.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(created.revision, 1);
        assert_eq!(
            db.get_space_brand_kit_for(&space.id, "viewer")
                .await
                .unwrap()
                .kit,
            kit
        );
        assert!(db
            .update_space_brand_kit(&space.id, "owner", 0, BrandKitPayload::default())
            .await
            .unwrap_err()
            .to_string()
            .contains("revision conflict"));
        assert!(db
            .update_space_brand_kit(
                &space.id,
                "owner",
                1,
                BrandKitPayload {
                    logo_source_ids: vec!["other-space-logo".into()],
                    ..BrandKitPayload::default()
                },
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("not owned"));
    }
}
