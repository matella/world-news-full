//! Explicit bounded intake orchestration. Web startup never constructs/runs it.
use crate::{
    content::valid_id,
    error::AppError,
    intake::{
        extraction_jobs::{ExtractionOutcome, ExtractionQueue},
        http::FetchError,
        jobs::{Queue, WorkOutcome},
        poll::{FeedPoller, PollOutcome},
    },
};
use serde::Serialize;
use sqlx::PgPool;
use std::{collections::BTreeSet, time::Duration};
use url::Url;
#[derive(Clone)]
pub struct WorkerPolicy {
    feeds: Vec<String>,
    extraction: bool,
    normalizations: u32,
    extractions: u32,
    budget: Duration,
}
impl WorkerPolicy {
    pub fn from_env() -> Result<Self, &'static str> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }
    pub fn from_lookup(mut get: impl FnMut(&str) -> Option<String>) -> Result<Self, &'static str> {
        let raw = get("NEWS_WORKER_FEEDS").unwrap_or_else(|| "[]".into());
        if raw.len() > 65536 {
            return Err("Worker feed list exceeds its input bound.");
        }
        let feeds: Vec<String> = serde_json::from_str(&raw)
            .map_err(|_| "Worker feeds must be a JSON list of opaque IDs.")?;
        if feeds.len() > 16
            || feeds.iter().any(|id| !valid_id(id))
            || feeds.iter().collect::<BTreeSet<_>>().len() != feeds.len()
        {
            return Err("Worker feed list must contain at most 16 unique opaque IDs.");
        }
        let extraction = match get("NEWS_WORKER_EXTRACTION_ENABLED").as_deref() {
            None | Some("false") => false,
            Some("true") => true,
            _ => return Err("Worker extraction must be true or false."),
        };
        let normalizations = number(get("NEWS_WORKER_MAX_NORMALIZATIONS"), 5, 0, 10)?;
        let extractions = number(get("NEWS_WORKER_MAX_EXTRACTIONS"), 1, 0, 10)?;
        let budget = number(get("NEWS_WORKER_BUDGET_SECONDS"), 60, 1, 300)?;
        Ok(Self {
            feeds,
            extraction,
            normalizations,
            extractions,
            budget: Duration::from_secs(u64::from(budget)),
        })
    }
}
fn number(raw: Option<String>, default: u32, min: u32, max: u32) -> Result<u32, &'static str> {
    let n = raw
        .map(|v| v.parse::<u32>())
        .transpose()
        .map_err(|_| "Invalid worker numeric limit.")?
        .unwrap_or(default);
    if !(min..=max).contains(&n) {
        return Err("Worker numeric limit is outside its supported bounds.");
    }
    Ok(n)
}
#[derive(Default, Serialize, Debug)]
pub struct StageCounts {
    pub calls: u32,
    pub completed: u32,
    pub deferred: u32,
    pub failed: u32,
    pub cancelled: u32,
    pub lease_lost: u32,
    pub skipped: u32,
    pub idle: u32,
    pub errors: u32,
    pub recovered: u32,
}
#[derive(Clone, Copy, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Recovery,
    Feeds,
    Normalization,
    Extraction,
}
#[derive(Serialize, Debug)]
pub struct WorkerError {
    pub stage: Stage,
    pub category: &'static str,
}
#[derive(Serialize, Debug)]
pub struct WorkerReport {
    pub stages_finished: bool,
    pub interrupted: bool,
    pub active_stage: Option<Stage>,
    pub extraction_enabled: bool,
    pub recovered_feeds: usize,
    pub feeds: StageCounts,
    pub normalization: StageCounts,
    pub extraction: StageCounts,
    pub errors: Vec<WorkerError>,
}
impl WorkerReport {
    pub fn succeeded(&self) -> bool {
        self.stages_finished
            && !self.interrupted
            && self.errors.is_empty()
            && self.feeds.failed == 0
            && self.normalization.failed == 0
            && self.extraction.failed == 0
    }
    fn error(&mut self, stage: Stage, error: AppError) {
        let category = match error {
            AppError::NotFound => "not_found",
            AppError::Validation(_) => "constraint",
            AppError::Conflict => "conflict",
            AppError::Forbidden => "forbidden",
            AppError::Unavailable => "unavailable",
            AppError::InvalidData => "invalid_data",
        };
        self.errors.push(WorkerError { stage, category });
    }
}
#[derive(Clone)]
pub struct IntakeWorker {
    feeds: FeedPoller,
    normalizations: Queue,
    extractions: ExtractionQueue,
}
impl IntakeWorker {
    pub fn new(pool: PgPool, public_url: &Url) -> Result<Self, FetchError> {
        Ok(Self {
            feeds: FeedPoller::new(pool.clone(), public_url)?,
            normalizations: Queue::new(pool.clone()),
            extractions: ExtractionQueue::new(pool, public_url)?,
        })
    }
    /// Trusted test/policy-aware injection only, never a user-controlled URL escape.
    pub fn with_services(
        feeds: FeedPoller,
        normalizations: Queue,
        extractions: ExtractionQueue,
    ) -> Self {
        Self {
            feeds,
            normalizations,
            extractions,
        }
    }
    pub async fn run_once(&self, policy: &WorkerPolicy) -> WorkerReport {
        let mut report = WorkerReport {
            stages_finished: false,
            interrupted: false,
            active_stage: None,
            extraction_enabled: policy.extraction,
            recovered_feeds: 0,
            feeds: StageCounts::default(),
            normalization: StageCounts::default(),
            extraction: StageCounts::default(),
            errors: vec![],
        };
        if tokio::time::timeout(policy.budget, self.run_stages(policy, &mut report))
            .await
            .is_err()
        {
            report.interrupted = true;
        }
        report
    }
    async fn run_stages(&self, policy: &WorkerPolicy, report: &mut WorkerReport) {
        report.active_stage = Some(Stage::Recovery);
        match self.feeds.recover_expired().await {
            Ok(n) => report.recovered_feeds = n,
            Err(e) => {
                report.error(Stage::Recovery, e);
                return;
            }
        }
        for id in &policy.feeds {
            report.active_stage = Some(Stage::Feeds);
            report.feeds.calls += 1;
            match self.feeds.poll_due(id).await {
                Ok(PollOutcome::Modified { .. } | PollOutcome::Unchanged { .. }) => {
                    report.feeds.completed += 1
                }
                Ok(PollOutcome::Failed { .. }) => report.feeds.failed += 1,
                Ok(PollOutcome::Cancelled) => report.feeds.cancelled += 1,
                Ok(PollOutcome::LeaseLost) => report.feeds.lease_lost += 1,
                Ok(PollOutcome::Recovered { .. }) => report.feeds.recovered += 1,
                Ok(PollOutcome::Disabled | PollOutcome::Paused | PollOutcome::NotDue) => {
                    report.feeds.skipped += 1
                }
                Err(e) => {
                    report.feeds.errors += 1;
                    report.error(Stage::Feeds, e);
                    return;
                }
            }
        }
        for _ in 0..policy.normalizations {
            report.active_stage = Some(Stage::Normalization);
            report.normalization.calls += 1;
            match self.normalizations.work_once().await {
                Ok(WorkOutcome::Idle) => {
                    report.normalization.idle += 1;
                    break;
                }
                Ok(WorkOutcome::Completed { .. }) => report.normalization.completed += 1,
                Ok(WorkOutcome::Deferred { .. }) => report.normalization.deferred += 1,
                Ok(WorkOutcome::Failed { .. }) => report.normalization.failed += 1,
                Ok(WorkOutcome::LostLease { .. }) => report.normalization.lease_lost += 1,
                Err(e) => {
                    report.normalization.errors += 1;
                    report.error(Stage::Normalization, e);
                    return;
                }
            }
        }
        if policy.extraction {
            for _ in 0..policy.extractions {
                report.active_stage = Some(Stage::Extraction);
                report.extraction.calls += 1;
                match self.extractions.work_once().await {
                    Ok(ExtractionOutcome::Idle) => {
                        report.extraction.idle += 1;
                        break;
                    }
                    Ok(ExtractionOutcome::Completed { .. }) => report.extraction.completed += 1,
                    Ok(ExtractionOutcome::Deferred { .. }) => report.extraction.deferred += 1,
                    Ok(ExtractionOutcome::Failed { .. }) => report.extraction.failed += 1,
                    Ok(ExtractionOutcome::Cancelled { .. }) => report.extraction.cancelled += 1,
                    Ok(ExtractionOutcome::LostLease { .. }) => report.extraction.lease_lost += 1,
                    Err(e) => {
                        report.extraction.errors += 1;
                        report.error(Stage::Extraction, e);
                        return;
                    }
                }
            }
        }
        report.active_stage = None;
        report.stages_finished = true;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn policy(pairs: &[(&str, &str)]) -> Result<WorkerPolicy, &'static str> {
        WorkerPolicy::from_lookup(|key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        })
    }
    #[test]
    fn worker_defaults_disable_source_fetches_and_limit_work() {
        let p = policy(&[]).unwrap();
        assert!(p.feeds.is_empty());
        assert!(!p.extraction);
        assert_eq!(p.normalizations, 5);
        assert_eq!(p.extractions, 1);
        assert_eq!(p.budget, Duration::from_secs(60));
    }
    #[test]
    fn worker_policy_rejects_unsafe_or_unbounded_inputs() {
        for pairs in [
            vec![("NEWS_WORKER_FEEDS", "https://private.invalid/")],
            vec![("NEWS_WORKER_FEEDS", r#"["one","","two"]"#)],
            vec![("NEWS_WORKER_FEEDS", r#"["one","one"]"#)],
            vec![("NEWS_WORKER_EXTRACTION_ENABLED", "TRUE")],
            vec![("NEWS_WORKER_BUDGET_SECONDS", "0")],
            vec![("NEWS_WORKER_BUDGET_SECONDS", "301")],
            vec![("NEWS_WORKER_MAX_NORMALIZATIONS", "11")],
            vec![("NEWS_WORKER_MAX_EXTRACTIONS", "-1")],
        ] {
            assert!(policy(&pairs).is_err());
        }
        assert!(
            policy(&[
                ("NEWS_WORKER_FEEDS", r#"["one","two"]"#),
                ("NEWS_WORKER_MAX_NORMALIZATIONS", "0")
            ])
            .is_ok()
        );
        let too_many =
            serde_json::to_string(&(0..17).map(|n| format!("f{n}")).collect::<Vec<_>>()).unwrap();
        assert!(policy(&[("NEWS_WORKER_FEEDS", &too_many)]).is_err());
        let exact = policy(&[("NEWS_WORKER_FEEDS", r#"["opaque,with,commas"]"#)]).unwrap();
        assert_eq!(exact.feeds, ["opaque,with,commas"]);
    }
}
