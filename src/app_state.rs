use crate::device::{CpuInfo, GpuInfo, MemoryInfo, ProcessInfo};
use crate::storage::info::StorageInfo;
use crate::ui::notification::NotificationManager;
use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct ConnectionStatus {
    pub hostname: String, // This is the server address key (e.g., "localhost:10001")
    #[allow(dead_code)]
    pub url: String,
    pub actual_hostname: Option<String>, // The real hostname from API (e.g., "node-0001")
    pub is_connected: bool,
    pub last_successful_connection: Option<Instant>,
    pub consecutive_failures: u32,
    pub last_error: Option<String>,
    pub last_update: Instant,
}

impl ConnectionStatus {
    pub fn new(hostname: String, url: String) -> Self {
        Self {
            hostname,
            url,
            actual_hostname: None,
            is_connected: false,
            last_successful_connection: None,
            consecutive_failures: 0,
            last_error: None,
            last_update: Instant::now(),
        }
    }

    pub fn mark_success(&mut self) {
        self.is_connected = true;
        self.last_successful_connection = Some(Instant::now());
        self.consecutive_failures = 0;
        self.last_error = None;
        self.last_update = Instant::now();
    }

    pub fn mark_failure(&mut self, error: String) {
        self.is_connected = false;
        self.consecutive_failures += 1;
        self.last_error = Some(error);
        self.last_update = Instant::now();
    }

    #[allow(dead_code)]
    pub fn is_recently_failed(&self) -> bool {
        !self.is_connected && self.last_update.elapsed() < Duration::from_secs(30)
    }

    #[allow(dead_code)]
    pub fn connection_duration(&self) -> Option<Duration> {
        self.last_successful_connection.map(|t| t.elapsed())
    }
}

#[derive(Clone)]
pub struct AppState {
    pub gpu_info: Vec<GpuInfo>,
    pub cpu_info: Vec<CpuInfo>,
    pub memory_info: Vec<MemoryInfo>,
    pub process_info: Vec<ProcessInfo>,
    pub selected_process_index: usize,
    pub start_index: usize,
    pub sort_criteria: SortCriteria,
    pub sort_direction: SortDirection,
    pub loading: bool,
    pub tabs: Vec<String>,
    pub current_tab: usize,
    pub gpu_scroll_offset: usize,
    pub storage_scroll_offset: usize,
    pub tab_scroll_offset: usize,
    pub process_horizontal_scroll_offset: usize,
    pub device_name_scroll_offsets: HashMap<String, usize>,
    pub hostname_scroll_offsets: HashMap<String, usize>,
    pub frame_counter: u64,
    pub storage_info: Vec<StorageInfo>,
    pub show_help: bool,
    pub utilization_history: VecDeque<f64>,
    pub memory_history: VecDeque<f64>,
    pub temperature_history: VecDeque<f64>,
    pub notifications: NotificationManager,
    pub nvml_notification_shown: bool,
    // Connection status tracking for remote mode
    pub connection_status: HashMap<String, ConnectionStatus>,
    pub known_hosts: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SortCriteria {
    // Process sorting (local mode only)
    Pid,            // Process ID
    User,           // User name
    Priority,       // Process priority (PRI)
    Nice,           // Nice value
    VirtualMemory,  // Virtual memory (VIRT)
    ResidentMemory, // Resident memory (RES)
    State,          // Process state
    CpuPercent,     // CPU usage percentage
    MemoryPercent,  // Memory usage percentage (was Memory)
    GpuPercent,     // GPU usage percentage
    GpuMemoryUsage, // GPU memory usage
    CpuTime,        // CPU time (TIME+)
    Command,        // Command line
    // GPU sorting (both local and remote modes)
    Default,     // Hostname then index (current behavior)
    Utilization, // GPU utilization
    GpuMemory,   // GPU memory usage
    #[allow(dead_code)]
    Power, // Power consumption
    #[allow(dead_code)]
    Temperature, // Temperature
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            gpu_info: Vec::new(),
            cpu_info: Vec::new(),
            memory_info: Vec::new(),
            process_info: Vec::new(),
            selected_process_index: 0,
            start_index: 0,
            sort_criteria: SortCriteria::Default,
            sort_direction: SortDirection::Descending,
            loading: true,
            tabs: vec![
                "All".to_string(),
                "GPU".to_string(),
                "Storage".to_string(),
                "Process".to_string(),
            ],
            current_tab: 0,
            gpu_scroll_offset: 0,
            storage_scroll_offset: 0,
            tab_scroll_offset: 0,
            process_horizontal_scroll_offset: 0,
            device_name_scroll_offsets: HashMap::new(),
            hostname_scroll_offsets: HashMap::new(),
            frame_counter: 0,
            storage_info: Vec::new(),
            show_help: false,
            utilization_history: VecDeque::new(),
            memory_history: VecDeque::new(),
            temperature_history: VecDeque::new(),
            notifications: NotificationManager::new(),
            nvml_notification_shown: false,
            // Connection status tracking for remote mode
            connection_status: HashMap::new(),
            known_hosts: Vec::new(),
        }
    }
}

