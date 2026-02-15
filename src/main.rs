use dotenvy_macro::dotenv;
use embedded_svc::{
    http::client::Client as HttpClient,
    io::Write,
    wifi,
    wifi::{AuthMethod, ClientConfiguration},
};
use esp_idf_svc::hal::{
    i2c::{I2cConfig, I2cDriver},
    prelude::Peripherals,
};
use esp_idf_svc::http::client::EspHttpConnection;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    wifi::{BlockingWifi, EspWifi},
};
use serde::{Deserialize, Serialize};
use std::{thread::sleep, time::Duration};

const API_ADDRESS: &str = "10.0.0.2:8002";

const SSID: &str = dotenv!("WIFI_SSID");
const PASSWORD: &str = dotenv!("WIFI_PASS");

const OVERSAMPLE: u64 = 16;

#[derive(Serialize, Deserialize)]
struct AHT10Value {
    temperature: f32,
    humidity: f32,
    pressure: f32,
}

fn main() {
    if let Err(e) = run_application() {
        println!("{e:#?}");
        sleep(Duration::from_secs(60));
        esp_idf_svc::hal::reset::restart();
    }
}

fn run_application() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // setup the sensor

    let peripherals = Peripherals::take()?;

    let i2c_driver = I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio5,
        peripherals.pins.gpio4,
        &I2cConfig::default().baudrate(10000.into()),
    )?;

    let mut aht10 = {
        let mut aht10 = adafruit_aht10::AdafruitAHT10::new(i2c_driver);
        aht10.begin()?;
        aht10.read_data()?;
        aht10
    };

    // setup wifi

    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
        sys_loop,
    )?;

    connect_wifi(&mut wifi)?;

    let mut client = HttpClient::wrap(EspHttpConnection::new(&Default::default())?);

    loop {
        let mut temperature = 0.0;
        let mut humidity = 0.0;

        for _ in 0..OVERSAMPLE {
            let result = aht10.read_data()?;
            temperature += result.1;
            humidity += result.0;

            sleep(Duration::from_millis(60_000 / OVERSAMPLE));
        }

        let result = AHT10Value {
            temperature: temperature / OVERSAMPLE as f32,
            humidity: humidity / OVERSAMPLE as f32,
            pressure: 0.0,
        };

        let result = serde_json::to_string(&result)?;
        let result = result.as_bytes();

        let url = format!("http://{API_ADDRESS}/indoor_sensor");

        let headers = [
            ("Content-Type", "application/json"),
            ("Content-Length", &result.len().to_string()),
        ];
        let mut request = client.post(&url, &headers)?;
        request.write_all(result)?;
        request.flush()?;
        request.submit()?;
    }
}

fn connect_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>) -> anyhow::Result<()> {
    let wifi_configuration: wifi::Configuration =
        wifi::Configuration::Client(ClientConfiguration {
            ssid: SSID.try_into().unwrap(),
            auth_method: AuthMethod::WPA2Personal,
            password: PASSWORD.try_into().unwrap(),
            ..Default::default()
        });

    wifi.set_configuration(&wifi_configuration)?;
    wifi.start()?;
    wifi.connect()?;
    wifi.wait_netif_up()?;

    Ok(())
}
