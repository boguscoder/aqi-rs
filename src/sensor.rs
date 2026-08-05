use esp_hal::{uart::Uart, Async};

const MAGIC1: u8 = 0x42;
const MAGIC2: u8 = 0x4D;
const COMMAND_SIZE: usize = 7;
const CHECKSUM_SIZE: usize = 2;

#[derive(Copy, Clone, Debug, Default)]
pub struct PmsReading {
    pub pm1_0_atm: u16,
    pub pm2_5_atm: u16,
    pub pm10_0_atm: u16,
}

pub struct Pms5003 {
    uart: Uart<'static, Async>,
}

impl Pms5003 {
    pub fn new(uart: Uart<'static, Async>) -> Self {
        Self { uart }
    }

    pub async fn read(&mut self) -> Option<PmsReading> {
        let mut magic = [0u8; 1];

        loop {
            if self.uart.read_exact_async(&mut magic).await.is_ok()
                && magic[0] == MAGIC1
                && self.uart.read_exact_async(&mut magic).await.is_ok()
                && magic[0] == MAGIC2
            {
                break;
            }
        }

        let mut payload = [0u8; 30];
        if self.uart.read_exact_async(&mut payload).await.is_ok() {
            let pm1_0_atm = u16::from_be_bytes([payload[8], payload[9]]);
            let pm2_5_atm = u16::from_be_bytes([payload[10], payload[11]]);
            let pm10_0_atm = u16::from_be_bytes([payload[12], payload[13]]);

            return Some(PmsReading {
                pm1_0_atm,
                pm2_5_atm,
                pm10_0_atm,
            });
        }

        None
    }

    pub async fn sleep(&mut self) -> Result<(), ()> {
        self.uart.write_async(&Self::to_command(0xe4, 0)).await.ok();
        Ok(())
    }

    pub async fn wake(&mut self) -> Result<(), ()> {
        self.uart.write_async(&Self::to_command(0xe4, 1)).await.ok();
        Ok(())
    }

    fn to_command(cmd: u8, data: u16) -> [u8; COMMAND_SIZE] {
        let mut buf = [0_u8; COMMAND_SIZE];

        buf[0] = MAGIC1;
        buf[1] = MAGIC2;
        buf[2] = cmd;

        let data_bytes = data.to_be_bytes();
        buf[3..5].copy_from_slice(&data_bytes);

        let checksum = buf
            .iter()
            .take(COMMAND_SIZE - CHECKSUM_SIZE)
            .map(|b| *b as u16)
            .sum::<u16>();

        let checksum = checksum.to_be_bytes();
        buf[5..7].copy_from_slice(&checksum);

        buf
    }
}
