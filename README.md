# ESP32-C6 AQI Monitor

Simple `no_std` air quality monitor for the **Waveshare ESP32-C6-LCD-1.47** board and **Plantower PMS5003** sensor.

## Features
- **Multi-Mode UI**: Cycle between 3 distinct views using the onboard BOOT button (GPIO 9):
  - **Live**: Current PM1.0, PM2.5, and PM10 readings in µg/m³.
  - **Graph**: Historical PM2.5 trend plot (last 24 hours).
  - **Daily**: Hourly aggregated trend view.
- **Power Management**: Automatic duty cycle (2 hours active, 10 minutes sleep) to preserve sensor life.

## Build & Run
Ensure you have the RISC-V target installed:
```bash
rustup target add riscv32imac-unknown-none-elf
```

Flash the device:
```bash
cargo run --release
```