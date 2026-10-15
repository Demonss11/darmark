use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::plugins::manager::{PluginManager, PluginRuntime};

use super::common::{child_exe, fixture, serial, services, store_with_doc, view_contrib};

mod metrics;

use std::collections::HashMap;

use crate::plugins::resource::{
    ChildMetrics, MainMetrics, ProcessMetrics, RawMetrics, ResourceMonitor, ResourceSink,
    ResourceSnapshot,
};
use crate::plugins::PluginStatus;

/// Бюджет Job Object по умолчанию (64 МБ, `SupervisorParams::new`).
const LIMIT: usize = 64 * 1024 * 1024;

/// Фейковый провайдер: «сырые» метрики по PID без реальных процессов (шов ADR-0031 §2.6).
struct FakeProvider {
    map: HashMap<u32, RawMetrics>,
}

impl FakeProvider {
    fn new(pairs: Vec<(u32, RawMetrics)>) -> Self {
        Self {
            map: pairs.into_iter().collect(),
        }
    }
}

impl ProcessMetrics for FakeProvider {
    fn read(&self, pid: u32) -> Option<RawMetrics> {
        self.map.get(&pid).copied()
    }
}

/// Фейковые сырые метрики: `private_working_set_bytes` совпадает с рабочим набором
/// (фолбэк-эквивалент), если тесту не нужно различать их явно.
fn raw(working_set: usize, private: usize) -> RawMetrics {
    RawMetrics {
        working_set_bytes: working_set,
        private_bytes: private,
        private_working_set_bytes: working_set,
        cpu_percent: None,
        handles: None,
    }
}

/// Записывающий sink: реальные снимки и события завершения (по образцу sink-тестов S11).
#[derive(Default)]
struct RecordingSink {
    metrics: Mutex<Vec<ResourceSnapshot>>,
    exited: Mutex<Vec<(String, u32, Option<String>)>>,
}

impl ResourceSink for RecordingSink {
    fn metrics(&self, snapshot: &ResourceSnapshot) {
        self.metrics.lock().unwrap().push(snapshot.clone());
    }

    fn process_exited(&self, plugin_id: &str, pid: u32, reason: Option<&str>) {
        self.exited
            .lock()
            .unwrap()
            .push((plugin_id.to_string(), pid, reason.map(str::to_string)));
    }
}
