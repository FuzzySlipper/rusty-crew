use rusty_crew_core_bridge_api::{
    BrainEvent, BrainEventEnvelope, BrainImplementationHandle, BrainWakeBufferInput, ClockConfig,
    EngineConfig, SessionId, ShutdownRequest,
};
use rusty_crew_core_bridge_node::NativeBridge;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

const DEFAULT_EVENT_COUNT: usize = 20_000;
const DEFAULT_BATCH_SIZES: &str = "1,16,64";
const DEFAULT_TEXT_BYTES: usize = 64;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_count = env_usize("RUSTY_CREW_THROUGHPUT_EVENTS", DEFAULT_EVENT_COUNT);
    let text_bytes = env_usize("RUSTY_CREW_THROUGHPUT_TEXT_BYTES", DEFAULT_TEXT_BYTES);
    let batch_sizes = env_batch_sizes("RUSTY_CREW_THROUGHPUT_BATCHES", DEFAULT_BATCH_SIZES);

    let mut cases = Vec::new();
    for batch_size in batch_sizes {
        cases.push(run_case(event_count, text_bytes, batch_size)?);
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "label": "pre-napi native-facade throughput",
            "warning": "These measurements do not include Node/Rust napi crossing overhead.",
            "event_count": event_count,
            "text_bytes": text_bytes,
            "cases": cases,
        }))?
    );
    Ok(())
}

fn run_case(
    event_count: usize,
    text_bytes: usize,
    producer_batch_size: usize,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut bridge = NativeBridge::new();
    let data_dir = benchmark_data_dir(producer_batch_size);
    let _ = fs::remove_dir_all(&data_dir);
    let engine = bridge.initialize_engine(EngineConfig {
        engine_data_dir: data_dir.to_string_lossy().to_string(),
        clock: ClockConfig::Fixed {
            at: "2026-06-19T00:00:00Z".to_string(),
        },
        default_turn_budget: 3,
        default_idle_timeout_ms: 1000,
        storage: None,
    })?;

    let session_id = SessionId::new(format!("throughput-session-{producer_batch_size}"));
    let hydration_start = Instant::now();
    let buffered = bridge.build_brain_wake_request(BrainWakeBufferInput {
        brain: BrainImplementationHandle::new(1),
        session_id: session_id.clone(),
        body_state_json: large_json_bytes("body_state", 256 * 1024),
        system_prompt: "system prompt ".repeat(24 * 1024),
        role_assembly_json: large_json_bytes("role_assembly", 128 * 1024),
        wake_id: "throughput-wake".to_string(),
    })?;
    let body_state_bytes = bridge.get_buffer(buffered.request.body_state)?.byte_len;
    let system_prompt_bytes = bridge.get_buffer(buffered.request.system_prompt)?.byte_len;
    let role_assembly_bytes = bridge.get_buffer(buffered.request.role_assembly)?.byte_len;
    bridge.release_buffer(buffered.request.body_state)?;
    bridge.release_buffer(buffered.request.system_prompt)?;
    bridge.release_buffer(buffered.request.role_assembly)?;
    bridge.assert_no_buffer_leaks()?;
    let hydration_elapsed_ms = hydration_start.elapsed().as_secs_f64() * 1000.0;

    let payload_text = "x".repeat(text_bytes);
    let rss_before = rss_bytes();
    let cpu_before = process_cpu_ticks();
    let elapsed_start = Instant::now();
    let mut submit_latencies_ns = Vec::with_capacity(event_count);
    let mut batch_elapsed_ns = Vec::new();
    let mut submitted = 0usize;

    while submitted < event_count {
        let remaining = event_count - submitted;
        let current_batch = remaining.min(producer_batch_size);
        let batch_start = Instant::now();

        for _index in 0..current_batch {
            let submit_start = Instant::now();
            bridge.submit_brain_event(BrainEventEnvelope {
                wake_id: "throughput-wake".to_string(),
                session_id: session_id.clone(),
                event: BrainEvent::TextDelta {
                    text: payload_text.clone(),
                },
            })?;
            submit_latencies_ns.push(submit_start.elapsed().as_nanos() as u64);
        }

        batch_elapsed_ns.push(batch_start.elapsed().as_nanos() as u64);
        submitted += current_batch;
    }

    let elapsed = elapsed_start.elapsed();
    let rss_after = rss_bytes();
    let cpu_after = process_cpu_ticks();
    submit_latencies_ns.sort_unstable();
    batch_elapsed_ns.sort_unstable();

    bridge.shutdown_engine(ShutdownRequest {
        engine,
        drain_timeout_ms: 1_000,
    })?;
    let _ = fs::remove_dir_all(&data_dir);

    Ok(json!({
        "producer_batch_size": producer_batch_size,
        "batch_count": batch_elapsed_ns.len(),
        "events_per_second": event_count as f64 / elapsed.as_secs_f64(),
        "elapsed_ms": elapsed.as_secs_f64() * 1000.0,
        "submit_latency_ns": {
            "p50": percentile(&submit_latencies_ns, 0.50),
            "p95": percentile(&submit_latencies_ns, 0.95),
            "p99": percentile(&submit_latencies_ns, 0.99),
            "max": submit_latencies_ns.last().copied().unwrap_or(0),
        },
        "producer_batch_elapsed_ns": {
            "p50": percentile(&batch_elapsed_ns, 0.50),
            "p95": percentile(&batch_elapsed_ns, 0.95),
            "max": batch_elapsed_ns.last().copied().unwrap_or(0),
        },
        "wake_buffer_hydration": {
            "elapsed_ms": hydration_elapsed_ms,
            "body_state_bytes": body_state_bytes,
            "system_prompt_bytes": system_prompt_bytes,
            "role_assembly_bytes": role_assembly_bytes,
        },
        "memory": {
            "rss_before_bytes": rss_before,
            "rss_after_bytes": rss_after,
            "rss_delta_bytes": rss_delta(rss_before, rss_after),
            "source": "linux /proc/self/statm with 4096-byte page estimate",
        },
        "cpu": {
            "process_ticks_before": cpu_before,
            "process_ticks_after": cpu_after,
            "process_ticks_delta": tick_delta(cpu_before, cpu_after),
            "source": "linux /proc/self/stat utime+stime ticks",
        },
    }))
}