impl SortCriteria {
    pub fn sort_gpus(&self, a: &GpuInfo, b: &GpuInfo) -> Ordering {
        match self {
            SortCriteria::Default => {
                // Sort by hostname first, then by index (original behavior)
                a.hostname.cmp(&b.hostname).then_with(|| {
                    let a_index = a
                        .detail
                        .get("index")
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(0);
                    let b_index = b
                        .detail
                        .get("index")
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(0);
                    a_index.cmp(&b_index)
                })
            }
            SortCriteria::Utilization => {
                // Sort by utilization (descending), then by hostname and index
                b.utilization
                    .partial_cmp(&a.utilization)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| a.hostname.cmp(&b.hostname))
                    .then_with(|| {
                        let a_index = a
                            .detail
                            .get("index")
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);
                        let b_index = b
                            .detail
                            .get("index")
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);
                        a_index.cmp(&b_index)
                    })
            }
            SortCriteria::GpuMemory => {
                // Sort by memory usage (descending), then by hostname and index
                b.used_memory
                    .cmp(&a.used_memory)
                    .then_with(|| a.hostname.cmp(&b.hostname))
                    .then_with(|| {
                        let a_index = a
                            .detail
                            .get("index")
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);
                        let b_index = b
                            .detail
                            .get("index")
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);
                        a_index.cmp(&b_index)
                    })
            }
            SortCriteria::Power => {
                // Sort by power consumption (descending), then by hostname and index
                b.power_consumption
                    .partial_cmp(&a.power_consumption)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| a.hostname.cmp(&b.hostname))
                    .then_with(|| {
                        let a_index = a
                            .detail
                            .get("index")
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);
                        let b_index = b
                            .detail
                            .get("index")
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);
                        a_index.cmp(&b_index)
                    })
            }
            SortCriteria::Temperature => {
                // Sort by temperature (descending), then by hostname and index
                b.temperature
                    .cmp(&a.temperature)
                    .then_with(|| a.hostname.cmp(&b.hostname))
                    .then_with(|| {
                        let a_index = a
                            .detail
                            .get("index")
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);
                        let b_index = b
                            .detail
                            .get("index")
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);
                        a_index.cmp(&b_index)
                    })
            }
            _ => {
                // For process sorting criteria, fall back to default GPU sorting
                a.hostname.cmp(&b.hostname).then_with(|| {
                    let a_index = a
                        .detail
                        .get("index")
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(0);
                    let b_index = b
                        .detail
                        .get("index")
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(0);
                    a_index.cmp(&b_index)
                })
            }
        }
    }

    pub fn sort_processes(
        &self,
        a: &ProcessInfo,
        b: &ProcessInfo,
        direction: SortDirection,
    ) -> Ordering {
        let base_ordering = match self {
            SortCriteria::Pid => a.pid.cmp(&b.pid),
            SortCriteria::User => a.user.cmp(&b.user),
            SortCriteria::Priority => a.priority.cmp(&b.priority),
            SortCriteria::Nice => a.nice_value.cmp(&b.nice_value),
            SortCriteria::VirtualMemory => a.memory_vms.cmp(&b.memory_vms),
            SortCriteria::ResidentMemory => a.memory_rss.cmp(&b.memory_rss),
            SortCriteria::State => a.state.cmp(&b.state),
            SortCriteria::CpuPercent => a
                .cpu_percent
                .partial_cmp(&b.cpu_percent)
                .unwrap_or(Ordering::Equal),
            SortCriteria::MemoryPercent => a
                .memory_percent
                .partial_cmp(&b.memory_percent)
                .unwrap_or(Ordering::Equal),
            SortCriteria::GpuPercent => a
                .gpu_utilization
                .partial_cmp(&b.gpu_utilization)
                .unwrap_or(Ordering::Equal),
            SortCriteria::GpuMemoryUsage => a.used_memory.cmp(&b.used_memory),
            SortCriteria::CpuTime => a.cpu_time.cmp(&b.cpu_time),
            SortCriteria::Command => a.command.cmp(&b.command),
            // For GPU-related sorting or default, sort by PID
            _ => a.pid.cmp(&b.pid),
        };

        match direction {
            SortDirection::Ascending => base_ordering,
            SortDirection::Descending => base_ordering.reverse(),
        }
    }
}
