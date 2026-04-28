use std::fs;
use std::process::Command;
use std::time::{Duration, Instant};

use tokio::time::sleep;

// =============================
// CONFIG
// =============================

const LOOP_INTERVAL: Duration = Duration::from_millis(400);
const MIN_MODE_HOLD: Duration = Duration::from_secs(2);

// =============================
// CPU MODES
// =============================

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CpuMode {
    Performance,
    BalancePerformance,
    BalancePower,
    Power,
}

impl CpuMode {
    fn as_epp(self) -> &'static str {
        match self {
            CpuMode::Performance => "performance",
            CpuMode::BalancePerformance => "balance_performance",
            CpuMode::BalancePower => "balance_power",
            CpuMode::Power => "power",
        }
    }
}

// =============================
// GPU MODES
// =============================

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GpuMode {
    Auto,
    Low,
}

// =============================
// GPU STRUCT
// =============================

#[derive(Debug)]
struct GpuActivity {
    gfx_load: u32,
    vcn_load: u32,
    vcn_active: bool,
}

// =============================
// TELEMETRY
// =============================

struct Telemetry {
    cpu_usage: f32,
    cpu_freq: f32,
    temp: u64,
    ac: bool,
    idle: bool,
    ppd: String,
    gpu: Option<GpuActivity>,
}

// =============================
// STATE
// =============================

struct State {
    cpu_mode: CpuMode,
    gpu_mode: GpuMode,
    last_switch: Instant,

    cpu_prev: Vec<(u64, u64)>,

    usage_smooth: f32,
    freq_smooth: f32,
}

// =============================
// SYSTEM READERS
// =============================

fn read_ppd_profile() -> String {
    Command::new("powerprofilesctl")
        .arg("get")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "balanced".into())
}

fn read_ac_online() -> bool {
    fs::read_to_string("/sys/class/power_supply/AC/online")
        .map(|v| v.trim() == "1")
        .unwrap_or(true)
}

fn read_temp() -> u64 {
    let paths = [
        "/sys/class/thermal/thermal_zone0/temp",
        "/sys/class/hwmon/hwmon0/temp1_input",
    ];

    for p in paths {
        if let Ok(v) = fs::read_to_string(p) {
            if let Ok(n) = v.trim().parse::<u64>() {
                return n / 1000;
            }
        }
    }
    50
}

fn all_users_idle() -> bool {
    let mut found = false;

    if let Ok(entries) = fs::read_dir("/run/user") {
        for e in entries.flatten() {
            let path = e.path().join("adaptive-powerd.state");

            if let Ok(content) = fs::read_to_string(path) {
                found = true;
                if content.contains("idle=false") {
                    return false;
                }
            }
        }
    }

    found
}

// =============================
// CPU METRICS
// =============================

fn cpu_usage(prev: &mut Vec<(u64, u64)>) -> f32 {
    let mut max = 0.0;

    if let Ok(stat) = fs::read_to_string("/proc/stat") {
        let mut i = 0;

        for line in stat.lines() {
            if !line.starts_with("cpu") || line.starts_with("cpu ") {
                continue;
            }

            let p: Vec<&str> = line.split_whitespace().collect();

            let user: u64 = p[1].parse().unwrap_or(0);
            let nice: u64 = p[2].parse().unwrap_or(0);
            let system: u64 = p[3].parse().unwrap_or(0);
            let idle: u64 = p[4].parse().unwrap_or(0);

            let total = user + nice + system + idle;

            if i >= prev.len() {
                prev.push((total, idle));
                i += 1;
                continue;
            }

            let (pt, pi) = prev[i];

            let dt = total.saturating_sub(pt);
            let di = idle.saturating_sub(pi);

            if dt > 0 {
                let usage = (dt - di) as f32 / dt as f32;
                if usage > max {
                    max = usage;
                }
            }

            prev[i] = (total, idle);
            i += 1;
        }
    }

    max
}

fn cpu_freq_ratio() -> f32 {
    let mut max_ratio = 0.0;

    if let Ok(entries) = fs::read_dir("/sys/devices/system/cpu") {
        for e in entries.flatten() {
            let cur = e.path().join("cpufreq/scaling_cur_freq");
            let max = e.path().join("cpufreq/cpuinfo_max_freq");

            if let (Ok(c), Ok(m)) =
                (fs::read_to_string(cur), fs::read_to_string(max))
            {
                if let (Ok(cn), Ok(mn)) =
                    (c.trim().parse::<f32>(), m.trim().parse::<f32>())
                {
                    if mn > 0.0 {
                        let r = cn / mn;
                        if r > max_ratio {
                            max_ratio = r;
                        }
                    }
                }
            }
        }
    }

    max_ratio
}

// =============================
// GPU DETECTION
// =============================

fn find_amdgpu_cards() -> Vec<std::path::PathBuf> {
    let mut cards = Vec::new();

    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("card") {
                continue;
            }

            let path = e.path();
            let driver = path.join("device/driver");

            if let Ok(target) = fs::read_link(driver) {
                if target.to_string_lossy().contains("amdgpu") {
                    cards.push(path);
                }
            }
        }
    }

    cards
}

