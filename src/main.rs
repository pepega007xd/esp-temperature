use bme280::{Configuration, IIRFilter, Oversampling};
use dotenvy_macro::dotenv;
use embedded_svc::{
    http::client::Client as HttpClient,
    io::Write,
    wifi,
    wifi::{AuthMethod, ClientConfiguration},
};
use esp_idf_svc::hal::{
    delay::Delay,
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

#[derive(Serialize, Deserialize)]
struct AHT10Value {
    temperature: f32,
    humidity: f32,
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
        &I2cConfig::default().baudrate(1000.into()),
    )?;

    let mut delay = Delay::default();

    #[cfg(feature = "bme280")]
    let mut bme280 = {
        let mut bme280 = bme280::i2c::BME280::new_primary(i2c_driver);

        let config = Configuration::default()
            .with_humidity_oversampling(Oversampling::Oversampling16X)
            .with_pressure_oversampling(Oversampling::Oversampling16X)
            .with_temperature_oversampling(Oversampling::Oversampling16X)
            .with_iir_filter(IIRFilter::Coefficient16);

        bme280.init_with_config(&mut delay, config).unwrap();

        bme280
    };

    #[cfg(feature = "aht10")]
    let mut aht10 = {
        let mut aht10 = adafruit_aht10::AdafruitAHT10::new(i2c_driver);
        aht10.begin()?;
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
        #[cfg(feature = "bme280")]
        let result = bme280.measure(&mut delay).unwrap();

        #[cfg(feature = "aht10")]
        let result = {
            let result = aht10.read_data()?;
            AHT10Value {
                temperature: result.1,
                humidity: result.0,
            }
        };

        #[cfg(not(any(feature = "bme280", feature = "aht10")))]
        let result = compile_error!(
            "enable either the 'bme280' or the 'aht10' feature to build for one of the sensors"
        );

        let result = serde_json::to_string(&result)?;
        let result = result.as_bytes();

        let url = if cfg!(feature = "bme280") {
            format!("http://{API_ADDRESS}/outdoor_sensor")
        } else {
            format!("http://{API_ADDRESS}/indoor_sensor")
        };

        let headers = [
            ("Content-Type", "application/json"),
            ("Content-Length", &result.len().to_string()),
        ];
        let mut request = client.post(&url, &headers)?;
        request.write_all(result)?;
        request.flush()?;
        request.submit()?;

        sleep(Duration::from_secs(60));
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
