//! Tauri command wrappers for the read-only Skein eval-bench Slice Explorer.

use crate::services::eval_bench::{self, ListRunsResult, RunDetail};

#[tauri::command]
pub async fn eval_bench_list_runs(
    limit: Option<i64>,
    recipe: Option<String>,
    store_path: Option<String>,
) -> Result<ListRunsResult, String> {
    let path = eval_bench::resolve_store_path(store_path.as_deref())?;
    eval_bench::list_runs(&path, limit.unwrap_or(20), recipe.as_deref())
}

#[tauri::command]
pub async fn eval_bench_get_run(
    run_id: i64,
    store_path: Option<String>,
) -> Result<Option<RunDetail>, String> {
    let path = eval_bench::resolve_store_path(store_path.as_deref())?;
    eval_bench::get_run_detail(&path, run_id)
}