fn read_gpu_activity() -> Option<GpuActivity> {
    if let Ok(entries) = fs::read_dir("/sys/kernel/debug/dri") {
        for e in entries.flatten() {
            let path = e.path().join("amdgpu_pm_info");

            if !path.exists() {
                continue;
            }

            if let Ok(content) = fs::read_to_string(path) {
                let mut gfx = 0;
                let mut vcn = 0;
                let mut active = false;

                for line in content.lines() {
                    if line.contains("GPU Load") {
                        if let Some(v) = line.split(':').nth(1) {
                            gfx = v.trim().trim_end_matches('%').parse().unwrap_or(0);
                        }
                    }

                    if line.contains("VCN Load") {
                        if let Some(v) = line.split(':').nth(1) {
                            vcn = v.trim().trim_end_matches('%').parse().unwrap_or(0);
                        }
                    }

                    if line.contains("VCN:") {
                        active = !line.contains("Powered down");
                    }
                }

                return Some(GpuActivity {
                    gfx_load: gfx,
                    vcn_load: vcn,
                    vcn_active: active,
                });
            }
        }
    }

    None
}

// =============================
// WRITE FUNCTIONS
// =============================

fn write_epp(value: &str) {
    if let Ok(entries) = fs::read_dir("/sys/devices/system/cpu") {
        for e in entries.flatten() {
            let p = e.path().join("cpufreq/energy_performance_preference");
            let _ = fs::write(p, value);
        }
    }
}

fn write_gpu_mode(mode: &str) {
    for card in find_amdgpu_cards() {
        let p = card.join("device/power_dpm_force_performance_level");
        if p.exists() {
            let _ = fs::write(p, mode);
        }
    }
}

// =============================
// CPU DECISION
// =============================

fn decide_cpu_mode(state: &mut State, t: &Telemetry) -> CpuMode {
    state.usage_smooth = state.usage_smooth * 0.7 + t.cpu_usage * 0.3;
    state.freq_smooth = state.freq_smooth * 0.7 + t.cpu_freq * 0.3;

    let mut score =
        state.usage_smooth * 0.6 + state.freq_smooth * 0.3;

    if let Some(g) = &t.gpu {
        if g.gfx_load > 30 {
            score += 0.1;
        }
        if g.vcn_active && g.vcn_load > 20 {
            score *= 0.6;
        }
    }

    if !t.ac {
        score *= 0.75;
    }

    if t.idle {
        return CpuMode::Power;
    }

    if t.temp > 92 {
        return CpuMode::Power;
    }

    match t.ppd.as_str() {
        "performance" => score *= 1.1,
        "power-saver" => score *= 0.7,
        _ => {}
    }

    match score {
        s if s > 0.80 => CpuMode::Performance,
        s if s > 0.60 => CpuMode::BalancePerformance,
        s if s > 0.30 => CpuMode::BalancePower,
        _ => CpuMode::Power,
    }
}

// =============================
// GPU DECISION (FIXED)
// =============================

fn decide_gpu_mode(t: &Telemetry) -> GpuMode {
    // Strong idle
    if t.idle {
        return GpuMode::Low;
    }

    if let Some(g) = &t.gpu {
        // Video playback
        if g.vcn_active && g.vcn_load > 15 {
            return GpuMode::Auto;
        }

        // GPU load
        if g.gfx_load > 20 {
            return GpuMode::Auto;
        }

        // 🔥 FIX: low activity detection
        if g.gfx_load < 5 && t.cpu_usage < 0.25 {
            return GpuMode::Low;
        }
    }

    // Battery bias
    if !t.ac {
        return GpuMode::Low;
    }

    GpuMode::Auto
}

// =============================
// MAIN
// =============================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut state = State {
        cpu_mode: CpuMode::BalancePower,
        gpu_mode: GpuMode::Auto,
        last_switch: Instant::now(),
        cpu_prev: vec![],
        usage_smooth: 0.0,
        freq_smooth: 0.0,
    };

    loop {
        let telemetry = Telemetry {
            cpu_usage: cpu_usage(&mut state.cpu_prev),
            cpu_freq: cpu_freq_ratio(),
            temp: read_temp(),
            ac: read_ac_online(),
            idle: all_users_idle(),
            ppd: read_ppd_profile(),
            gpu: read_gpu_activity(),
        };

        let cpu_mode = decide_cpu_mode(&mut state, &telemetry);
        let gpu_mode = decide_gpu_mode(&telemetry);

        let now = Instant::now();

        if now.duration_since(state.last_switch) >= MIN_MODE_HOLD {
            if cpu_mode != state.cpu_mode {
                write_epp(cpu_mode.as_epp());
                state.cpu_mode = cpu_mode;
            }

            if gpu_mode != state.gpu_mode {
                match gpu_mode {
                    GpuMode::Low => write_gpu_mode("low"),
                    GpuMode::Auto => write_gpu_mode("auto"),
                }
                state.gpu_mode = gpu_mode;
            }

            println!(
                "CPU → {:?} | GPU → {:?} | idle {} | cpu {:.2}",
                cpu_mode, gpu_mode, telemetry.idle, telemetry.cpu_usage
            );

            state.last_switch = now;
        }

        sleep(LOOP_INTERVAL).await;
    }
}
