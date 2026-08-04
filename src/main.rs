#![no_std]
#![no_main]

mod sensor;
mod ui;

use crate::sensor::init_sensor;
use crate::ui::{render_ui, ViewMode};
use embedded_hal::delay::DelayNs;
use esp_backtrace as _;
use esp_hal::{
    delay::Delay,
    gpio::{Input, InputConfig, Pull},
    time::Instant,
};
use heapless::Vec;
use static_cell::StaticCell;
use waveshare_display::{init_display, set_backlight, DisplayConfig};
esp_bootloader_esp_idf::esp_app_desc!();

// Duty cycle constants
const SLEEP_MINUTES: u64 = 10;
const ACTIVE_MINUTES: u64 = 120;
// History constants
pub(crate) const HISTORY_HOURS: usize = 24;
pub(crate) const SAMPLE_INTERVAL_SECS: u64 = 5;
pub(crate) const MAX_HISTORY: usize = HISTORY_HOURS * 60 * (60 / SAMPLE_INTERVAL_SECS as usize);
pub(crate) const BACKLIGHT_BRIGHTNESS: u8 = 10;

static HISTORY_CELL: StaticCell<Vec<u16, MAX_HISTORY>> = StaticCell::new();

#[esp_hal::main]
fn main() -> ! {
    let history: &'static mut Vec<u16, MAX_HISTORY> = HISTORY_CELL.init(Vec::new());

    let config = esp_hal::Config::default();
    let peripherals = esp_hal::init(config);
    let mut delay = Delay::new();

    let button = Input::new(
        peripherals.GPIO9,
        InputConfig::default().with_pull(Pull::Up),
    );

    let mut display = init_display(
        DisplayConfig {
            spi: peripherals.SPI2,
            mosi: peripherals.GPIO6.into(),
            sclk: peripherals.GPIO7.into(),
            cs: peripherals.GPIO14.into(),
            dc: peripherals.GPIO15.into(),
            rst: peripherals.GPIO21.into(),
            bl: peripherals.GPIO22.into(),
            ledc: peripherals.LEDC,
            backlight_duty: BACKLIGHT_BRIGHTNESS,
        },
        &mut delay,
    );

    let mut pms = init_sensor(
        peripherals.UART1,
        peripherals.GPIO16.into(),
        peripherals.GPIO17.into(),
    );

    let mut current_view = ViewMode::Live;
    let mut last_button_state = button.is_high();

    let mut force_redraw = true;

    let mut cycle_start = Instant::now();
    let mut last_sample_time = cycle_start;
    let mut sensor_active = true;

    loop {
        let now = Instant::now();
        let elapsed_secs = (now - cycle_start).as_secs();

        // Button polling
        let current_button_state = button.is_high();
        if last_button_state && !current_button_state {
            current_view = current_view.next();
            force_redraw = true;
        }
        last_button_state = current_button_state;

        // Duty cycle
        if (elapsed_secs / 60) >= ACTIVE_MINUTES {
            if sensor_active {
                let _ = pms.sleep();
                sensor_active = false;
                set_backlight(1);
            }
            if elapsed_secs >= (ACTIVE_MINUTES * 60 + SLEEP_MINUTES * 60) {
                let _ = pms.wake();
                sensor_active = true;
                set_backlight(BACKLIGHT_BRIGHTNESS);
                cycle_start = now;
            }
        } else {
            if let Ok(frame) = pms.read() {
                let mut sample_ready = false;
                if (now - last_sample_time).as_secs() >= SAMPLE_INTERVAL_SECS {
                    if history.is_full() {
                        history.remove(0);
                    }
                    let _ = history.push(frame.pm2_5_atm);
                    last_sample_time = now;
                    sample_ready = true;
                }

                if force_redraw || sample_ready {
                    render_ui(&mut display, &frame, history, current_view, force_redraw);
                    force_redraw = false;
                }
            }
        }

        delay.delay_ms(100u32);
    }
}
