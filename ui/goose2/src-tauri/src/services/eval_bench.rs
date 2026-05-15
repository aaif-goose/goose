//! Read-side access to the Skein eval-bench SQLite store.
//!
//! The store is written by the Python eval-bench tooling (`run_kpass.py`,
//! `run_once.py`) and lives at `~/.skein/eval-bench.sqlite` by default. This
//! module never writes to the store; it only opens it `READ_ONLY` so the
//! desktop view can never corrupt results recorded by the harness.
//!
//! The schema is mirrored from `eval-bench/lib/store.py` — keep the two in
//! sync if columns ever change.

use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const DEFAULT_STORE_SUBPATH: &str = ".skein/eval-bench.sqlite";

pub fn default_store_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(DEFAULT_STORE_SUBPATH))
}

/// Resolve a caller-supplied override against the default location.
pub fn resolve_store_path(override_path: Option<&str>) -> Result<PathBuf, String> {
    match override_path {
        Some(p) if !p.is_empty() => Ok(PathBuf::from(p)),
        _ => default_store_path().ok_or_else(|| "could not determine home directory".to_string()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub id: i64,
    pub recipe: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub k: i64,
    pub notes: Option<String>,
    pub n_trials: i64,
    /// Headline pass^k over every trial in this run. `None` when the run has
    /// no recorded trials yet.
    pub pass_pow_k: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Trial {
    pub task_id: String,
    pub trial_index: i64,
    pub passed: bool,
    pub polarity: String,
    pub tags: Vec<String>,
    pub axes: BTreeMap<String, serde_json::Value>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunDetail {
    pub run: RunSummary,
    pub trials: Vec<Trial>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ListRunsResult {
    pub runs: Vec<RunSummary>,
    pub store_path: String,
    /// True when the SQLite file is not present yet. `runs` is empty in that
    /// case — callers render a "no runs yet, run a recipe" state.
    pub store_missing: bool,
}

fn open_readonly(path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|err| format!("open eval-bench store: {err}"))
}

fn pass_pow_k(passes: i64, total: i64, k: i64) -> Option<f64> {
    if total <= 0 || k < 0 {
        return None;
    }
    let p = passes as f64 / total as f64;
    Some(p.powi(k as i32))
}

pub fn list_runs(
    store_path: &Path,
    limit: i64,
    recipe: Option<&str>,
) -> Result<ListRunsResult, String> {
    let store_path_str = store_path.to_string_lossy().into_owned();
    if !store_path.exists() {
        return Ok(ListRunsResult {
            runs: Vec::new(),
            store_path: store_path_str,
            store_missing: true,
        });
    }
    let conn = open_readonly(store_path)?;
    let limit = limit.clamp(1, 500);

    let base_sql = "SELECT r.id, r.recipe, r.started_at, r.finished_at, r.k, r.notes, \
            COALESCE(c.n, 0) AS n_trials, \
            COALESCE(c.passes, 0) AS passes \
        FROM run r \
        LEFT JOIN ( \
            SELECT run_id, COUNT(*) AS n, SUM(passed) AS passes \
            FROM trial GROUP BY run_id \
        ) c ON c.run_id = r.id";

    let mut runs = Vec::new();
    if let Some(recipe) = recipe {
        let sql = format!("{base_sql} WHERE r.recipe = ?1 ORDER BY r.id DESC LIMIT ?2");
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("prepare list_runs: {e}"))?;
        let rows = stmt
            .query_map((recipe, limit), row_to_run_summary)
            .map_err(|e| format!("query list_runs: {e}"))?;
        for row in rows {
            runs.push(row.map_err(|e| format!("row list_runs: {e}"))?);
        }
    } else {
        let sql = format!("{base_sql} ORDER BY r.id DESC LIMIT ?1");
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("prepare list_runs: {e}"))?;
        let rows = stmt
            .query_map((limit,), row_to_run_summary)
            .map_err(|e| format!("query list_runs: {e}"))?;
        for row in rows {
            runs.push(row.map_err(|e| format!("row list_runs: {e}"))?);
        }
    }

    Ok(ListRunsResult {
        runs,
        store_path: store_path_str,
        store_missing: false,
    })
}

fn row_to_run_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunSummary> {
    let id: i64 = row.get(0)?;
    let recipe: String = row.get(1)?;
    let started_at: String = row.get(2)?;
    let finished_at: Option<String> = row.get(3)?;
    let k: i64 = row.get(4)?;
    let notes: Option<String> = row.get(5)?;
    let n_trials: i64 = row.get(6)?;
    let passes: i64 = row.get(7)?;
    Ok(RunSummary {
        id,
        recipe,
        started_at,
        finished_at,
        k,
        notes,
        n_trials,
        pass_pow_k: pass_pow_k(passes, n_trials, k),
    })
}

pub fn get_run_detail(store_path: &Path, run_id: i64) -> Result<Option<RunDetail>, String> {
    if !store_path.exists() {
        return Ok(None);
    }
    let conn = open_readonly(store_path)?;

    let run = conn
        .query_row(
            "SELECT r.id, r.recipe, r.started_at, r.finished_at, r.k, r.notes, \
                COALESCE(c.n, 0) AS n_trials, COALESCE(c.passes, 0) AS passes \
             FROM run r \
             LEFT JOIN ( \
                SELECT run_id, COUNT(*) AS n, SUM(passed) AS passes \
                FROM trial GROUP BY run_id \
             ) c ON c.run_id = r.id \
             WHERE r.id = ?1",
            (run_id,),
            row_to_run_summary,
        )
        .map(Some)
        .or_else(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(format!("get_run_detail: {other}")),
        })?;

    let Some(run) = run else {
        return Ok(None);
    };

    let mut stmt = conn
        .prepare(
            "SELECT task_id, trial_index, passed, polarity, tags, axes_json, duration_ms \
             FROM trial WHERE run_id = ?1 ORDER BY task_id, trial_index",
        )
        .map_err(|e| format!("prepare get_run_detail trials: {e}"))?;
    let rows = stmt
        .query_map((run_id,), |row| {
            let task_id: String = row.get(0)?;
            let trial_index: i64 = row.get(1)?;
            let passed: i64 = row.get(2)?;
            let polarity: String = row.get(3)?;
            let tags_raw: String = row.get(4)?;
            let axes_json: String = row.get(5)?;
            let duration_ms: Option<i64> = row.get(6)?;
            Ok((
                task_id,
                trial_index,
                passed,
                polarity,
                tags_raw,
                axes_json,
                duration_ms,
            ))
        })
        .map_err(|e| format!("query trials: {e}"))?;

    let mut trials = Vec::new();
    for row in rows {
        let (task_id, trial_index, passed, polarity, tags_raw, axes_json, duration_ms) =
            row.map_err(|e| format!("trial row: {e}"))?;
        let tags: Vec<String> = tags_raw
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let axes: BTreeMap<String, serde_json::Value> = if axes_json.is_empty() {
            BTreeMap::new()
        } else {
            serde_json::from_str(&axes_json)
                .map_err(|e| format!("parse axes_json for trial in run {run_id}: {e}"))?
        };
        trials.push(Trial {
            task_id,
            trial_index,
            passed: passed != 0,
            polarity,
            tags,
            axes,
            duration_ms,
        });
    }

    Ok(Some(RunDetail { run, trials }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use tempfile::TempDir;

    fn make_store(dir: &TempDir) -> PathBuf {
        let path = dir.path().join("eval-bench.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE run (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                recipe TEXT NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                k INTEGER NOT NULL,
                notes TEXT
            );
            CREATE TABLE trial (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id INTEGER NOT NULL REFERENCES run(id) ON DELETE CASCADE,
                task_id TEXT NOT NULL,
                trial_index INTEGER NOT NULL,
                passed INTEGER NOT NULL CHECK (passed IN (0, 1)),
                polarity TEXT NOT NULL,
                tags TEXT NOT NULL,
                axes_json TEXT NOT NULL,
                grader_scores_json TEXT NOT NULL,
                duration_ms INTEGER,
                trace_id TEXT
            );",
        )
        .unwrap();
        path
    }

    fn insert_run(conn: &Connection, recipe: &str, k: i64) -> i64 {
        conn.execute(
            "INSERT INTO run (recipe, started_at, k) VALUES (?1, '2026-05-15T10:00:00+00:00', ?2)",
            params![recipe, k],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn insert_trial(
        conn: &Connection,
        run_id: i64,
        task: &str,
        idx: i64,
        passed: bool,
        axes_json: &str,
    ) {
        conn.execute(
            "INSERT INTO trial \
                (run_id, task_id, trial_index, passed, polarity, tags, axes_json, grader_scores_json) \
             VALUES (?1, ?2, ?3, ?4, 'positive', '', ?5, '{}')",
            params![run_id, task, idx, if passed { 1 } else { 0 }, axes_json],
        )
        .unwrap();
    }

    #[test]
    fn list_runs_returns_missing_marker_when_store_absent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nope.sqlite");
        let res = list_runs(&path, 10, None).unwrap();
        assert!(res.store_missing);
        assert!(res.runs.is_empty());
    }

    #[test]
    fn list_runs_orders_newest_first_and_applies_limit() {
        let dir = TempDir::new().unwrap();
        let path = make_store(&dir);
        let conn = Connection::open(&path).unwrap();
        insert_run(&conn, "recipes/a", 3);
        insert_run(&conn, "recipes/b", 5);
        insert_run(&conn, "recipes/c", 1);

        let res = list_runs(&path, 2, None).unwrap();
        assert!(!res.store_missing);
        let ids: Vec<i64> = res.runs.iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![3, 2]);
    }

    #[test]
    fn list_runs_filters_by_recipe() {
        let dir = TempDir::new().unwrap();
        let path = make_store(&dir);
        let conn = Connection::open(&path).unwrap();
        insert_run(&conn, "recipes/a", 3);
        insert_run(&conn, "recipes/b", 5);
        insert_run(&conn, "recipes/a", 2);

        let res = list_runs(&path, 10, Some("recipes/a")).unwrap();
        assert_eq!(res.runs.len(), 2);
        assert!(res.runs.iter().all(|r| r.recipe == "recipes/a"));
    }

    #[test]
    fn pass_pow_k_matches_python_definition() {
        // p = 3/4 = 0.75, k = 2 => 0.5625
        assert!((pass_pow_k(3, 4, 2).unwrap() - 0.5625).abs() < 1e-9);
        // total = 0 => no data
        assert_eq!(pass_pow_k(0, 0, 3), None);
    }

    #[test]
    fn list_runs_includes_headline_pass_pow_k() {
        let dir = TempDir::new().unwrap();
        let path = make_store(&dir);
        let conn = Connection::open(&path).unwrap();
        let run_id = insert_run(&conn, "recipes/test", 2);
        insert_trial(&conn, run_id, "t1", 0, true, r#"{"axis":"a"}"#);
        insert_trial(&conn, run_id, "t1", 1, false, r#"{"axis":"a"}"#);
        insert_trial(&conn, run_id, "t2", 0, true, r#"{"axis":"b"}"#);
        insert_trial(&conn, run_id, "t2", 1, true, r#"{"axis":"b"}"#);
        // p = 3/4 = 0.75, k = 2 => pass^k = 0.5625

        let res = list_runs(&path, 10, None).unwrap();
        let run = &res.runs[0];
        assert_eq!(run.n_trials, 4);
        let pow_k = run
            .pass_pow_k
            .expect("pass_pow_k populated for run with trials");
        assert!((pow_k - 0.5625).abs() < 1e-9);
    }

    #[test]
    fn list_runs_pass_pow_k_none_when_run_has_no_trials() {
        let dir = TempDir::new().unwrap();
        let path = make_store(&dir);
        let conn = Connection::open(&path).unwrap();
        insert_run(&conn, "recipes/empty", 3);

        let res = list_runs(&path, 10, None).unwrap();
        assert_eq!(res.runs[0].pass_pow_k, None);
    }

    #[test]
    fn get_run_detail_returns_trials_ordered_and_axes_parsed() {
        let dir = TempDir::new().unwrap();
        let path = make_store(&dir);
        let conn = Connection::open(&path).unwrap();
        let run_id = insert_run(&conn, "recipes/test", 1);
        insert_trial(&conn, run_id, "t2", 0, false, r#"{"complexity":"high"}"#);
        insert_trial(&conn, run_id, "t1", 1, true, r#"{"complexity":"low"}"#);
        insert_trial(&conn, run_id, "t1", 0, true, r#"{"complexity":"low"}"#);

        let detail = get_run_detail(&path, run_id).unwrap().unwrap();
        assert_eq!(detail.trials.len(), 3);
        // ordered by (task_id, trial_index)
        assert_eq!(detail.trials[0].task_id, "t1");
        assert_eq!(detail.trials[0].trial_index, 0);
        assert_eq!(detail.trials[1].task_id, "t1");
        assert_eq!(detail.trials[1].trial_index, 1);
        assert_eq!(detail.trials[2].task_id, "t2");
        assert_eq!(
            detail.trials[0].axes.get("complexity").unwrap(),
            &serde_json::json!("low")
        );
    }

    #[test]
    fn get_run_detail_returns_none_when_run_id_missing() {
        let dir = TempDir::new().unwrap();
        let path = make_store(&dir);
        let res = get_run_detail(&path, 999).unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn get_run_detail_returns_none_when_store_absent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("missing.sqlite");
        let res = get_run_detail(&path, 1).unwrap();
        assert!(res.is_none());
    }

    #[test]
    fn resolve_store_path_uses_override_when_provided() {
        let resolved = resolve_store_path(Some("/tmp/x.sqlite")).unwrap();
        assert_eq!(resolved, PathBuf::from("/tmp/x.sqlite"));
    }

    #[test]
    fn resolve_store_path_empty_string_falls_back_to_default() {
        let resolved = resolve_store_path(Some("")).unwrap();
        assert!(resolved.to_string_lossy().ends_with(DEFAULT_STORE_SUBPATH));
    }
}
