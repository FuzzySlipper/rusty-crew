use super::*;

#[napi_derive::napi]
impl NativeBridgeBinding {
    #[napi]
    pub fn register_scheduled_wake_job_json(
        &self,
        job_id: String,
        target_session_id: String,
        interval_ms: Option<f64>,
        first_due_at: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let job = bridge
            .register_scheduled_wake_job(
                job_id,
                SessionId::new(target_session_id),
                interval_ms.map(|value| value as u64),
                first_due_at,
            )
            .map_err(to_napi_error)?;
        serde_json::to_string(&job)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn register_scheduled_host_job_json(
        &self,
        job_id: String,
        job_kind: String,
        interval_ms: Option<f64>,
        first_due_at: String,
        payload_json: String,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let payload_json = serde_json::from_str(&payload_json)
            .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))?;
        let job = bridge
            .register_scheduled_host_job(
                job_id,
                job_kind,
                interval_ms.map(|value| value as u64),
                first_due_at,
                payload_json,
            )
            .map_err(to_napi_error)?;
        serde_json::to_string(&job)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn list_scheduled_jobs_json(
        &self,
        status: Option<String>,
        job_kind: Option<String>,
        limit: Option<f64>,
        offset: Option<f64>,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let jobs = bridge
            .list_scheduled_jobs(
                status,
                job_kind,
                limit.map(|value| value as u32),
                offset.map(|value| value as u32),
            )
            .map_err(to_napi_error)?;
        serde_json::to_string(&jobs)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn list_scheduled_runs_json(
        &self,
        job_id: Option<String>,
        status: Option<String>,
        trigger: Option<String>,
        target_session_id: Option<String>,
        limit: Option<f64>,
        offset: Option<f64>,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let runs = bridge
            .list_scheduled_runs(
                job_id,
                status,
                trigger,
                target_session_id,
                limit.map(|value| value as u32),
                offset.map(|value| value as u32),
            )
            .map_err(to_napi_error)?;
        serde_json::to_string(&runs)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn claim_scheduled_host_runs_json(
        &self,
        supported_job_kinds: Vec<String>,
        limit: Option<f64>,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let runs = bridge
            .claim_scheduled_host_runs(supported_job_kinds, limit.map(|value| value as u32))
            .map_err(to_napi_error)?;
        serde_json::to_string(&runs)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn request_scheduled_host_job_run_json(
        &self,
        job_id: String,
        supported_job_kinds: Vec<String>,
    ) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let run = bridge
            .request_scheduled_host_job_run(job_id, supported_job_kinds)
            .map_err(to_napi_error)?;
        serde_json::to_string(&run)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn complete_scheduled_host_run(
        &self,
        run_id: String,
        status: String,
        output_json: String,
        error: Option<String>,
    ) -> napi::Result<()> {
        let bridge = self.bridge()?;
        let output_json = serde_json::from_str(&output_json)
            .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))?;
        bridge
            .complete_scheduled_host_run(
                rusty_crew_core_bridge_api::RunId::new(run_id),
                status,
                output_json,
                error,
            )
            .map_err(to_napi_error)?;
        Ok(())
    }

    #[napi]
    pub fn run_scheduler_tick_json(&self) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let report = bridge.run_scheduler_tick().map_err(to_napi_error)?;
        serde_json::to_string(&report)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn request_scheduled_job_run_json(&self, job_id: String) -> napi::Result<String> {
        let bridge = self.bridge()?;
        let run = bridge
            .request_scheduled_job_run(job_id)
            .map_err(to_napi_error)?;
        serde_json::to_string(&run)
            .map_err(|error| napi::Error::new(napi::Status::GenericFailure, error.to_string()))
    }

    #[napi]
    pub fn pause_scheduled_job(&self, job_id: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        bridge.pause_scheduled_job(job_id).map_err(to_napi_error)?;
        Ok(())
    }

    #[napi]
    pub fn resume_scheduled_job(&self, job_id: String, next_due_at: String) -> napi::Result<()> {
        let bridge = self.bridge()?;
        bridge
            .resume_scheduled_job(job_id, next_due_at)
            .map_err(to_napi_error)?;
        Ok(())
    }
}
