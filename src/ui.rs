use crate::sensor::PmsReading;
use crate::{HISTORY_HOURS, MAX_HISTORY, SAMPLE_INTERVAL_SECS};
use core::fmt::Write;
use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::{iso_8859_1::FONT_10X20, MonoTextStyle, MonoTextStyleBuilder},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Line, PrimitiveStyle},
    text::Text,
    Drawable,
};
use heapless::String;

const GRAPH_TOP: i32 = 50;
const GRAPH_BOTTOM: i32 = 160;
const GRAPH_LEFT: i32 = 10;
const GRAPH_WIDTH: i32 = 300;
const GRAPH_HEIGHT: i32 = GRAPH_BOTTOM - GRAPH_TOP;

const COLOR_GOOD: Rgb565 = Rgb565::GREEN;
const COLOR_MODERATE: Rgb565 = Rgb565::YELLOW;
const COLOR_SENSITIVE: Rgb565 = Rgb565::new(31, 38, 0); // Orange-ish in RGB565
const COLOR_UNHEALTHY: Rgb565 = Rgb565::RED;

fn get_aqi_color(pm2_5: i32) -> Rgb565 {
    match pm2_5 {
        0..=12 => COLOR_GOOD,
        13..=35 => COLOR_MODERATE,
        36..=55 => COLOR_SENSITIVE,
        _ => COLOR_UNHEALTHY,
    }
}

#[derive(Copy, Clone, PartialEq)]
pub(crate) enum ViewMode {
    Live,
    LastHour,
    LastDay,
    Hourly,
}

impl ViewMode {
    pub fn next(self) -> Self {
        match self {
            Self::Live => Self::LastHour,
            Self::LastHour => Self::LastDay,
            Self::LastDay => Self::Hourly,
            Self::Hourly => Self::Live,
        }
    }
}

pub fn render_ui<T>(
    display: &mut T,
    frame: &PmsReading,
    history: &[u16],
    mode: ViewMode,
    force_redraw: bool,
) where
    T: DrawTarget<Color = Rgb565>,
{
    let title_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(Rgb565::CYAN)
        .background_color(Rgb565::BLACK)
        .build();

    if force_redraw {
        let _ = display.clear(Rgb565::BLACK);
    }

    match mode {
        ViewMode::Live => {
            let _ = Text::new("Air Quality Monitor", Point::new(10, 30), title_style).draw(display);

            let value_style = MonoTextStyleBuilder::new()
                .font(&FONT_10X20)
                .text_color(Rgb565::WHITE)
                .background_color(Rgb565::BLACK)
                .build();
            let alert_style = MonoTextStyleBuilder::new()
                .font(&FONT_10X20)
                .text_color(get_aqi_color(frame.pm2_5_atm.into()))
                .background_color(Rgb565::BLACK)
                .build();

            let mut s1: String<64> = String::new();
            let _ = write!(s1, "PM 1.0: {} µg/m³    ", frame.pm1_0_atm);
            let _ = Text::new(&s1, Point::new(10, 70), value_style).draw(display);

            let mut s2: String<64> = String::new();
            let _ = write!(s2, "PM 2.5: {} µg/m³    ", frame.pm2_5_atm);
            let _ = Text::new(&s2, Point::new(10, 100), alert_style).draw(display);

            let mut s3: String<64> = String::new();
            let _ = write!(s3, "PM 10:  {} µg/m³    ", frame.pm10_0_atm);
            let _ = Text::new(&s3, Point::new(10, 130), value_style).draw(display);
        }
        ViewMode::LastHour => {
            draw_graph_view(
                display,
                history,
                "Last Hour",
                60, // 60 minutes
                title_style,
                force_redraw,
            );
        }
        ViewMode::LastDay => {
            draw_graph_view(
                display,
                history,
                "Last 24h",
                HISTORY_HOURS * 60,
                title_style,
                force_redraw,
            );
        }
        ViewMode::Hourly => {
            draw_hourly_view(display, history, title_style, force_redraw);
        }
    }
}

