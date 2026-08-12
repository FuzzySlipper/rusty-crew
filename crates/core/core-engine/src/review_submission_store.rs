use rusty_crew_core_persistence::CoreCoordinationStore;
use rusty_crew_core_protocol::{CoreResult, ReviewSubmissionRecord};

pub(crate) fn load(
    store: &CoreCoordinationStore,
    submission_id: &str,
) -> CoreResult<Option<ReviewSubmissionRecord>> {
    store.get_review_submission(submission_id)
}

pub(crate) fn list(store: &CoreCoordinationStore) -> CoreResult<Vec<ReviewSubmissionRecord>> {
    store.list_review_submissions()
}

pub(crate) fn list_page(
    store: &CoreCoordinationStore,
    project_id: Option<&str>,
    limit: u32,
    offset: u64,
) -> CoreResult<Vec<ReviewSubmissionRecord>> {
    store.list_review_submissions_page(project_id, limit, offset)
}

pub(crate) fn save(
    store: &CoreCoordinationStore,
    record: &ReviewSubmissionRecord,
) -> CoreResult<()> {
    match store.get_review_submission(&record.submission_id)? {
        Some(existing) => {
            store.update_review_submission(record, existing.revision)?;
        }
        None => {
            store.insert_review_submission(record)?;
        }
    }
    Ok(())
}
