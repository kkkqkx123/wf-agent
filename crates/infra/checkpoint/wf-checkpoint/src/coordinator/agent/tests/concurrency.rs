use super::super::*;
use super::fixtures::*;
use wf_types::checkpoint::CheckpointTiming;

#[tokio::test]
async fn concurrent_creates_all_succeed_with_unique_ids() {
    let coord = make_coordinator();
    let n = 8u32;

    let tasks = (1..=n).map(|i| {
        let coord = &coord;
        async move {
            let mut snapshot = make_snapshot();
            snapshot.current_iteration = i;
            snapshot.status = format!("running-{i}");
            coord
                .create_checkpoint(CheckpointTiming::AfterExecute, "loop-1", snapshot)
                .await
                .unwrap()
        }
    });
    let ids = futures::future::join_all(tasks).await;

    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(
        unique.len(),
        n as usize,
        "every creation must mint its own row"
    );
    assert_eq!(
        coord
            .state_manager()
            .count_by_entity("loop-1")
            .await
            .unwrap(),
        n as u64
    );
}

#[tokio::test]
async fn concurrent_creates_keep_previous_links_resolvable() {
    let coord = make_coordinator();
    let n = 6u32;

    let tasks = (1..=n).map(|i| {
        let coord = &coord;
        async move {
            let mut snapshot = make_snapshot();
            snapshot.current_iteration = i;
            coord
                .create_checkpoint(CheckpointTiming::AfterExecute, "loop-1", snapshot)
                .await
                .unwrap()
        }
    });
    let _ids = futures::future::join_all(tasks).await;

    let all = coord
        .state_manager()
        .list_by_entity("loop-1")
        .await
        .unwrap();
    assert_eq!(all.len(), n as usize);

    // Every previous link must point at a row that exists: a racing merge
    // back must never leave a dangling parent behind.
    let known: std::collections::HashSet<_> = all.iter().map(|m| m.id.clone()).collect();
    for meta in &all {
        if let Some(prev) = &meta.previous_checkpoint_id {
            assert!(
                known.contains(prev),
                "previous link {} dangles (row {})",
                prev,
                meta.id
            );
        }
    }
}

#[tokio::test]
async fn concurrent_creates_restore_the_newest_state() {
    let coord = make_coordinator();
    let n = 5u32;

    let tasks = (1..=n).map(|i| {
        let coord = &coord;
        async move {
            let mut snapshot = make_snapshot();
            snapshot.current_iteration = i;
            coord
                .create_checkpoint(CheckpointTiming::AfterExecute, "loop-1", snapshot)
                .await
                .unwrap()
        }
    });
    let ids = futures::future::join_all(tasks).await;

    let latest = coord
        .state_manager()
        .get_latest("loop-1")
        .await
        .unwrap()
        .unwrap();
    assert!(
        ids.iter().any(|id| id == &latest.id),
        "latest row must be one of the concurrently created rows"
    );
    let entity = coord.restore(&latest.id).await.unwrap();
    assert!(entity.current_iteration >= 1 && entity.current_iteration <= n);
}