fn draw_graph_view<T>(
    display: &mut T,
    history: &[u16],
    title: &str,
    window_minutes: usize,
    title_style: MonoTextStyle<Rgb565>,
    force_redraw: bool,
) where
    T: DrawTarget<Color = Rgb565>,
{
    if force_redraw {
        let _ = Text::new(title, Point::new(10, 30), title_style).draw(display);
    }

    let value_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(Rgb565::WHITE)
        .background_color(Rgb565::BLACK)
        .build();

    if history.is_empty() {
        let _ = Text::new(
            "Collecting data...",
            Point::new(GRAPH_LEFT, GRAPH_TOP + 20),
            value_style,
        )
        .draw(display);
        return;
    }

    let samples_per_min = 60 / SAMPLE_INTERVAL_SECS as usize;
    let window_samples = window_minutes * samples_per_min;

    // Slice only the relevant part of history for this window
    let current_len = history.len();
    let start_in_history = current_len.saturating_sub(window_samples);
    let history_window = &history[start_in_history..];
    let window_len = history_window.len();

    let max_val = history_window.iter().max().copied().unwrap_or(1);
    let now_val = history.last().copied().unwrap_or(0);

    let mut s: String<16> = String::new();
    let _ = write!(s, "now: {} max: {}", now_val, max_val);
    let _ = Text::new(&s, Point::new(160, 30), value_style).draw(display);

    // Timeline is fixed to window_samples, right-justified
    let shift = window_samples.saturating_sub(window_len);

    for x_offset in 0..GRAPH_WIDTH {
        let pixel_window_end = ((x_offset as usize + 1) * window_samples) / GRAPH_WIDTH as usize;
        let pixel_window_start = (x_offset as usize * window_samples) / GRAPH_WIDTH as usize;

        let start_idx = pixel_window_start.saturating_sub(shift);
        let end_idx = pixel_window_end.saturating_sub(shift);

        let avg_val = if start_idx < window_len && end_idx > start_idx {
            let bucket = &history_window[start_idx..end_idx.min(window_len)];
            let sum: i32 = bucket.iter().map(|&x| i32::from(x)).sum();
            sum / bucket.len() as i32
        } else {
            0
        };

        let x = GRAPH_LEFT + x_offset;
        let scaled_val = (avg_val * GRAPH_HEIGHT) / i32::from(max_val);
        let y = GRAPH_BOTTOM - scaled_val;
        let color = get_aqi_color(avg_val);

        let _ = Line::new(Point::new(x, GRAPH_BOTTOM), Point::new(x, y))
            .into_styled(PrimitiveStyle::with_stroke(color, 1))
            .draw(display);

        if y > GRAPH_TOP {
            let _ = Line::new(Point::new(x, y - 1), Point::new(x, GRAPH_TOP))
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::BLACK, 1))
                .draw(display);
        }
    }
}

fn draw_hourly_view<T>(
    display: &mut T,
    history: &[u16],
    title_style: MonoTextStyle<Rgb565>,
    force_redraw: bool,
) where
    T: DrawTarget<Color = Rgb565>,
{
    if force_redraw {
        let _ = Text::new("24h Hourly", Point::new(10, 30), title_style).draw(display);
    }

    let value_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(Rgb565::WHITE)
        .background_color(Rgb565::BLACK)
        .build();

    if history.is_empty() {
        let _ = Text::new("No data", Point::new(10, 70), value_style).draw(display);
        return;
    }

    let max_val = history.iter().max().copied().unwrap_or(1);
    let now_val = history.last().copied().unwrap_or(0);

    let mut s: String<16> = String::new();
    let _ = write!(s, "now: {} max: {}", now_val, max_val);
    let _ = Text::new(&s, Point::new(160, 30), value_style).draw(display);

    const SAMPLES_PER_HOUR: usize = MAX_HISTORY / HISTORY_HOURS;
    let current_len = history.len();
    let shift = MAX_HISTORY.saturating_sub(current_len);

    for h in 0..HISTORY_HOURS {
        let hour_start = h * SAMPLES_PER_HOUR;
        let hour_end = (h + 1) * SAMPLES_PER_HOUR;

        let start_idx = hour_start.saturating_sub(shift);
        let end_idx = hour_end.saturating_sub(shift);

        let avg_val = if start_idx < current_len && end_idx > start_idx {
            let bucket = &history[start_idx..end_idx.min(current_len)];
            let sum: i32 = bucket.iter().map(|&x| i32::from(x)).sum();
            sum / bucket.len() as i32
        } else {
            0
        };

        const BAR_HALF_WIDTH: i32 = 5;
        let x_center = 25 + h as i32 * 12;
        let scaled_val = (avg_val * 100) / i32::from(max_val);
        let y = GRAPH_BOTTOM - scaled_val;
        let color = get_aqi_color(avg_val);

        for x in (x_center - BAR_HALF_WIDTH)..(x_center + BAR_HALF_WIDTH) {
            let _ = Line::new(Point::new(x, GRAPH_BOTTOM), Point::new(x, y))
                .into_styled(PrimitiveStyle::with_stroke(color, 1))
                .draw(display);
            let _ = Line::new(Point::new(x, y - 1), Point::new(x, 50))
                .into_styled(PrimitiveStyle::with_stroke(Rgb565::BLACK, 1))
                .draw(display);
        }
    }
}
