use video_kadr_backend::{services::concat_cutout_selection::{analyse,stale,Model},domain::{concat_cutout_import::Subject,concat_cutout_clock::ResolvedMask}};
use tokio_util::sync::CancellationToken;
fn mask(value:u8)->ResolvedMask{ResolvedMask{width:1,height:1,probabilities:vec![value]}}
#[tokio::test]
async fn auto_redecodes_object_only_on_first_blank_and_respects_recorded_family(){
 for (subject,cached,probability,expected,calls) in [
 (Subject::Auto,None,9,Model::Object,vec![Model::Person,Model::Object]),
 (Subject::Auto,None,10,Model::Person,vec![Model::Person]),
 (Subject::Auto,Some(Model::Person),0,Model::Person,vec![Model::Person]),
 (Subject::Auto,Some(Model::Object),0,Model::Object,vec![Model::Object]),
 (Subject::Person,None,0,Model::Person,vec![Model::Person]),
 (Subject::Object,None,0,Model::Object,vec![Model::Object])]{
  let mut loaded=vec![];let mut decoded=vec![];
  let(chosen,_)=analyse(subject,cached,|m|{loaded.push(m);std::future::ready(Ok(m))},|m|{decoded.push(m);std::future::ready(Ok(mask(probability)))},&CancellationToken::new()).await.unwrap();
  assert_eq!(chosen,expected);assert_eq!(loaded,calls);assert_eq!(decoded,calls);
 }
 assert!(stale(Subject::Person,Some(Model::Object)));assert!(stale(Subject::Object,Some(Model::Selfie)));assert!(!stale(Subject::Auto,Some(Model::Object)));
}
#[tokio::test]
async fn person_acquisition_falls_back_but_cancel_and_inference_errors_propagate(){
 let token=CancellationToken::new();let mut loaded=vec![];
 let(chosen,_)=analyse(Subject::Person,None,|m|{loaded.push(m);std::future::ready(if m==Model::Person{Err(anyhow::anyhow!("download unavailable"))}else{Ok(m)})},|_|std::future::ready(Ok(mask(255))),&token).await.unwrap();
 assert_eq!(chosen,Model::Selfie);assert_eq!(loaded,[Model::Person,Model::Selfie]);
 let mut loaded=vec![];assert!(analyse(Subject::Auto,None,|m|{loaded.push(m);std::future::ready(Ok(m))},|_|std::future::ready(Err(anyhow::anyhow!("inference failed"))),&token).await.is_err());assert_eq!(loaded,[Model::Person]);
 let other=token.clone();let mut count=0;assert!(analyse(Subject::Auto,None,|_|{count+=1;other.cancel();std::future::ready(Err::<Model,_>(anyhow::anyhow!("cancelled")))},|_|std::future::ready(Ok(mask(0))),&token).await.is_err());assert_eq!(count,1);
}
