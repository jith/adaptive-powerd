use std::fs;
use std::time::{Duration, Instant};

// -----------------------------
struct State {
    last_mode: String,
    last_switch: Instant,
    cpu_prev: Vec<(u64, u64)>,

    last_freq: f32,
    last_usage: f32,
}

// -----------------------------
// MAX CORE FREQ
// -----------------------------
fn effective_core_freq_ratio() -> f32 {
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
                        let ratio = cn / mn;
                        if ratio > max_ratio {
                            max_ratio = ratio;
                        }
                    }
                }
            }
        }
    }

    max_ratio
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
// GPU BUSY
// -----------------------------
fn gpu_busy() -> bool {
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for e in entries.flatten() {
            let p = e.path().join("device/gpu_busy_percent");
            if let Ok(v) = fs::read_to_string(p) {
                if let Ok(n) = v.trim().parse::<u64>() {
                    return n > 10;
                }
            }
        }
    }
    false
}

// -----------------------------
// IDLE DETECTION (FIXED)
// -----------------------------
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
fn write_epp(value: &str) {
    if let Ok(entries) = fs::read_dir("/sys/devices/system/cpu") {
        for e in entries.flatten() {
            let path = e.path().join("cpufreq/energy_performance_preference");
            let _ = fs::write(path, value);
        }
    }
}

// -----------------------------
// GPU MODE CONTROL
// -----------------------------
fn write_gpu_mode(mode: &str) {
    let value = match mode {
        "performance" => "high",
        "power" => "low",
        _ => "auto",
    };

    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for e in entries.flatten() {
            let path =
                e.path().join("device/power_dpm_force_performance_level");

            if path.exists() {
                let _ = fs::write(path, value);
            }
        }
    }
}

// -----------------------------
// DYNAMIC SCORING ENGINE
// -----------------------------
fn compute_score(
    usage: f32,
    freq: f32,
    gpu: bool,
    state: &mut State,
) -> f32 {
    let freq_delta = (freq - state.last_freq).max(0.0);
    let usage_delta = (usage - state.last_usage).max(0.0);

    let delta = (freq_delta + usage_delta) / 2.0;

    let gpu_boost = if gpu { 0.1 } else { 0.0 };

    let score = usage * 0.5 + freq * 0.3 + delta * 0.2 + gpu_boost;

    state.last_freq = freq;
    state.last_usage = usage;

    score.min(1.0)
}

// -----------------------------
fn decide_mode(state: &mut State) -> String {
    let freq = effective_core_freq_ratio();
    let usage = max_core_usage(&mut state.cpu_prev);
    let gpu = gpu_busy();
    let idle = all_users_idle();
    let temp = read_temp_c();

    let score = compute_score(usage, freq, gpu, state);

    // thermal override
    if temp > 92 {
        return "power".into();
    } else if temp > 85 {
        return "balance_power".into();
    } else if temp > 75 {
        return "balance_performance".into();
    }

    if idle {
        return "power".into();
    }

    // dynamic mapping
    if score > 0.80 {
        "performance".into()
    } else if score > 0.60 {
        "balance_performance".into()
    } else if score > 0.30 {
        "balance_power".into()
    } else {
        "power".into()
    }
}

// -----------------------------
fn rebalance(state: &mut State) {
    let now = Instant::now();
    let mode = decide_mode(state);

    // stickiness
    if now.duration_since(state.last_switch)
        < Duration::from_millis(300)
    {
        return;
    }

    if mode != state.last_mode {
        write_epp(&mode);
        write_gpu_mode(&mode);
        state.last_mode = mode;
        state.last_switch = now;
    }
}

// -----------------------------
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut state = State {
        last_mode: String::new(),
        last_switch: Instant::now(),
        cpu_prev: Vec::new(),
        last_freq: 0.0,
        last_usage: 0.0,
    };

    loop {
        rebalance(&mut state);
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
