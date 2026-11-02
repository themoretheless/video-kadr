//! Bounded actor/project/operation-scoped cancellation, including early cancel.
use std::{collections::HashMap,sync::{Arc,Mutex},time::{Duration,Instant}};
use anyhow::{ensure,Result};use tokio_util::sync::CancellationToken;
type Key=(String,String,String);
const LIMIT:usize=1024;const EARLY_TTL:Duration=Duration::from_secs(30);
struct Entry{token:CancellationToken,active:bool,expires:Instant}
#[derive(Default)]pub struct Registry{entries:Mutex<HashMap<Key,Entry>>}
pub struct Lease{registry:Arc<Registry>,key:Key,pub token:CancellationToken}
impl Drop for Lease{fn drop(&mut self){if let Ok(mut entries)=self.registry.entries.lock(){entries.remove(&self.key);}}}
fn key(actor:&str,project:&str,id:&str)->Result<Key>{ensure!(uuid::Uuid::parse_str(id)?.to_string()==id,"invalid video operation ID");Ok((actor.into(),project.into(),id.into()))}
impl Registry{
 pub fn begin(self:&Arc<Self>,actor:&str,project:&str,id:&str,parent:&CancellationToken)->Result<Lease>{
  let key=key(actor,project,id)?;let mut entries=self.entries.lock().map_err(|_|anyhow::anyhow!("invalid cancellation registry"))?;entries.retain(|_,entry|entry.active||entry.expires>Instant::now());
  if let Some(entry)=entries.get_mut(&key){ensure!(!entry.active,"video operation ID already active");entry.active=true;return Ok(Lease{registry:self.clone(),key,token:entry.token.clone()})}
  ensure!(entries.len()<LIMIT,"too many video operation handles");let token=parent.child_token();entries.insert(key.clone(),Entry{token:token.clone(),active:true,expires:Instant::now()+EARLY_TTL});Ok(Lease{registry:self.clone(),key,token})
 }
 pub fn cancel(&self,actor:&str,project:&str,id:&str)->Result<()>{
  let key=key(actor,project,id)?;let mut entries=self.entries.lock().map_err(|_|anyhow::anyhow!("invalid cancellation registry"))?;entries.retain(|_,entry|entry.active||entry.expires>Instant::now());
  if let Some(entry)=entries.get(&key){entry.token.cancel();return Ok(())}
  ensure!(entries.len()<LIMIT,"too many video operation handles");let token=CancellationToken::new();token.cancel();entries.insert(key,Entry{token,active:false,expires:Instant::now()+EARLY_TTL});Ok(())
 }
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn cancellation_is_scoped_and_live_handles_cannot_be_replaced(){let registry=Arc::new(Registry::default());let parent=CancellationToken::new();let id=uuid::Uuid::new_v4().to_string();let lease=registry.begin("owner","project",&id,&parent).unwrap();registry.cancel("other","project",&id).unwrap();registry.cancel("owner","other-project",&id).unwrap();assert!(!lease.token.is_cancelled());assert!(registry.begin("owner","project",&id,&parent).is_err());registry.cancel("owner","project",&id).unwrap();assert!(lease.token.is_cancelled());assert!(registry.begin("owner","project",&id,&parent).is_err());drop(lease);assert!(!registry.begin("owner","project",&id,&parent).unwrap().token.is_cancelled());}
 #[test]fn early_cancel_survives_registration_and_shutdown_propagates(){let registry=Arc::new(Registry::default());let parent=CancellationToken::new();let id=uuid::Uuid::new_v4().to_string();registry.cancel("owner","project",&id).unwrap();let lease=registry.begin("owner","project",&id,&parent).unwrap();assert!(lease.token.is_cancelled());drop(lease);let lease=registry.begin("owner","project",&id,&parent).unwrap();parent.cancel();assert!(lease.token.is_cancelled());drop(lease);assert!(registry.entries.lock().unwrap().is_empty());}
 #[test]fn tombstones_are_bounded_and_expired_entries_reclaimed(){let registry=Arc::new(Registry::default());for _ in 0..LIMIT{registry.cancel("owner","project",&uuid::Uuid::new_v4().to_string()).unwrap();}assert!(registry.cancel("owner","project",&uuid::Uuid::new_v4().to_string()).is_err());for entry in registry.entries.lock().unwrap().values_mut(){entry.expires=Instant::now()-Duration::from_secs(1);}registry.cancel("owner","project",&uuid::Uuid::new_v4().to_string()).unwrap();assert_eq!(registry.entries.lock().unwrap().len(),1);assert!(registry.cancel("owner","project","../bad").is_err());}
}
