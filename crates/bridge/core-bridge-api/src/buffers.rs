use crate::{
    BrainWakeRequest, CoreError, CoreErrorKind, CoreResult, RuntimeBufferHandle, RuntimeBufferView,
    SessionId,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const APPLICATION_JSON: &str = "application/json";
pub const TEXT_PLAIN: &str = "text/plain; charset=utf-8";

#[derive(Debug, Clone)]
pub struct RuntimeBufferStore {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    next_handle: AtomicU64,
    buffers: Mutex<HashMap<RuntimeBufferHandle, RuntimeBufferEntry>>,
}

#[derive(Debug, Clone)]
struct RuntimeBufferEntry {
    media_type: String,
    bytes: Vec<u8>,
    leases: u32,
}

#[derive(Debug, Clone)]
pub struct RuntimeBufferLease {
    handle: RuntimeBufferHandle,
    store: RuntimeBufferStore,
    released: bool,
}

#[derive(Debug, Clone)]
pub struct BrainWakeBufferInput {
    pub brain: crate::BrainImplementationHandle,
    pub session_id: SessionId,
    pub body_state_json: Vec<u8>,
    pub system_prompt: String,
    pub role_assembly_json: Vec<u8>,
    pub wake_id: String,
}

#[derive(Debug, Clone)]
pub struct BufferedBrainWakeRequest {
    pub request: BrainWakeRequest,
    pub leases: Vec<RuntimeBufferLease>,
}

impl RuntimeBufferStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                next_handle: AtomicU64::new(1),
                buffers: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn insert(
        &self,
        media_type: impl Into<String>,
        bytes: Vec<u8>,
    ) -> CoreResult<RuntimeBufferHandle> {
        if bytes.is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "runtime buffer cannot be empty",
            ));
        }

        let handle =
            RuntimeBufferHandle::new(self.inner.next_handle.fetch_add(1, Ordering::Relaxed));
        let entry = RuntimeBufferEntry {
            media_type: media_type.into(),
            bytes,
            leases: 0,
        };
        self.buffers()?.insert(handle, entry);
        Ok(handle)
    }

    pub fn insert_json<T: Serialize>(&self, value: &T) -> CoreResult<RuntimeBufferHandle> {
        let bytes = serde_json::to_vec(value).map_err(|error| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("serialize runtime buffer json: {error}"),
            )
        })?;
        self.insert(APPLICATION_JSON, bytes)
    }

    pub fn insert_text(&self, value: impl Into<String>) -> CoreResult<RuntimeBufferHandle> {
        self.insert(TEXT_PLAIN, value.into().into_bytes())
    }

    pub fn acquire(&self, handle: RuntimeBufferHandle) -> CoreResult<RuntimeBufferLease> {
        let mut buffers = self.buffers()?;
        let entry = buffers.get_mut(&handle).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("runtime buffer {handle:?} not found"),
            )
        })?;
        entry.leases = entry.leases.checked_add(1).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("runtime buffer {handle:?} lease count overflow"),
            )
        })?;
        Ok(RuntimeBufferLease {
            handle,
            store: self.clone(),
            released: false,
        })
    }

    pub fn get_buffer(&self, handle: RuntimeBufferHandle) -> CoreResult<RuntimeBufferView> {
        let buffers = self.buffers()?;
        let entry = buffers.get(&handle).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("runtime buffer {handle:?} not found"),
            )
        })?;
        if entry.leases == 0 {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("runtime buffer {handle:?} has no active lease"),
            ));
        }

        Ok(RuntimeBufferView {
            handle,
            media_type: entry.media_type.clone(),
            byte_len: entry.bytes.len() as u64,
            bytes: entry.bytes.clone(),
        })
    }

    pub fn release_buffer(&self, handle: RuntimeBufferHandle) -> CoreResult<()> {
        let mut buffers = self.buffers()?;
        let entry = buffers.get_mut(&handle).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("runtime buffer {handle:?} not found or already released"),
            )
        })?;
        if entry.leases == 0 {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("runtime buffer {handle:?} has no active lease"),
            ));
        }

        entry.leases -= 1;
        if entry.leases == 0 {
            buffers.remove(&handle);
        }
        Ok(())
    }

    pub fn assert_no_leaks(&self) -> CoreResult<()> {
        let leaked_handles = self
            .buffers()?
            .iter()
            .filter_map(|(handle, entry)| (entry.leases > 0).then_some(*handle))
            .collect::<Vec<_>>();

        if leaked_handles.is_empty() {
            return Ok(());
        }

        Err(CoreError::new(
            CoreErrorKind::InternalError,
            format!("leaked runtime buffer handles: {leaked_handles:?}"),
        ))
    }

    pub fn build_brain_wake_request(
        &self,
        input: BrainWakeBufferInput,
    ) -> CoreResult<BufferedBrainWakeRequest> {
        let body_state = self.insert(APPLICATION_JSON, input.body_state_json)?;
        let system_prompt = self.insert_text(input.system_prompt)?;
        let role_assembly = self.insert(APPLICATION_JSON, input.role_assembly_json)?;
        let leases = vec![
            self.acquire(body_state)?,
            self.acquire(system_prompt)?,
            self.acquire(role_assembly)?,
        ];

        Ok(BufferedBrainWakeRequest {
            request: BrainWakeRequest {
                brain: input.brain,
                session_id: input.session_id,
                body_state,
                system_prompt,
                role_assembly,
                wake_id: input.wake_id,
            },
            leases,
        })
    }

    fn buffers(
        &self,
    ) -> CoreResult<std::sync::MutexGuard<'_, HashMap<RuntimeBufferHandle, RuntimeBufferEntry>>>
    {
        self.inner.buffers.lock().map_err(|_| {
            CoreError::new(CoreErrorKind::InternalError, "runtime buffer lock poisoned")
        })
    }
}

