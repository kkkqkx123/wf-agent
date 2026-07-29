// 导出辅助模块
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BenchmarkResult {
    pub name: String,
    pub mean_ns: f64,
    pub std_dev_ns: f64,
    pub median_ns: f64,
    pub min_ns: f64,
    pub max_ns: f64,
    pub iterations: u64,
    pub sample_size: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BenchmarkGroup {
    pub group_name: String,
    pub benchmarks: Vec<BenchmarkResult>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub timestamp: String,
    pub groups: Vec<BenchmarkGroup>,
}

impl Default for BenchmarkReport {
    fn default() -> Self {
        Self::new()
    }
}

impl BenchmarkReport {
    pub fn new() -> Self {
        BenchmarkReport {
            timestamp: chrono::Utc::now().to_rfc3339(),
            groups: Vec::new(),
        }
    }

    pub fn add_group(&mut self, group: BenchmarkGroup) {
        self.groups.push(group);
    }

    pub fn save_to_file(&self, filename: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(filename, json)?;
        Ok(())
    }
}

pub struct BenchmarkCollector {
    pub report: BenchmarkReport,
    pub current_group: Option<String>,
    pub current_benchmarks: Vec<BenchmarkResult>,
}

impl Default for BenchmarkCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl BenchmarkCollector {
    pub fn new() -> Self {
        BenchmarkCollector {
            report: BenchmarkReport::new(),
            current_group: None,
            current_benchmarks: Vec::new(),
        }
    }

    pub fn start_group(&mut self, group_name: &str) {
        if let Some(group) = self.current_group.take() {
            if !self.current_benchmarks.is_empty() {
                self.report.add_group(BenchmarkGroup {
                    group_name: group,
                    benchmarks: std::mem::take(&mut self.current_benchmarks),
                });
            }
        }
        self.current_group = Some(group_name.to_string());
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_benchmark(
        &mut self,
        name: String,
        mean_ns: f64,
        std_dev_ns: f64,
        median_ns: f64,
        min_ns: f64,
        max_ns: f64,
        iterations: u64,
        sample_size: usize,
    ) {
        self.current_benchmarks.push(BenchmarkResult {
            name,
            mean_ns,
            std_dev_ns,
            median_ns,
            min_ns,
            max_ns,
            iterations,
            sample_size,
        });
    }

    pub fn finalize(&mut self) {
        if let Some(group) = self.current_group.take() {
            if !self.current_benchmarks.is_empty() {
                self.report.add_group(BenchmarkGroup {
                    group_name: group,
                    benchmarks: std::mem::take(&mut self.current_benchmarks),
                });
            }
        }
    }

    pub fn save(&self, filename: &str) -> std::io::Result<()> {
        self.report.save_to_file(filename)
    }
}

lazy_static::lazy_static! {
    pub static ref GLOBAL_COLLECTOR: std::sync::Mutex<BenchmarkCollector> =
        std::sync::Mutex::new(BenchmarkCollector::new());
}

pub fn save_benchmark_results(benchmark_name: &str) -> std::io::Result<()> {
    let mut collector = GLOBAL_COLLECTOR.lock().unwrap();
    collector.finalize();
    let filename = format!("benches/results/{}.json", benchmark_name);
    collector.save(&filename)
}

pub fn create_benchmark_summary(benchmark_name: &str) -> String {
    let results_path = format!("benches/results/{}.json", benchmark_name);
    if let Ok(content) = fs::read_to_string(&results_path) {
        if let Ok(report) = serde_json::from_str::<BenchmarkReport>(&content) {
            let mut summary = String::new();
            summary.push_str(&format!("Benchmark Report: {}\n", benchmark_name));
            summary.push_str(&format!("Timestamp: {}\n\n", report.timestamp));

            for group in &report.groups {
                summary.push_str(&format!("## Group: {}\n", group.group_name));
                for bench in &group.benchmarks {
                    summary.push_str(&format!(
                        "- {}: Mean={:.2}ns, Median={:.2}ns, Samples={}\n",
                        bench.name, bench.mean_ns, bench.median_ns, bench.sample_size
                    ));
                }
                summary.push('\n');
            }
            summary
        } else {
            format!("Failed to parse benchmark results from {}", results_path)
        }
    } else {
        format!("No benchmark results found in {}", results_path)
    }
}
