use wf_storage::adapter::metrics::{MetricsDataPoint, MetricsStorageAdapter};
use wf_storage::context::StorageContext;

pub async fn save_metrics_batch(
    ctx: &StorageContext,
    points: &[MetricsDataPoint],
) -> crate::ApiResult<()> {
    ctx.metrics.save_batch(points).await?;
    Ok(())
}

pub async fn query_metrics(
    ctx: &StorageContext,
    name: &str,
    start_time: i64,
    end_time: i64,
) -> crate::ApiResult<Vec<MetricsDataPoint>> {
    ctx.metrics
        .query(name, start_time, end_time)
        .await
        .map_err(Into::into)
}

pub async fn delete_old_metrics(ctx: &StorageContext, older_than: i64) -> crate::ApiResult<u64> {
    ctx.metrics.delete_old(older_than).await.map_err(Into::into)
}
