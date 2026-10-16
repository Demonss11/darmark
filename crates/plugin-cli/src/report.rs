//! Модель отчёта команды `test` и его печать (текст/JSON).

use std::path::PathBuf;



pub use plugin_core::REPORT_SCHEMA_VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Fail,
}

#[derive(Debug, Clone)]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
}

impl Check {
    pub fn ok(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Ok,
            message: message.into(),
        }
    }

    pub fn fail(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Fail,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestReport {
    pub path: PathBuf,
    pub checks: Vec<Check>,
    pub sandbox_logs: Vec<String>,
}

impl TestReport {
    pub fn ok(&self) -> bool {
        self.checks.iter().all(|c| c.status == CheckStatus::Ok)
    }

    pub fn failed(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .count()
    }
}

pub fn print_report_text(report: &TestReport) {
    for check in &report.checks {
        let status = match check.status {
            CheckStatus::Ok => "OK",
            CheckStatus::Fail => "FAIL",
        };
        println!("[{status}] {}: {}", check.name, check.message);
    }
    if !report.sandbox_logs.is_empty() {
        println!("Лог песочницы:");
        for line in &report.sandbox_logs {
            println!("  {line}");
        }
    }
    println!(
        "Итого: {} проверок, ошибок {}",
        report.checks.len(),
        report.failed()
    );
}


pub fn report_to_json(report: &TestReport) -> serde_json::Value {
    let checks: Vec<serde_json::Value> = report
        .checks
        .iter()
        .map(|check| {
            serde_json::json!({
                "name": check.name,
                "status": match check.status {
                    CheckStatus::Ok => "ok",
                    CheckStatus::Fail => "fail",
                },
                "message": check.message,
            })
        })
        .collect();
    serde_json::json!({
        "schema_version": REPORT_SCHEMA_VERSION,
        "path": report.path.to_string_lossy(),
        "ok": report.ok(),
        "checks": checks,
        "sandbox_logs": report.sandbox_logs,
    })
}

pub fn print_report_json(report: &TestReport) {
    match serde_json::to_string_pretty(&report_to_json(report)) {
        Ok(text) => println!("{text}"),
        Err(error) => eprintln!("ошибка сериализации JSON: {error}"),
    }
}
