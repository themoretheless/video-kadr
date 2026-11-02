use video_kadr_backend::{domain::concat_cutout_clock::{MaskTimeline,AnalysisRun},services::{concat_video_masks::{open,ExpectedRun},local_person_sequence::Metadata,local_person::MODEL_HASH}};
use sha2::{Digest,Sha256};use tokio_util::sync::CancellationToken;
#[test]
fn missing_grid_includes_source_boundaries_and_splits_existing_frames(){
 let times=MaskTimeline::new(vec![100,400]).unwrap();
 assert_eq!(times.missing_runs(&[(0.15,0.4),(0.35,0.61)]).unwrap(),vec![AnalysisRun{start_millis:200,frames:2},AnalysisRun{start_millis:500,frames:2}]);
 assert_eq!(MaskTimeline::new(vec![]).unwrap().missing_runs(&[(-1.,0.),(0.,0.0001)]).unwrap(),vec![AnalysisRun{start_millis:0,frames:1}]);
 assert!(times.missing_runs(&[(f64::NAN,1.)]).is_err());assert!(times.missing_runs(&[(0.,36000.)]).is_err());
}
#[tokio::test]
async fn verified_run_reads_exact_planes_and_refuses_tampering_and_wrong_identity(){
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("masks.raw");let mut bytes=vec![0u8;512*256];bytes.extend(vec![255;512*256]);std::fs::write(&path,&bytes).unwrap();
 let metadata=Metadata{schema_version:7,width:2,height:1,mask_width:512,mask_height:256,mask_bytes:bytes.len()as u64,blank_frames:1,mask_hash:format!("{:x}",Sha256::digest(&bytes)),frames:2,bytes:16,source_hash:"a".repeat(64),model_hash:MODEL_HASH.into(),runtime_hash:"b".repeat(64),output_hash:"c".repeat(64)};
 let expected=ExpectedRun{decoded_source_hash:&metadata.source_hash,model_hash:MODEL_HASH,runtime_hash:&metadata.runtime_hash};let cancel=CancellationToken::new();let mut run=open(&path,300,&metadata,expected,&cancel).await.unwrap();
 assert!(run.automatic(300,&cancel).unwrap().is_none());assert_eq!(run.read(400,&cancel).unwrap().probabilities,vec![255;512*256]);assert_eq!(run.timeline().unwrap().nearest_millis(0.36).unwrap(),Some(400));assert!(run.read(350,&cancel).is_err());
 use std::io::{Seek,SeekFrom,Write};let mut file=std::fs::OpenOptions::new().write(true).open(&path).unwrap();file.seek(SeekFrom::Start(512*256)).unwrap();file.write_all(&[0]).unwrap();assert!(run.read(400,&cancel).is_err());assert!(open(&path,300,&metadata,expected,&cancel).await.is_err());
 let wrong=ExpectedRun{decoded_source_hash:"wrong",..expected};assert!(open(&path,300,&metadata,wrong,&cancel).await.is_err());cancel.cancel();assert!(run.read(300,&cancel).is_err());
}

#[tokio::test]
#[ignore="requires native RVM executable/model/ORT; frames are procedural"]
async fn actual_rvm_run_enters_source_clock_provider_and_preserves_blank_fallback(){
 use video_kadr_backend::{services::{local_person,local_person_sequence},process_control::ProcessRuntime,domain::{composition::{ClipPlacement,PlaybackMode},concat_cutout_clock::{apply_video,Dispatch},concat_picture::Preparation,concat_cutout_mask::{apply,MaskPlane},group_surface::GroupSurface}};
 let config=local_person::Config{executable:std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),model:std::env::var("VIDEO_KADR_TEST_PERSON_MODEL").unwrap().into(),runtime_library:std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap().into()};
 let dir=tempfile::tempdir().unwrap();let source=dir.path().join("frames.rgba");let pixels:Vec<u8>=(0..3).flat_map(|frame|(0..37*23).flat_map(move|pixel|[(pixel%151)as u8,((pixel*17+frame*31)%180)as u8,((pixel*31)%199)as u8,(pixel%256)as u8])).collect();std::fs::write(&source,&pixels).unwrap();
 let hash=format!("{:x}",Sha256::digest(&pixels));let output=dir.path().join("output.rgba");let mask_path=dir.path().join("masks.raw");let cancel=CancellationToken::new();let(tx,_rx)=tokio::sync::mpsc::unbounded_channel();
 let metadata=local_person_sequence::segment(&ProcessRuntime::local_default(),&config,local_person_sequence::Input{source:&source,source_hash:&hash,mask_destination:Some(&mask_path),width:37,height:23,frames:3},&output,&cancel,std::time::Duration::from_secs(60),&tx).await.unwrap();
 let mut run=open(&mask_path,300,&metadata,ExpectedRun{decoded_source_hash:&hash,model_hash:MODEL_HASH,runtime_hash:&metadata.runtime_hash},&cancel).await.unwrap();let times=run.timeline().unwrap();assert!(times.missing_runs(&[(0.3,0.5)]).unwrap().is_empty());
 let placement:ClipPlacement=serde_json::from_value(serde_json::json!({"timelineStartTick":1000000,"sourceInTick":300000,"sourceOutTick":600000,"speed":2})).unwrap();let geometry=Preparation{crop:[0.;4],canvas:[37,23],flip_h:false,flip_v:false};
 let raw=GroupSurface{width:37,height:23,rgba:pixels[..37*23*4].to_vec()};let mask=run.read(400,&cancel).unwrap();let blank=mask.probabilities.iter().all(|v|*v<10);let mut expected=raw.clone();if !blank{apply(&mut expected,MaskPlane{width:mask.width,height:mask.height,probabilities:&mask.probabilities},&geometry,&mut||Ok(())).unwrap();}
 let mut actual=raw;let mut requested=vec![];let dispatch=apply_video(&mut actual,&placement,PlaybackMode::Forward,1075000,&times,&geometry,false,&mut|millis|{requested.push(millis);run.automatic(millis,&cancel)},&mut||Ok(())).unwrap();assert_eq!(requested,[400]);assert_eq!(actual,expected);
 assert_eq!(dispatch,if blank{Dispatch::Missing{source_us:450000,mask_millis:400}}else{Dispatch::Applied{source_us:450000,mask_millis:400}});
}

