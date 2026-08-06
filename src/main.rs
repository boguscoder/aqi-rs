#![no_std]
#![no_main]

mod sensor;
mod ui;

use crate::ui::{render_ui, ViewMode};
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{with_timeout, Duration, Timer};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use embedded_graphics_framebuf::FrameBuf;
use esp_backtrace as _;
use esp_hal::ledc::channel::ChannelIFace;
use esp_hal::ledc::{channel, LowSpeed};
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
    init_display, setup_backlight, DisplayConfig, LandscapeDisplay, DISPLAY_HEIGHT, DISPLAY_WIDTH,
};
esp_bootloader_esp_idf::esp_app_desc!();

// Duty cycle constants
const SLEEP_MINUTES: u64 = 10;
const ACTIVE_MINUTES: u64 = 60;
const BACKLIGHT_BRIGHTNESS: u8 = 10;
const DIMMED_BRIGHTNESS: u8 = 0;
const BACKLIGHT_TIMEOUT_SECS: u64 = 30;
// History constants
const HISTORY_HOURS: usize = 24;
const SAMPLE_INTERVAL_SECS: u64 = 5;
const MAX_HISTORY: usize = HISTORY_HOURS * 60 * (60 / SAMPLE_INTERVAL_SECS as usize);

static BUTTON_PRESSED: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static SENSOR_CHANNEL: Signal<CriticalSectionRawMutex, PmsReading> = Signal::new();
static BACKLIGHT_WAKE: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[embassy_executor::task]
async fn backlight_task(backlight: channel::Channel<'static, LowSpeed>) {
    loop {
        let result = with_timeout(
            Duration::from_secs(BACKLIGHT_TIMEOUT_SECS),
            BACKLIGHT_WAKE.wait(),
        )
        .await;

        if result.is_ok() {
            backlight.set_duty(BACKLIGHT_BRIGHTNESS).unwrap();
        } else {
            backlight.set_duty(DIMMED_BRIGHTNESS).unwrap();
            BACKLIGHT_WAKE.wait().await;
            backlight.set_duty(BACKLIGHT_BRIGHTNESS).unwrap();
        }
    }
}

#[embassy_executor::task]
async fn button_task(mut button: Input<'static>) {
    loop {
        button.wait_for_falling_edge().await;
        BUTTON_PRESSED.signal(());
        BACKLIGHT_WAKE.signal(());
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

        if sensor_active {
            if (elapsed_secs / 60) >= ACTIVE_MINUTES {
                pms.sleep().await.ok();
                sensor_active = false;
            } else if let Some(frame) = pms.read().await {
                SENSOR_CHANNEL.signal(frame);
            }
        } else {
            let sleep_duration = (ACTIVE_MINUTES * 60) + (SLEEP_MINUTES * 60);
            if elapsed_secs >= sleep_duration {
                pms.wake().await.ok();
                sensor_active = true;
                cycle_start = Instant::now();
            } else {
                Timer::after(Duration::from_secs(1)).await;
            }
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

    setup_backlight!(peripherals.LEDC, peripherals.GPIO22, backlight_channel);
    backlight_channel.set_duty(BACKLIGHT_BRIGHTNESS).unwrap();

    spawner.spawn(button_task(button).unwrap());
    spawner.spawn(sensor_task(pms).unwrap());
    spawner.spawn(backlight_task(backlight_channel).unwrap());

    let mut delay = Delay::new();
    let mut display = init_display(
        DisplayConfig {
            spi: peripherals.SPI2,
            mosi: peripherals.GPIO6.into(),
            sclk: peripherals.GPIO7.into(),
            cs: peripherals.GPIO14.into(),
            dc: peripherals.GPIO15.into(),
            rst: peripherals.GPIO21.into(),
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
    let mut last_frame = Some(PmsReading {
        pm1_0_atm: 0,
        pm2_5_atm: 0,
        pm10_0_atm: 0,
    });

    loop {
        let now = Instant::now();
        let mut sample_ready = false;

        if let Some(()) = BUTTON_PRESSED.try_take() {
            current_view = current_view.next();
            force_redraw = true;
        }

        if let Some(frame) = SENSOR_CHANNEL.try_take() {
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
