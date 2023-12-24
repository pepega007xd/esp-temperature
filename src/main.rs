use serde::{Deserialize, Serialize};
use std::io::Write;
use std::time::Duration;

use esp_idf_hal::{
    i2c::{I2cConfig, I2cDriver},
    prelude::Peripherals,
};

#[derive(Serialize, Deserialize)]
struct AHT10Value {
    temperature: f32,
    humidity: f32,
}

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    // esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("starting...");

    let peripherals = Peripherals::take()?;
    let scl = peripherals.pins.gpio25;
    let sda = peripherals.pins.gpio26;
    let config = I2cConfig::new().baudrate(100_000.into());
    let i2c = I2cDriver::new(peripherals.i2c0, sda, scl, &config)?;

    let mut aht10 = adafruit_aht10::AdafruitAHT10::new(i2c);
    aht10.begin()?;

    let stdout = std::io::stdout();

    loop {
        std::thread::sleep(Duration::from_secs(1));

        let reading = aht10.read_data()?;
        let reading = AHT10Value {
            temperature: reading.1,
            humidity: reading.0,
        };

        serde_json::to_writer(&stdout, &reading)?;
        writeln!(&stdout, "")?;
    }
}