fn env_usize(name: &str, fallback: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn env_batch_sizes(name: &str, fallback: &str) -> Vec<usize> {
    let raw = std::env::var(name).unwrap_or_else(|_| fallback.to_string());
    let mut values = raw
        .split(',')
        .filter_map(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .collect::<Vec<_>>();
    values.sort_unstable();
    values.dedup();
    if values.is_empty() {
        vec![1]
    } else {
        values
    }
}

fn large_json_bytes(label: &str, target_bytes: usize) -> Vec<u8> {
    let repeated = "x".repeat(target_bytes);
    json!({
        "label": label,
        "payload": repeated,
    })
    .to_string()
    .into_bytes()
}

fn percentile(sorted: &[u64], percentile: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

fn benchmark_data_dir(batch_size: usize) -> PathBuf {
    std::env::temp_dir().join(format!(
        "rusty-crew-throughput-{}-{}",
        std::process::id(),
        batch_size
    ))
}

fn rss_bytes() -> Option<u64> {
    let statm = fs::read_to_string("/proc/self/statm").ok()?;
    let resident_pages = statm.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    Some(resident_pages * 4096)
}

fn process_cpu_ticks() -> Option<u64> {
    let stat = fs::read_to_string("/proc/self/stat").ok()?;
    let after_command = stat.rsplit_once(") ")?.1;
    let fields = after_command.split_whitespace().collect::<Vec<_>>();
    let user_ticks = fields.get(11)?.parse::<u64>().ok()?;
    let system_ticks = fields.get(12)?.parse::<u64>().ok()?;
    Some(user_ticks + system_ticks)
}

fn rss_delta(before: Option<u64>, after: Option<u64>) -> Option<i64> {
    Some(after? as i64 - before? as i64)
}

fn tick_delta(before: Option<u64>, after: Option<u64>) -> Option<i64> {
    Some(after? as i64 - before? as i64)
}
