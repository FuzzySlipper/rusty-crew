use crate::{CoreError, CoreErrorKind, CoreResult, RuntimeStorageConnectionHealth};
use postgres::{Client, NoTls};
use std::ops::{Deref, DerefMut};
use std::sync::Mutex;

use super::{DEFAULT_POSTGRES_POOL_SIZE, MAX_POSTGRES_POOL_SIZE};

pub(super) struct PostgresConnectionPool {
    database_url: String,
    state: Mutex<PostgresConnectionPoolState>,
}

struct PostgresConnectionPoolState {
    idle: Vec<Client>,
    active_connections: usize,
    max_connections: usize,
    total_opened: u64,
    checkout_count: u64,
    checkout_reuse_count: u64,
    reconnect_attempts: u64,
    reconnect_successes: u64,
    closed_connections_discarded: u64,
    last_error: Option<String>,
}

pub(super) struct PostgresClientLease<'a> {
    pool: &'a PostgresConnectionPool,
    client: Option<Client>,
}

impl PostgresConnectionPool {
    pub(super) fn new(database_url: &str, max_connections: Option<u32>) -> Self {
        let max_connections = max_connections
            .map(|value| value.max(1) as usize)
            .unwrap_or(DEFAULT_POSTGRES_POOL_SIZE)
            .min(MAX_POSTGRES_POOL_SIZE);
        Self {
            database_url: database_url.to_string(),
            state: Mutex::new(PostgresConnectionPoolState {
                idle: Vec::new(),
                active_connections: 0,
                max_connections,
                total_opened: 0,
                checkout_count: 0,
                checkout_reuse_count: 0,
                reconnect_attempts: 0,
                reconnect_successes: 0,
                closed_connections_discarded: 0,
                last_error: None,
            }),
        }
    }

    pub(super) fn checkout(&self) -> CoreResult<PostgresClientLease<'_>> {
        loop {
            let mut state = self.state.lock().map_err(|_| {
                CoreError::new(
                    CoreErrorKind::PersistenceFailure,
                    "PostgreSQL durable backend connection pool mutex poisoned",
                )
            })?;
            state.checkout_count += 1;
            if let Some(client) = state.idle.pop() {
                if client.is_closed() {
                    state.closed_connections_discarded += 1;
                    state.last_error =
                        Some("discarded closed idle PostgreSQL connection".to_string());
                    continue;
                }
                state.checkout_reuse_count += 1;
                state.active_connections += 1;
                return Ok(PostgresClientLease {
                    pool: self,
                    client: Some(client),
                });
            }

            if state.active_connections >= state.max_connections {
                let error = format!(
                    "transient PostgreSQL connection pool exhausted: active={} max={}",
                    state.active_connections, state.max_connections
                );
                state.last_error = Some(error.clone());
                return Err(CoreError::new(CoreErrorKind::PersistenceFailure, error));
            }

            state.active_connections += 1;
            state.reconnect_attempts += 1;
            break;
        }

        match Client::connect(&self.database_url, NoTls) {
            Ok(client) => {
                let mut state = self.state.lock().map_err(|_| {
                    CoreError::new(
                        CoreErrorKind::PersistenceFailure,
                        "PostgreSQL durable backend connection pool mutex poisoned",
                    )
                })?;
                state.total_opened += 1;
                state.reconnect_successes += 1;
                state.last_error = None;
                drop(state);
                Ok(PostgresClientLease {
                    pool: self,
                    client: Some(client),
                })
            }
            Err(error) => {
                let mut state = self.state.lock().map_err(|_| {
                    CoreError::new(
                        CoreErrorKind::PersistenceFailure,
                        "PostgreSQL durable backend connection pool mutex poisoned",
                    )
                })?;
                state.active_connections = state.active_connections.saturating_sub(1);
                let message = format!("transient PostgreSQL connection failure: {error}");
                state.last_error = Some(message.clone());
                Err(CoreError::new(CoreErrorKind::PersistenceFailure, message))
            }
        }
    }

    pub(super) fn health(&self) -> CoreResult<RuntimeStorageConnectionHealth> {
        let state = self.state.lock().map_err(|_| {
            CoreError::new(
                CoreErrorKind::PersistenceFailure,
                "PostgreSQL durable backend connection pool mutex poisoned",
            )
        })?;
        let status = if state.active_connections >= state.max_connections && state.idle.is_empty() {
            "exhausted"
        } else if state.last_error.is_some() {
            "degraded"
        } else {
            "healthy"
        };
        Ok(RuntimeStorageConnectionHealth {
            backend: "postgres".to_string(),
            status: status.to_string(),
            max_connections: state.max_connections as u32,
            active_connections: state.active_connections as u32,
            idle_connections: state.idle.len() as u32,
            total_opened: state.total_opened,
            checkout_count: state.checkout_count,
            checkout_reuse_count: state.checkout_reuse_count,
            reconnect_attempts: state.reconnect_attempts,
            reconnect_successes: state.reconnect_successes,
            closed_connections_discarded: state.closed_connections_discarded,
            last_error: state.last_error.clone(),
        })
    }
}

impl Drop for PostgresClientLease<'_> {
    fn drop(&mut self) {
        let Some(client) = self.client.take() else {
            return;
        };
        let Ok(mut state) = self.pool.state.lock() else {
            return;
        };
        state.active_connections = state.active_connections.saturating_sub(1);
        if client.is_closed() {
            state.closed_connections_discarded += 1;
            state.last_error = Some("discarded closed PostgreSQL connection".to_string());
            return;
        }
        if state.idle.len() < state.max_connections {
            state.idle.push(client);
        }
    }
}

impl Deref for PostgresClientLease<'_> {
    type Target = Client;

    fn deref(&self) -> &Self::Target {
        self.client
            .as_ref()
            .expect("PostgreSQL client lease always contains a client until drop")
    }
}

impl DerefMut for PostgresClientLease<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.client
            .as_mut()
            .expect("PostgreSQL client lease always contains a client until drop")
    }
}
