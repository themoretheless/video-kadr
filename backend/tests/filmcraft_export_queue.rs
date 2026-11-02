use serde_json::{json, Value};
use std::sync::Arc;
use video_kadr_backend::domain::filmcraft_export_queue::{Outcome, Queue, Status};

#[test]
fn source_order_cancel_retry_and_finished_clear_scenario() {
    let mut queue = Queue::default();
    let ids = queue.add(vec!["a", "b", "c", "d"]).unwrap();
    let (a, b, c, d) = (ids[0], ids[1], ids[2], ids[3]);
    assert!(queue
        .items()
        .iter()
        .all(|item| item.status == Status::Ready && item.job.is_none()));
    queue.move_item(c, Some(0), None).unwrap();
    queue.move_item(a, None, Some(1)).unwrap();
    assert_eq!(
        queue.items().iter().map(|item| item.id).collect::<Vec<_>>(),
        [c, b, a, d]
    );
    assert!(queue.cancel(Some(d)).unwrap().is_empty());
    queue.start().unwrap();
    let mut order = Vec::new();
    while let Some(item) = queue.next_ready() {
        let id = item.id;
        order.push(item.payload);
        queue.begin(id, format!("job-{id}")).unwrap();
        assert!(queue.next_ready().is_none());
        queue.settle(id, Outcome::Done).unwrap();
    }
    assert_eq!(order, ["c", "b", "a"]);
    assert!(!queue.running());
    assert_eq!(
        queue
            .items()
            .iter()
            .map(|item| item.status)
            .collect::<Vec<_>>(),
        [Status::Done, Status::Done, Status::Done, Status::Cancelled]
    );
    queue.retry(d, true).unwrap();
    assert_eq!(queue.next_ready().unwrap().id, d);
    queue.begin(d, "retry-d".into()).unwrap();
    queue.settle(d, Outcome::Done).unwrap();
    assert!(queue.next_ready().is_none());
    queue.clear(false).unwrap();
    assert!(queue.items().is_empty());
    assert!(queue.start().is_err());
}

#[test]
fn stop_drains_active_cancel_waits_for_settlement_and_refusals_are_atomic() {
    let mut queue = Queue::default();
    let ids = queue.add(vec![1, 2, 3]).unwrap();
    queue.start().unwrap();
    queue.begin(ids[0], "active".into()).unwrap();
    queue.stop();
    assert!(queue.is_active());
    assert!(queue.next_ready().is_none());
    let before = serde_json::to_value(&queue).unwrap();
    assert!(queue.remove(ids[0]).is_err());
    assert!(queue.clear(true).is_err());
    assert!(queue.retry(ids[0], true).is_err());
    assert!(queue.begin(ids[1], "overlap".into()).is_err());
    assert!(queue.cancel(Some(999)).is_err());
    assert_eq!(serde_json::to_value(&queue).unwrap(), before);
    queue.settle(ids[0], Outcome::Done).unwrap();
    assert!(!queue.is_active());
    assert!(queue.next_ready().is_none());
    queue.start().unwrap();
    queue.begin(ids[1], "second".into()).unwrap();
    assert_eq!(queue.cancel(None).unwrap(), ["second"]);
    assert_eq!(queue.items()[1].status, Status::Encoding);
    assert_eq!(queue.items()[2].status, Status::Cancelled);
    assert!(queue.is_active());
    queue.settle(ids[1], Outcome::Cancelled).unwrap();
    assert!(!queue.is_active());
    queue.clear(false).unwrap();
    assert!(queue.items().is_empty());
}

#[test]
fn spawn_failure_and_runtime_failure_retry_keep_payload_snapshot() {
    let mut document = Arc::new(json!({"sequence":{"name":"queued","revision":1}}));
    let mut queue = Queue::default();
    let id = queue.add(vec![document.clone()]).unwrap()[0];
    Arc::make_mut(&mut document)["sequence"]["revision"] = json!(2);
    queue.start().unwrap();
    queue.fail_ready(id, "admission failed".into()).unwrap();
    assert_eq!(queue.items()[0].error.as_deref(), Some("admission failed"));
    assert_eq!(queue.items()[0].payload["sequence"]["revision"], 1);
    queue.retry(id, true).unwrap();
    assert!(queue.items()[0].error.is_none());
    assert!(queue.items()[0].job.is_none());
    queue.begin(id, "job".into()).unwrap();
    queue
        .settle(id, Outcome::Failed("encoder failed".into()))
        .unwrap();
    queue.retry(id, false).unwrap();
    assert_eq!(queue.items()[0].payload["sequence"]["revision"], 1);
    let wire = serde_json::to_value(&queue).unwrap();
    assert!(wire["items"][0].get("payload").is_none());
    assert!(wire["items"][0].get("project").is_none());
}

#[test]
fn move_clamps_extreme_offsets_and_clear_keeps_pending_items() {
    let mut queue = Queue::<Value>::default();
    assert!(queue.add(vec![]).is_err());
    let ids = queue.add(vec![json!("a"), json!("b"), json!("c")]).unwrap();
    queue.move_item(ids[0], None, Some(i64::MAX)).unwrap();
    assert_eq!(queue.items()[2].id, ids[0]);
    queue
        .move_item(ids[0], Some(i64::MIN), Some(i64::MAX))
        .unwrap();
    assert_eq!(queue.items()[0].id, ids[0]);
    queue.cancel(Some(ids[1])).unwrap();
    queue.clear(false).unwrap();
    assert_eq!(queue.items().len(), 2);
    queue.clear(true).unwrap();
    assert!(queue.items().is_empty());
    let next = queue.add(vec![json!("new")]).unwrap()[0];
    assert!(next > ids[2]);
}
