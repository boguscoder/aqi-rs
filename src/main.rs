#![no_std]
#![no_main]

mod sensor;
mod ui;

use crate::ui::{render_ui, ViewMode};
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Timer};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_graphics_framebuf::FrameBuf;
use esp_backtrace as _;
use esp_hal::{
    delay::Delay,
    gpio::{Input, InputConfig, Pull},
    time::Instant,
    uart::{Config, Uart},
};
use esp_hal::{interrupt::software::SoftwareInterruptControl, timer::timg::TimerGroup};
use heapless::Vec;
use sensor::{Pms5003, PmsReading};
use waveshare_display::{
    init_display, set_backlight, DisplayConfig, LandscapeDisplay, DISPLAY_HEIGHT, DISPLAY_WIDTH,
};

esp_bootloader_esp_idf::esp_app_desc!();

// Duty cycle constants
const SLEEP_MINUTES: u64 = 10;
const ACTIVE_MINUTES: u64 = 120;
// History constants
const HISTORY_HOURS: usize = 24;
const SAMPLE_INTERVAL_SECS: u64 = 5;
const MAX_HISTORY: usize = HISTORY_HOURS * 60 * (60 / SAMPLE_INTERVAL_SECS as usize);
const BACKLIGHT_BRIGHTNESS: u8 = 10;

static BUTTON_PRESSED: AtomicBool = AtomicBool::new(false);
static SENSOR_CHANNEL: Channel<CriticalSectionRawMutex, PmsReading, 2> = Channel::new();

#[embassy_executor::task]
async fn button_task(mut button: Input<'static>) {
    loop {
        button.wait_for_falling_edge().await;
        BUTTON_PRESSED.store(true, Ordering::Relaxed);
        // Debounce the press
        Timer::after(Duration::from_millis(50)).await;
        while button.is_low() {
            Timer::after(Duration::from_millis(10)).await;
        }
        // Debounce the release
        Timer::after(Duration::from_millis(20)).await;
    }
}

#[embassy_executor::task]
async fn sensor_task(mut pms: Pms5003) {
    let mut cycle_start = Instant::now();
    let mut sensor_active = true;

    loop {
        let elapsed_secs = (Instant::now() - cycle_start).as_secs();

        if (elapsed_secs / 60) >= ACTIVE_MINUTES {
            if sensor_active {
                pms.sleep().await.ok();
                sensor_active = false;
                set_backlight(1);
            }
            if elapsed_secs >= (ACTIVE_MINUTES * 60 + SLEEP_MINUTES * 60) {
                pms.wake().await.ok();
                sensor_active = true;
                set_backlight(BACKLIGHT_BRIGHTNESS);
                cycle_start = Instant::now();
            }
            Timer::after(Duration::from_secs(1)).await;
        } else if let Some(frame) = pms.read().await {
            SENSOR_CHANNEL.send(frame).await;
        }
    }
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let config = esp_hal::Config::default();
    let peripherals = esp_hal::init(config);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_intr = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_intr.software_interrupt0);

    let button = Input::new(
        peripherals.GPIO9,
        InputConfig::default().with_pull(Pull::Up),
    );

    let pms = Pms5003::new(
        Uart::new(peripherals.UART1, Config::default().with_baudrate(9600))
            .unwrap()
            .with_tx(peripherals.GPIO16)
            .with_rx(peripherals.GPIO17)
            .into_async(),
    );

    spawner.spawn(button_task(button).unwrap());
    spawner.spawn(sensor_task(pms).unwrap());

    let mut delay = Delay::new();
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

    let mut history: Vec<u16, MAX_HISTORY> = Vec::new();
    let framebuffer: [Rgb565; (DISPLAY_WIDTH * DISPLAY_HEIGHT) as usize] =
        [Rgb565::BLACK; (DISPLAY_WIDTH * DISPLAY_HEIGHT) as usize];
    let mut hardware_display = LandscapeDisplay { base: &mut display };
    let mut fbuf = FrameBuf::new(framebuffer, DISPLAY_WIDTH as usize, DISPLAY_HEIGHT as usize);

    let mut current_view = ViewMode::Live;

    let mut force_redraw = true;

    let mut last_sample_time = Instant::now();
    let mut last_frame = None;

    loop {
        let now = Instant::now();
        let mut sample_ready = false;

        if BUTTON_PRESSED.swap(false, Ordering::Relaxed) {
            current_view = current_view.next();
            force_redraw = true;
        }

        if let Ok(frame) = SENSOR_CHANNEL.try_receive() {
            if (now - last_sample_time).as_secs() >= SAMPLE_INTERVAL_SECS {
                if history.is_full() {
                    history.remove(0);
                }
                history.push(frame.pm2_5_atm).ok();
                last_sample_time = now;
                sample_ready = true;
            }
            last_frame = Some(frame);
        }

        if force_redraw || sample_ready {
            if let Some(ref frame) = last_frame {
                render_ui(&mut fbuf, frame, &history, current_view, force_redraw);
            }
            hardware_display.draw_iter(&fbuf).unwrap();
            force_redraw = false;
        }

        Timer::after(Duration::from_millis(10)).await;
    }
}
