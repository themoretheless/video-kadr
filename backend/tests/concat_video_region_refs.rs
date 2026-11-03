mod support;
use video_kadr_backend::db::Db;
#[tokio::test]
async fn shared_region_refs_are_additive_persistent_and_project_scoped(){
 let(state,_dir)=support::make_state(false,false).await;let pool=sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")).foreign_keys(true)).await.unwrap();
 let document=serde_json::to_value(video_kadr_backend::domain::composition::Composition::new(Default::default())).unwrap();let space=state.db.create_space("owner","Region refs").await.unwrap();let source=state.db.create_space_composition_project("Source",&document,&[],"owner",&space.id).await.unwrap();let target=state.db.create_space_composition_project("Target",&document,&[],"owner",&space.id).await.unwrap();
 sqlx::query("INSERT INTO person_video_project_resources(project_id,role,file_hash,source_id) VALUES(?,'sam-video-regions','file-hash','physical-mask')").bind(&source.id).execute(&pool).await.unwrap();
 sqlx::query("DROP TABLE concat_video_project_region_refs").execute(&pool).await.unwrap();let reopened=Db::open(&state.storage).await.unwrap();drop(reopened);
 for project in [&source.id,&target.id]{sqlx::query("INSERT INTO concat_video_project_region_refs(project_id,source_id,video_hash,mask_source_id,library_source_id,file_hash) VALUES(?,'slot','video-hash','logical-region','physical-mask','file-hash')").bind(project).execute(&pool).await.unwrap();}
 let reopened=Db::open(&state.storage).await.unwrap();drop(reopened);let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM concat_video_project_region_refs WHERE library_source_id='physical-mask'").fetch_one(&pool).await.unwrap();assert_eq!(count,2);
 assert!(sqlx::query("INSERT INTO person_video_project_resources(project_id,role,file_hash,source_id) VALUES(?,'sam-video-regions','file-hash','physical-mask')").bind(&target.id).execute(&pool).await.is_err());
 sqlx::query("DELETE FROM composition_projects WHERE id=?").bind(&target.id).execute(&pool).await.unwrap();let refs:i64=sqlx::query_scalar("SELECT COUNT(*) FROM concat_video_project_region_refs").fetch_one(&pool).await.unwrap();let legacy:i64=sqlx::query_scalar("SELECT COUNT(*) FROM person_video_project_resources").fetch_one(&pool).await.unwrap();assert_eq!(refs,1);assert_eq!(legacy,1);
}