#[tokio::test]
async fn reopened_plane_refuses_path_replacement_symlink_missing_and_size_drift(){
 let root=tempfile::tempdir().unwrap();let path=root.path().join("mask.raw");let original=vec![0u8;512*256];std::fs::write(&path,&original).unwrap();let metadata=Metadata{schema_version:7,width:2,height:1,mask_width:512,mask_height:256,mask_bytes:original.len()as u64,blank_frames:1,mask_hash:format!("{:x}",Sha256::digest(&original)),frames:1,bytes:8,source_hash:"a".repeat(64),model_hash:MODEL_HASH.into(),runtime_hash:"b".repeat(64),output_hash:"c".repeat(64)};let expected=ExpectedRun{decoded_source_hash:&metadata.source_hash,model_hash:MODEL_HASH,runtime_hash:&metadata.runtime_hash};let cancel=CancellationToken::new();let mut run=open(&path,0,&metadata,expected,&cancel).await.unwrap();
 let moved=root.path().join("original.raw");std::fs::rename(&path,&moved).unwrap();std::fs::write(&path,vec![255u8;original.len()]).unwrap();assert!(run.read(0,&cancel).is_err());std::fs::remove_file(&path).unwrap();assert!(run.read(0,&cancel).is_err());
 #[cfg(unix)]{std::os::unix::fs::symlink(&moved,&path).unwrap();assert!(run.read(0,&cancel).is_err());assert!(open(&path,0,&metadata,expected,&cancel).await.is_err());std::fs::remove_file(&path).unwrap();}
 std::fs::write(&path,&original).unwrap();assert!(run.automatic(0,&cancel).unwrap().is_none());let mut extra=original.clone();extra.push(0);std::fs::write(&path,extra).unwrap();assert!(run.read(0,&cancel).is_err());std::fs::remove_file(&path).unwrap();std::fs::create_dir(&path).unwrap();assert!(run.read(0,&cancel).is_err());
}

#[cfg(unix)]
#[test]
fn many_verified_runs_work_under_an_isolated_64_descriptor_limit(){
 let result=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","descriptor_limit_child","--nocapture"]).env("VIDEO_KADR_MASK_FD_CHILD","1").output().unwrap();assert!(result.status.success(),"stdout:{} stderr:{}",String::from_utf8_lossy(&result.stdout),String::from_utf8_lossy(&result.stderr));
}
#[cfg(unix)]
#[test]
fn descriptor_limit_child(){
 if std::env::var_os("VIDEO_KADR_MASK_FD_CHILD").is_none(){return}
 let mut limit=libc::rlimit{rlim_cur:0,rlim_max:0};assert_eq!(unsafe{libc::getrlimit(libc::RLIMIT_NOFILE,&mut limit)},0);limit.rlim_cur=64.min(limit.rlim_max);assert_eq!(unsafe{libc::setrlimit(libc::RLIMIT_NOFILE,&limit)},0);
 let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();runtime.block_on(async{
  let root=tempfile::tempdir().unwrap();let path=root.path().join("mask.raw");let pixels=vec![255u8;256*256];std::fs::write(&path,&pixels).unwrap();let model="3241ac4ad8aa35bdaf33946776db29f7c283a413aa0b0dacb9483594b4531aad";let metadata=Metadata{schema_version:10,width:1,height:1,mask_width:256,mask_height:256,mask_bytes:pixels.len()as u64,blank_frames:0,mask_hash:format!("{:x}",Sha256::digest(&pixels)),frames:1,bytes:4,source_hash:"a".repeat(64),model_hash:model.into(),runtime_hash:"b".repeat(64),output_hash:"c".repeat(64)};let expected=ExpectedRun{decoded_source_hash:&metadata.source_hash,model_hash:model,runtime_hash:&metadata.runtime_hash};let cancel=CancellationToken::new();let mut runs=vec![];
  for index in 0..512{runs.push(open(&path,index*100,&metadata,expected,&cancel).await.unwrap());}
  for(index,run)in runs.iter_mut().enumerate(){assert_eq!(run.read(index as u64*100,&cancel).unwrap().probabilities,pixels);}
  eprintln!("512 verified runs read under RLIMIT_NOFILE={}",limit.rlim_cur);
 });
}
