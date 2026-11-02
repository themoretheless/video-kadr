//! Owner-scoped transient native trim sessions. Invoke from the bounded CPU queue.
use super::filmcraft_dynamic_trim::DynamicTrimSession;
use std::{collections::HashMap,sync::Mutex,time::{Duration,Instant}};
use serde_json::Value;
#[derive(Debug,PartialEq,Eq)]
pub enum SessionError{Missing,Stale,CreationConflict,Capacity,Invalid(String),Unavailable}
pub struct SessionSnapshot{pub id:String,pub revision:u64,pub session:DynamicTrimSession}
struct Entry{owner:String,revision:u64,touched:Instant,bytes:usize,creation:Option<(String,[u8;32])>,session:DynamicTrimSession}
struct Inner{entries:HashMap<String,Entry>}
pub struct TrimSessionStore{inner:Mutex<Inner>,max_sessions:usize,max_owner_sessions:usize,max_bytes:usize,idle:Duration}
impl Default for TrimSessionStore{fn default()->Self{Self::new(16,4,64*1024*1024,Duration::from_secs(600))}}
impl TrimSessionStore{
 pub fn new(max_sessions:usize,max_owner_sessions:usize,max_bytes:usize,idle:Duration)->Self{Self{inner:Mutex::new(Inner{entries:HashMap::new()}),max_sessions,max_owner_sessions,max_bytes,idle}}
 fn prune(&self,inner:&mut Inner,now:Instant){inner.entries.retain(|_,entry|now.saturating_duration_since(entry.touched)<self.idle);}
 pub fn create(&self,owner:&str,session:DynamicTrimSession)->Result<SessionSnapshot,SessionError>{self.create_keyed_at(owner,session,None,Instant::now())}
 pub fn create_keyed(&self,owner:&str,session:DynamicTrimSession,key:String,fingerprint:[u8;32])->Result<SessionSnapshot,SessionError>{
  if key.is_empty()||key.len()>128||!key.bytes().all(|c|c.is_ascii_alphanumeric()||c==b'-'||c==b'_'){return Err(SessionError::Invalid("invalid trim creation key".into()));}
  self.create_keyed_at(owner,session,Some((key,fingerprint)),Instant::now())
 }
 #[cfg(test)]fn create_at(&self,owner:&str,session:DynamicTrimSession,now:Instant)->Result<SessionSnapshot,SessionError>{self.create_keyed_at(owner,session,None,now)}
 fn create_keyed_at(&self,owner:&str,session:DynamicTrimSession,creation:Option<(String,[u8;32])>,now:Instant)->Result<SessionSnapshot,SessionError>{
  if owner.is_empty(){return Err(SessionError::Missing);}let bytes=session.retained_bytes().map_err(|e|SessionError::Invalid(e.to_string()))?;let mut inner=self.inner.lock().map_err(|_|SessionError::Unavailable)?;self.prune(&mut inner,now);
  if let Some((key,fingerprint))=&creation{if let Some((id,entry))=inner.entries.iter_mut().find(|(_,entry)|entry.owner==owner&&entry.creation.as_ref().is_some_and(|(stored,_)|stored==key)){
   if entry.creation.as_ref().unwrap().1!=*fingerprint{return Err(SessionError::CreationConflict);}entry.touched=now;return Ok(SessionSnapshot{id:id.clone(),revision:entry.revision,session:entry.session.clone()});
  }}
  let used:usize=inner.entries.values().map(|entry|entry.bytes).sum();if inner.entries.len()>=self.max_sessions||inner.entries.values().filter(|entry|entry.owner==owner).count()>=self.max_owner_sessions||bytes>self.max_bytes.saturating_sub(used){return Err(SessionError::Capacity);}
  let id=uuid::Uuid::new_v4().to_string();let snapshot=SessionSnapshot{id:id.clone(),revision:0,session:session.clone()};inner.entries.insert(id,Entry{owner:owner.into(),revision:0,touched:now,bytes,creation,session});Ok(snapshot)
 }
 pub fn read(&self,owner:&str,id:&str)->Result<SessionSnapshot,SessionError>{self.read_at(owner,id,Instant::now())}
 fn read_at(&self,owner:&str,id:&str,now:Instant)->Result<SessionSnapshot,SessionError>{let mut inner=self.inner.lock().map_err(|_|SessionError::Unavailable)?;self.prune(&mut inner,now);let entry=inner.entries.get_mut(id).filter(|entry|entry.owner==owner).ok_or(SessionError::Missing)?;entry.touched=now;Ok(SessionSnapshot{id:id.into(),revision:entry.revision,session:entry.session.clone()})}
 pub fn command(&self,owner:&str,id:&str,expected:u64,action:impl FnOnce(&mut DynamicTrimSession)->anyhow::Result<Value>)->Result<(SessionSnapshot,Value),SessionError>{self.command_at(owner,id,expected,Instant::now(),action)}
 fn command_at(&self,owner:&str,id:&str,expected:u64,now:Instant,action:impl FnOnce(&mut DynamicTrimSession)->anyhow::Result<Value>)->Result<(SessionSnapshot,Value),SessionError>{
  let mut inner=self.inner.lock().map_err(|_|SessionError::Unavailable)?;self.prune(&mut inner,now);let entry=inner.entries.get(id).filter(|entry|entry.owner==owner).ok_or(SessionError::Missing)?;if expected!=entry.revision{return Err(SessionError::Stale);}let revision=entry.revision.checked_add(1).ok_or(SessionError::Unavailable)?;let mut work=entry.session.clone();
  let result=action(&mut work).map_err(|e|SessionError::Invalid(e.to_string()))?;let bytes=work.retained_bytes().map_err(|e|SessionError::Invalid(e.to_string()))?;let others:usize=inner.entries.iter().filter(|(key,_)|key.as_str()!=id).map(|(_,entry)|entry.bytes).sum();if bytes>self.max_bytes.saturating_sub(others){return Err(SessionError::Capacity);}
  let snapshot=SessionSnapshot{id:id.into(),revision,session:work.clone()};let entry=inner.entries.get_mut(id).unwrap();entry.session=work;entry.revision=revision;entry.touched=now;entry.bytes=bytes;Ok((snapshot,result))
 }
 pub fn close(&self,owner:&str,id:&str,expected:u64)->Result<(),SessionError>{let mut inner=self.inner.lock().map_err(|_|SessionError::Unavailable)?;self.prune(&mut inner,Instant::now());let entry=inner.entries.get(id).filter(|entry|entry.owner==owner).ok_or(SessionError::Missing)?;if entry.revision!=expected{return Err(SessionError::Stale);}inner.entries.remove(id);Ok(())}
}
#[cfg(test)]mod tests{
 use super::*;use crate::domain::{filmcraft_project_file::SourceProjectDocument,filmcraft_trim::{TrimEditPoint,TrimKind,Edge}};
 pub(super) fn session()->DynamicTrimSession{let source:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-edit-point-transition.json")).unwrap();let base=&source["cases"][0];DynamicTrimSession::new(SourceProjectDocument::read(&serde_json::to_vec(&base["document"]).unwrap()).unwrap(),base["sequence"].as_u64().unwrap(),vec![TrimEditPoint{clip:1001,edge:Edge::Out,kind:TrimKind::Trim}],true).unwrap()}
 #[test]fn owner_revision_refusal_and_close(){let store=TrimSessionStore::default();let created=store.create("alice",session()).unwrap();assert!(matches!(store.read("bob",&created.id),Err(SessionError::Missing)));let (updated,_)=store.command("alice",&created.id,0,|s|s.shuttle(0.0,1.0,false)).unwrap();assert_eq!(updated.revision,1);assert!(matches!(store.command("alice",&created.id,0,|s|s.tick(1.0)),Err(SessionError::Stale)));let before=store.read("alice",&created.id).unwrap();assert!(matches!(store.command("alice",&created.id,1,|s|{s.playhead=123;anyhow::bail!("refused")}),Err(SessionError::Invalid(_))));let after=store.read("alice",&created.id).unwrap();assert_eq!(before.session.playhead,after.session.playhead);assert_eq!(after.revision,1);assert_eq!(store.close("bob",&created.id,1),Err(SessionError::Missing));assert_eq!(store.close("alice",&created.id,0),Err(SessionError::Stale));store.close("alice",&created.id,1).unwrap();assert!(matches!(store.read("alice",&created.id),Err(SessionError::Missing)));}
 #[test]fn capacity_does_not_evict_and_expiry_recovers(){let now=Instant::now();let store=TrimSessionStore::new(2,1,usize::MAX,Duration::from_secs(10));let first=store.create_at("alice",session(),now).unwrap();assert!(matches!(store.create_at("alice",session(),now),Err(SessionError::Capacity)));store.create_at("bob",session(),now).unwrap();assert!(matches!(store.create_at("carol",session(),now),Err(SessionError::Capacity)));assert!(store.read_at("alice",&first.id,now).is_ok());assert!(matches!(store.read_at("alice",&first.id,now+Duration::from_secs(10)),Err(SessionError::Missing)));assert!(store.create_at("carol",session(),now+Duration::from_secs(10)).is_ok());}
 #[test]fn snapshot_growth_capacity_rolls_back(){let initial=session();let bytes=initial.retained_bytes().unwrap();let store=TrimSessionStore::new(4,4,bytes,Duration::from_secs(10));let created=store.create("alice",initial).unwrap();assert!(matches!(store.command("alice",&created.id,0,|s|s.shuttle(0.0,1.0,false)),Err(SessionError::Capacity)));let state=store.read("alice",&created.id).unwrap();assert_eq!(state.revision,0);assert_eq!(state.session.status().unwrap()["playing"],false);}
 #[test]fn competing_same_revision_mutates_once(){let store=std::sync::Arc::new(TrimSessionStore::default());let created=store.create("alice",session()).unwrap();let barrier=std::sync::Arc::new(std::sync::Barrier::new(2));let threads:Vec<_>=(0..2).map(|_|{let store=store.clone();let barrier=barrier.clone();let id=created.id.clone();std::thread::spawn(move||{barrier.wait();store.command("alice",&id,0,|s|s.shuttle(0.0,1.0,false)).map(|(s,_)|s.revision)})}).collect();let results:Vec<_>=threads.into_iter().map(|t|t.join().unwrap()).collect();assert_eq!(results.iter().filter(|r|r.is_ok()).count(),1);assert_eq!(results.iter().filter(|r|matches!(r,Err(SessionError::Stale))).count(),1);assert_eq!(store.read("alice",&created.id).unwrap().revision,1);}
}

