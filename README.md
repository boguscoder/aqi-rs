# ESP32-C6 AQI Monitor

Simple `no_std` air quality monitor for the **Waveshare ESP32-C6-LCD-1.47** board and **Plantower PMS5003** sensor.

## Features
- **Multi-Mode UI**: Cycle between 4 distinct views using the onboard BOOT button (GPIO 9):
  - **Live**: Real-time PM1.0, PM2.5, and PM10 readings with health-based color alerts.
  - **Last Hour**: High-resolution 60-minute trend graph (12s per pixel).
  - **Last 24h**: Bird's-eye view of the full day's trend (5m per pixel).
  - **24h Hourly**: Aggregated hourly summary bars for the past day.
- **AQI Color Coding**: Integrated health zones (Green, Yellow, Orange, Red) across all views for immediate visual feedback.
- **Power Management**: Automatic duty cycle (2 hours active, 10 minutes sleep) for the PMS5003 sensor.

## Build & Run
Ensure you have the RISC-V target installed:
```bash
rustup target add riscv32imac-unknown-none-elf
```

Flash the device:
```bash
cargo run --release
```