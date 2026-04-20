# 🚀 adaptive-powerd  
**Mac-like intelligent power management for Linux (Ryzen / AMD optimized)**

---

![Linux](https://img.shields.io/badge/Linux-Ubuntu-orange?logo=ubuntu)
![Rust](https://img.shields.io/badge/Built%20with-Rust-red?logo=rust)
![Status](https://img.shields.io/badge/status-active-success)
![License](https://img.shields.io/badge/license-MIT-blue)

---

## 🧠 Overview

`adaptive-powerd` is a lightweight, system-level power manager that replaces traditional Linux CPU scaling behavior with **workload-aware, per-core intelligent tuning**.

### ✨ What it does

- ✔ Per-core CPU awareness  
- ✔ Frequency-based intent detection  
- ✔ GPU activity awareness  
- ✔ GNOME idle integration  
- ✔ Instant boost + instant drop  
- ✔ Zero logging overhead (production safe)  

👉 Goal:  
Bring Linux behavior closer to **MacBook-style responsiveness + efficiency**

---

## ⚙️ Architecture

```text
GNOME (Idle state)
        ↓
User daemon (per user)
        ↓
/run/user/<uid>/adaptive-powerd.state
        ↓
Root daemon (system-wide)
        ↓
EPP (energy_performance_preference)
        ↓
CPU hardware scaling
```

---

## 🔥 Features

### ⚡ Performance
- Instant boost on real workload (no lag)
- Per-core detection (fixes Linux averaging issue)
- Excellent for:
  - CLI tools
  - builds
  - AI workloads (Ollama, Python)

---

### 🔋 Efficiency
- No unnecessary boosts  
- Instant drop after workload (~400ms)  
- Lower idle power than stock Linux  

---

### 🧊 Thermal Safety
- Automatic thermal throttling protection  
- Prevents overheating and battery stress  

---

## 📦 Requirements

- Ubuntu **24.04 / 26.04+**
- Kernel **6.8+ (7.x recommended)**
- GNOME **45+ (tested on GNOME 50)**
- AMD Ryzen CPU (Zen4 / Zen5 / Ryzen AI)

---

## ✅ Pre-checks

### 1️⃣ CPU driver

```bash
cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_driver
```

Expected:

```text
amd-pstate-epp
```

---

### 2️⃣ EPP support

```bash
cat /sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference
```

Expected values:

```text
performance balance_performance balance_power power
```

---

### 3️⃣ GPU metrics (optional)

```bash
ls /sys/class/drm/*/device/gpu_busy_percent
```

---

## 🛠️ Installation

### 1️⃣ Setup

```bash
mkdir -p ~/adaptive-powerd
cd ~/adaptive-powerd
```

---

### 2️⃣ Build root daemon

```bash
cd root-daemon
cargo build --release
```

---

### 3️⃣ Install root binary

```bash
sudo install -m 755 target/release/adaptive-powerd-root /usr/local/bin/
```

---

### 4️⃣ Install systemd service

```bash
sudo cp ../systemd/adaptive-powerd-root.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable adaptive-powerd-root
sudo systemctl start adaptive-powerd-root
```

---

### 5️⃣ Build user daemon

```bash
cd ../user-daemon
cargo build --release
```

---

### 6️⃣ Install user binary

```bash
sudo install -m 755 target/release/adaptive-powerd-user /usr/local/bin/
```

---

### 7️⃣ Enable GNOME autostart

```bash
sudo cp ../systemd/adaptive-powerd-user.desktop /etc/xdg/autostart/
```

---

## 🧪 Verification

### Check running processes

```bash
pgrep -fa adaptive-powerd
```

---

### Monitor power mode (EPP)

```bash
watch -n0.5 cat /sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference
```

---

### Expected behavior

| Scenario        | Mode               |
|----------------|--------------------|
| Idle           | power              |
| Browsing       | balance_power      |
| App launch     | performance        |
| CLI / AI       | performance        |
| Work finished  | instant drop       |

---

## 🔋 Memory Optimization (zswap + zram)

Inspired by: https://github.com/jith/ubuntu-tuned

### Enable zswap

```bash
sudo nano /etc/default/grub
```

Update:

```bash
GRUB_CMDLINE_LINUX_DEFAULT="quiet splash zswap.enabled=1 zswap.compressor=zstd zswap.max_pool_percent=20"
```

Apply:

```bash
sudo update-grub
sudo reboot
```

---

### Enable zram

```bash
sudo apt install zram-tools
```

```bash
sudo nano /etc/default/zramswap
```

```
ALGO=zstd
PERCENT=50
PRIORITY=100
```

```bash
sudo systemctl restart zramswap
```

---

## ⚡ Boot Optimization (BIG WIN ⚡)

Inspired by: https://github.com/jith/ubuntu-tuned

### Analyze boot time

```bash
systemd-analyze blame
```

---

### Disable unused services

```bash
sudo systemctl disable bluetooth
sudo systemctl disable cups
sudo systemctl disable ModemManager
```

---

### Reduce shutdown delay

```bash
sudo nano /etc/systemd/system.conf
```

```
DefaultTimeoutStopSec=5s
```

---

### Optional: disable snapd

```bash
sudo systemctl disable snapd
```

---

## ⚡ Performance Comparison

| System           | Performance | Efficiency |
|-----------------|------------|-----------|
| Stock Linux     | medium     | medium    |
| adaptive-powerd | 🔥 high     | 🔥 high    |
| MacBook Air     | highest    | highest   |

---

## 📊 Expected Gains

- ✔ Faster app launch  
- ✔ Better CLI responsiveness  
- ✔ ~5–12% battery improvement  
- ✔ Lower idle watt usage  

---

## ⚠️ Safety

- ✔ No kernel modification  
- ✔ No scheduler override  
- ✔ Uses standard Linux interfaces  
- ✔ Thermal safe  

---

## 🚀 Future Improvements

- Per-core type awareness (Zen5 vs Zen5c)  
- AI workload detection (ollama-specific)  
- Adaptive threshold learning  

---

## 🏁 Final

```
✔ Custom Linux power scheduler
✔ Mac-like responsiveness
✔ Better efficiency than stock Linux
✔ Full control over tuning
```

---

## ⭐ Contributing

Pull requests are welcome. Ideas and tuning improvements are encouraged.

---

## 📜 License

MIT
