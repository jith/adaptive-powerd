use std::fs;
use std::time::{Duration, Instant};

// -----------------------------
struct State {
    last_mode: String,
    last_active: Instant,
    cpu_prev: Vec<(u64, u64)>,

    // burst detection
    last_freq: f32,
    last_usage: f32,
    stable_count: u8,
}

// -----------------------------
// TOP-CORE AVERAGE FREQ (FIXED)
// -----------------------------
fn effective_core_freq_ratio() -> f32 {
    let mut ratios: Vec<f32> = Vec::new();

    if let Ok(entries) = fs::read_dir("/sys/devices/system/cpu") {
        for e in entries.flatten() {
            let cur = e.path().join("cpufreq/scaling_cur_freq");
            let max = e.path().join("cpufreq/cpuinfo_max_freq");

            if let (Ok(c), Ok(m)) = (
                fs::read_to_string(cur),
                fs::read_to_string(max),
            ) {
                if let (Ok(cn), Ok(mn)) = (
                    c.trim().parse::<f32>(),
                    m.trim().parse::<f32>(),
                ) {
                    if mn > 0.0 {
                        ratios.push(cn / mn);
                    }
                }
            }
        }
    }

    if ratios.is_empty() {
        return 0.0;
    }

    // sort descending
    ratios.sort_by(|a, b| b.partial_cmp(a).unwrap());

    // take top 2 cores (or 1 if single-core read)
    let count = ratios.len().min(2);
    let sum: f32 = ratios.iter().take(count).sum();

    sum / count as f32
}

// -----------------------------
// PER-CORE USAGE
// -----------------------------
fn max_core_usage(prev: &mut Vec<(u64, u64)>) -> f32 {
    let mut max_usage = 0.0;

    if let Ok(stat) = fs::read_to_string("/proc/stat") {
        let mut i = 0;

        for line in stat.lines() {
            if !line.starts_with("cpu") || line.starts_with("cpu ") {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();

            let user: u64 = parts[1].parse().unwrap_or(0);
            let nice: u64 = parts[2].parse().unwrap_or(0);
            let system: u64 = parts[3].parse().unwrap_or(0);
            let idle: u64 = parts[4].parse().unwrap_or(0);

            let total = user + nice + system + idle;

            if i >= prev.len() {
                prev.push((total, idle));
                i += 1;
                continue;
            }

            let (prev_total, prev_idle) = prev[i];

            let dt = total.saturating_sub(prev_total);
            let di = idle.saturating_sub(prev_idle);

            if dt > 0 {
                let usage = (dt - di) as f32 / dt as f32;
                if usage > max_usage {
                    max_usage = usage;
                }
            }

            prev[i] = (total, idle);
            i += 1;
        }
    }

    max_usage
}

// -----------------------------
fn gpu_busy() -> bool {
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for e in entries.flatten() {
            let p = e.path().join("device/gpu_busy_percent");
            if let Ok(v) = fs::read_to_string(p) {
                if let Ok(n) = v.trim().parse::<u64>() {
                    return n > 25;
                }
            }
        }
    }
    false
}

// -----------------------------
fn any_user_idle() -> bool {
    if let Ok(entries) = fs::read_dir("/run/user") {
        for e in entries.flatten() {
            let path = e.path().join("adaptive-powerd.state");
            if let Ok(content) = fs::read_to_string(path) {
                if content.contains("idle=true") {
                    return true;
                }
            }
        }
    }
    false
}

// -----------------------------
fn read_temp_c() -> u64 {
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

// -----------------------------
fn on_ac() -> bool {
    if let Ok(entries) = fs::read_dir("/sys/class/power_supply") {
        for e in entries.flatten() {
            let p = e.path().join("online");
            if let Ok(v) = fs::read_to_string(p) {
                if v.trim() == "1" {
                    return true;
                }
            }
        }
    }
    false
}

// -----------------------------
fn write_epp(value: &str) {
    if let Ok(entries) = fs::read_dir("/sys/devices/system/cpu") {
        for e in entries.flatten() {
            let path = e.path().join("cpufreq/energy_performance_preference");
            let _ = fs::write(path, value);
        }
    }
}

// -----------------------------
// BURST DETECTION (FIXED)
// -----------------------------
fn detect_burst(
    usage: f32,
    freq: f32,
    gpu: bool,
    state: &mut State,
) -> bool {
    let freq_delta = freq - state.last_freq;
    let usage_delta = usage - state.last_usage;

    let strong_signal =
        usage > 0.65 &&
        freq > 0.75 &&
        freq_delta > 0.05;

    let medium_signal =
        usage > 0.50 &&
        freq > 0.65 &&
        usage_delta > 0.05;

    let gpu_signal =
        gpu && usage > 0.40;

    let signal = strong_signal || medium_signal || gpu_signal;

    if signal {
        state.stable_count += 1;
    } else {
        state.stable_count = 0;
    }

    state.last_freq = freq;
    state.last_usage = usage;

    state.stable_count >= 2
}

// -----------------------------
fn decide_mode(state: &mut State) -> String {
    let freq = effective_core_freq_ratio(); // FIXED
    let usage = max_core_usage(&mut state.cpu_prev);
    let gpu = gpu_busy();
    let idle = any_user_idle();
    let temp = read_temp_c();
    let ac = on_ac();

    let now = Instant::now();

    let active = detect_burst(usage, freq, gpu, state);

    if active {
        state.last_active = now;
    }

    let recently_active =
        now.duration_since(state.last_active) < Duration::from_millis(400);

    if temp > 85 {
        return "power".into();
    } else if temp > 75 {
        return "balance_performance".into();
    }

    if ac {
        if recently_active {
            return "performance".into();
        }
        if idle {
            return "power".into();
        }
        return "balance_power".into();
    }

    if recently_active {
        return "balance_performance".into();
    }

    if idle {
        return "power".into();
    }

    "balance_power".into()
}

// -----------------------------
fn rebalance(state: &mut State) {
    let mode = decide_mode(state);
    if mode != state.last_mode {
        write_epp(&mode);
        state.last_mode = mode;
    }
}

// -----------------------------
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut state = State {
        last_mode: String::new(),
        last_active: Instant::now(),
        cpu_prev: Vec::new(),

        last_freq: 0.0,
        last_usage: 0.0,
        stable_count: 0,
    };

    loop {
        rebalance(&mut state);
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
