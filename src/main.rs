use bme280::{Configuration, IIRFilter, Oversampling};
use dotenvy_macro::dotenv;
use embedded_hal_bus::i2c as i2c_bus;
use embedded_svc::{
    http::client::Client as HttpClient,
    io::Write,
    wifi,
    wifi::{AuthMethod, ClientConfiguration},
};
use esp_idf_svc::hal::{
    delay::Delay,
    i2c::{I2cConfig, I2cDriver},
    peripherals::Peripherals,
};
use esp_idf_svc::http::client::EspHttpConnection;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    wifi::{BlockingWifi, EspWifi},
};
use std::{cell::RefCell, thread::sleep, time::Duration};

const API_ADDRESS: &str = "10.0.0.2:8002";

const SSID: &str = dotenv!("WIFI_SSID");
const PASSWORD: &str = dotenv!("WIFI_PASS");

#[derive(serde::Serialize)]
struct SensorData {
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

    // setup the sensors

    let mut delay = Delay::default();

    let peripherals = Peripherals::take()?;

    let i2c_driver = I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio5, // SDA
        peripherals.pins.gpio4, // SCL
        &I2cConfig::default().baudrate(10_000.into()),
    )?;
    let i2c_ref_cell = RefCell::new(i2c_driver);

    let mut bmp280 = bme280::i2c::BME280::new_secondary(i2c_bus::RefCellDevice::new(&i2c_ref_cell));
    let config = Configuration::default()
        .with_humidity_oversampling(Oversampling::Oversampling16X)
        .with_pressure_oversampling(Oversampling::Oversampling16X)
        .with_temperature_oversampling(Oversampling::Oversampling16X)
        .with_iir_filter(IIRFilter::Coefficient16);
    bmp280.init_with_config(&mut delay, config)?;

    let mut aht20 = aht20_driver::AHT20::new(
        i2c_bus::RefCellDevice::new(&i2c_ref_cell),
        aht20_driver::SENSOR_ADDRESS,
    );
    let mut aht20 = aht20.init(&mut delay)?;

    // setup wifi

    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs))?,
        sys_loop,
    )?;

    connect_wifi(&mut wifi)?;

    let mut client = HttpClient::wrap(EspHttpConnection::new(&Default::default())?);

    // read invalid data
    for _ in 0..3 {
        let _ = aht20.measure(&mut delay)?;
        let _ = bmp280.measure(&mut delay)?;
        sleep(Duration::from_secs(1));
    }

    loop {
        let aht20_result = aht20.measure(&mut delay)?;
        let bmp280_result = bmp280.measure(&mut delay)?;

        let result = SensorData {
            temperature: bmp280_result.temperature,
            humidity: aht20_result.humidity,
            pressure: bmp280_result.pressure,
        };
        let result = serde_json::to_string(&result)?;
        let result = result.as_bytes();

        #[cfg(not(any(feature = "indoor_sensor", feature = "outdoor_sensor")))]
        compile_error!(
            "enable either the 'indoor_sensor' or the 'outdoor_sensor' feature to build for one of the sensors"
        );

        let url = if cfg!(feature = "outdoor_sensor") {
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
