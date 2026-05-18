use esp_hal::{
    gpio::AnyPin,
    uart::{Config as UartConfig, Uart},
};
use pmsx003::PmsX003Sensor;

pub fn init_sensor<'d>(
    uart_periph: esp_hal::peripherals::UART1<'d>,
    tx: AnyPin<'d>,
    rx: AnyPin<'d>,
) -> PmsX003Sensor<Uart<'d, esp_hal::Blocking>> {
    let uart_config = UartConfig::default().with_baudrate(9600);
    let uart = Uart::new(uart_periph, uart_config)
        .unwrap()
        .with_tx(tx)
        .with_rx(rx);
    PmsX003Sensor::new(uart)
}
