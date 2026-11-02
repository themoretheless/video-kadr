//! Session queue rules adapted from pinned Filmcraft engine/export_tools.rs (MIT,
//! 5231852443363f001c3f6b396dd9b1e6461ae2be). Payloads are immutable snapshots
//! supplied by admission; starting/cancelling jobs belongs to the host adapter.
use anyhow::{ensure, Context, Result};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Ready,
    Encoding,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Serialize)]
#[serde(bound(serialize = ""))]
pub struct Item<T> {
    pub id: u64,
    pub status: Status,
    pub job: Option<String>,
    pub error: Option<String>,
    #[serde(skip)]
    pub payload: T,
}

#[derive(Debug, Serialize)]
#[serde(bound(serialize = ""))]
pub struct Queue<T> {
    items: Vec<Item<T>>,
    running: bool,
    #[serde(skip)]
    next_id: u64,
}
impl<T> Default for Queue<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            running: false,
            next_id: 0,
        }
    }
}
pub enum Outcome {
    Done,
    Failed(String),
    Cancelled,
}

impl<T> Queue<T> {
    pub fn items(&self) -> &[Item<T>] {
        &self.items
    }
    pub fn running(&self) -> bool {
        self.running
    }
    pub fn is_active(&self) -> bool {
        self.items
            .iter()
            .any(|item| item.status == Status::Encoding)
            || (self.running && self.items.iter().any(|item| item.status == Status::Ready))
    }
    fn index(&self, id: u64) -> Result<usize> {
        self.items
            .iter()
            .position(|item| item.id == id)
            .context("unknown queued export")
    }

    /// Admission validates the complete sequence/range expansion before this call.
    pub fn add(&mut self, payloads: Vec<T>) -> Result<Vec<u64>> {
        ensure!(!payloads.is_empty(), "no exports to add");
        let last = self
            .next_id
            .checked_add(u64::try_from(payloads.len())?)
            .context("queue ID overflow")?;
        let mut ids = Vec::with_capacity(payloads.len());
        for payload in payloads {
            self.next_id += 1;
            ids.push(self.next_id);
            self.items.push(Item {
                id: self.next_id,
                status: Status::Ready,
                job: None,
                error: None,
                payload,
            });
        }
        debug_assert_eq!(self.next_id, last);
        Ok(ids)
    }
    pub fn start(&mut self) -> Result<()> {
        ensure!(
            self.items
                .iter()
                .any(|item| matches!(item.status, Status::Ready | Status::Encoding)),
            "queue has no ready items"
        );
        self.running = true;
        Ok(())
    }
    pub fn stop(&mut self) {
        self.running = false;
    }
    /// The adapter holds its owner lock across spawn and begin/fail_ready.
    pub fn next_ready(&mut self) -> Option<&Item<T>> {
        if !self.running
            || self
                .items
                .iter()
                .any(|item| item.status == Status::Encoding)
        {
            return None;
        }
        let index = self
            .items
            .iter()
            .position(|item| item.status == Status::Ready);
        if index.is_none() {
            self.running = false;
        }
        index.map(|index| &self.items[index])
    }
    fn candidate(&self, id: u64) -> Result<usize> {
        ensure!(
            self.running
                && !self
                    .items
                    .iter()
                    .any(|item| item.status == Status::Encoding),
            "queue is stopped or encoding"
        );
        let index = self
            .items
            .iter()
            .position(|item| item.status == Status::Ready)
            .context("queue has no ready items")?;
        ensure!(self.items[index].id == id, "export is not next in queue");
        Ok(index)
    }
    pub fn begin(&mut self, id: u64, job: String) -> Result<()> {
        let index = self.candidate(id)?;
        ensure!(!job.is_empty(), "missing export job");
        let item = &mut self.items[index];
        item.status = Status::Encoding;
        item.job = Some(job);
        item.error = None;
        Ok(())
    }
    pub fn fail_ready(&mut self, id: u64, error: String) -> Result<()> {
        let index = self.candidate(id)?;
        let item = &mut self.items[index];
        item.status = Status::Failed;
        item.error = Some(error);
        item.job = None;
        Ok(())
    }
    pub fn settle(&mut self, id: u64, outcome: Outcome) -> Result<()> {
        let index = self.index(id)?;
        let item = &mut self.items[index];
        ensure!(item.status == Status::Encoding, "export is not encoding");
        item.error = None;
        item.status = match outcome {
            Outcome::Done => Status::Done,
            Outcome::Cancelled => Status::Cancelled,
            Outcome::Failed(error) => {
                item.error = Some(error);
                Status::Failed
            }
        };
        Ok(())
    }
    /// Return active job IDs whose cancellation tokens the adapter must signal.
    /// Encoding stays Encoding until its job actually stops.
    pub fn cancel(&mut self, id: Option<u64>) -> Result<Vec<String>> {
        let indices = match id {
            Some(id) => vec![self.index(id)?],
            None => {
                self.running = false;
                (0..self.items.len()).collect()
            }
        };
        let mut jobs = Vec::new();
        for index in indices {
            let item = &mut self.items[index];
            match item.status {
                Status::Ready => item.status = Status::Cancelled,
                Status::Encoding => {
                    if let Some(job) = &item.job {
                        jobs.push(job.clone());
                    }
                }
                _ => {}
            }
        }
        Ok(jobs)
    }
    pub fn retry(&mut self, id: u64, start: bool) -> Result<()> {
        let index = self.index(id)?;
        let item = &mut self.items[index];
        ensure!(
            !matches!(item.status, Status::Ready | Status::Encoding),
            "only terminal exports can be retried"
        );
        item.status = Status::Ready;
        item.job = None;
        item.error = None;
        if start {
            self.running = true;
        }
        Ok(())
    }
    pub fn remove(&mut self, id: u64) -> Result<Item<T>> {
        let index = self.index(id)?;
        ensure!(
            self.items[index].status != Status::Encoding,
            "cancel the export before removing it"
        );
        Ok(self.items.remove(index))
    }
    pub fn move_item(&mut self, id: u64, to: Option<i64>, by: Option<i64>) -> Result<()> {
        let index = self.index(id)?;
        let target = match (to, by) {
            (Some(to), _) => i128::from(to),
            (None, Some(by)) => index as i128 + i128::from(by),
            _ => anyhow::bail!("need absolute or relative destination"),
        };
        let target = target.clamp(0, self.items.len() as i128 - 1) as usize;
        let item = self.items.remove(index);
        self.items.insert(target, item);
        Ok(())
    }
    pub fn clear(&mut self, all: bool) -> Result<()> {
        ensure!(
            !all || !self
                .items
                .iter()
                .any(|item| item.status == Status::Encoding),
            "cancel the export before clearing all"
        );
        self.items
            .retain(|item| !all && matches!(item.status, Status::Ready | Status::Encoding));
        Ok(())
    }
}