#[cfg(test)]mod creation_tests{
 use super::*;
 #[test]fn creation_replay_is_owner_scoped_and_does_not_reset_commands(){
  let store=TrimSessionStore::new(2,1,usize::MAX,Duration::from_secs(600));
  let make=||super::tests::session();let first=store.create_keyed("alice",make(),"retry-key".into(),[1;32]).unwrap();
  let again=store.create_keyed("alice",make(),"retry-key".into(),[1;32]).unwrap();assert_eq!(first.id,again.id);
  store.command("alice",&first.id,0,|s|{s.playhead=123;Ok(Value::Null)}).unwrap();
  let recovered=store.create_keyed("alice",make(),"retry-key".into(),[1;32]).unwrap();assert_eq!(recovered.revision,1);assert_eq!(recovered.session.playhead,123);
  assert!(matches!(store.create_keyed("alice",make(),"retry-key".into(),[2;32]),Err(SessionError::CreationConflict)));
  let bob=store.create_keyed("bob",make(),"retry-key".into(),[1;32]).unwrap();assert_ne!(first.id,bob.id);
  assert!(matches!(store.create_keyed("alice",make(),"another-key".into(),[1;32]),Err(SessionError::Capacity)));
 }
 #[test]fn expired_creation_identity_is_removed_with_session(){let now=Instant::now();let store=TrimSessionStore::new(1,1,usize::MAX,Duration::from_secs(10));let first=store.create_keyed_at("alice",super::tests::session(),Some(("key".into(),[1;32])),now).unwrap();let next=store.create_keyed_at("alice",super::tests::session(),Some(("key".into(),[1;32])),now+Duration::from_secs(10)).unwrap();assert_ne!(first.id,next.id);}
}