impl Default for RuntimeBufferStore {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeBufferLease {
    pub fn handle(&self) -> RuntimeBufferHandle {
        self.handle
    }

    pub fn release(&mut self) -> CoreResult<()> {
        if self.released {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("runtime buffer {:?} lease already released", self.handle),
            ));
        }

        self.store.release_buffer(self.handle)?;
        self.released = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AgentId, BrainImplementationHandle};

    #[test]
    fn large_wake_payloads_cross_by_runtime_buffer_handle() {
        let store = RuntimeBufferStore::new();
        let body = format!(
            r#"{{"session":{{"agent":"planner"}},"messages":["{}"]}}"#,
            "body-state ".repeat(16_384)
        );
        let role = format!(r#"{{"instructions":"{}"}}"#, "role ".repeat(16_384));

        let buffered = store
            .build_brain_wake_request(BrainWakeBufferInput {
                brain: BrainImplementationHandle::new(1),
                session_id: SessionId::new("planner-session"),
                body_state_json: body.as_bytes().to_vec(),
                system_prompt: "system prompt ".repeat(16_384),
                role_assembly_json: role.as_bytes().to_vec(),
                wake_id: "wake-large".to_string(),
            })
            .unwrap();

        assert_ne!(buffered.request.body_state, buffered.request.system_prompt);
        assert_ne!(buffered.request.body_state, buffered.request.role_assembly);
        assert!(
            store
                .get_buffer(buffered.request.body_state)
                .unwrap()
                .byte_len
                > 100_000
        );
        assert!(
            store
                .get_buffer(buffered.request.role_assembly)
                .unwrap()
                .byte_len
                > 50_000
        );
        assert!(
            store
                .get_buffer(buffered.request.system_prompt)
                .unwrap()
                .byte_len
                > 100_000
        );

        for mut lease in buffered.leases {
            lease.release().unwrap();
        }
        store.assert_no_leaks().unwrap();
    }

    #[test]
    fn double_release_fails_loudly() {
        let store = RuntimeBufferStore::new();
        let handle = store.insert_text("hello").unwrap();
        let mut lease = store.acquire(handle).unwrap();

        lease.release().unwrap();
        let error = lease.release().expect_err("double release must fail");

        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    }

    #[test]
    fn missing_release_is_reported_as_a_leak() {
        let store = RuntimeBufferStore::new();
        let handle = store.insert_text("hello").unwrap();
        let _lease = store.acquire(handle).unwrap();

        let error = store
            .assert_no_leaks()
            .expect_err("active lease should be reported");

        assert_eq!(error.kind, CoreErrorKind::InternalError);
        assert!(error.message.contains("leaked runtime buffer handles"));
    }

    #[test]
    fn unleased_buffers_cannot_be_read_across_the_bridge() {
        let store = RuntimeBufferStore::new();
        let handle = store.insert_json(&AgentId::new("planner")).unwrap();

        let error = store
            .get_buffer(handle)
            .expect_err("unleased buffer should be inaccessible");

        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    }
}
