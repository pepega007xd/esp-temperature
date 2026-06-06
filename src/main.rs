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
    peripherals::Peripherals,
};
use esp_idf_svc::http::client::EspHttpConnection;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    wifi::{BlockingWifi, EspWifi},
};
use std::{thread::sleep, time::Duration};

const API_ADDRESS: &str = "10.0.0.2:8002";

const SSID: &str = dotenv!("WIFI_SSID");
const PASSWORD: &str = dotenv!("WIFI_PASS");

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

    let peripherals = Peripherals::take()?;

    let i2c_driver = I2cDriver::new(
        peripherals.i2c0,
        peripherals.pins.gpio5,
        peripherals.pins.gpio4,
        &I2cConfig::default().baudrate(10_000.into()),
    )?;

    let mut delay = Delay::default();

    let mut aht20 = aht20_driver::AHT20::new(i2c_driver, aht20_driver::SENSOR_ADDRESS);
    let mut aht20 = aht20.init(&mut delay).unwrap();

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
        let aht20_result = aht20.measure(&mut delay)?;
        println!("aht20: {aht20_result:?}");

        // #[cfg(not(any(feature = "indoor_sensor", feature = "outdoor_sensor")))]
        // let result = compile_error!(
        //     "enable either the 'indoor_sensor' or the 'outdoor_sensor' feature to build for one of the sensors"
        // );
        //
        // let result = serde_json::to_string(&result)?;
        // let result = result.as_bytes();
        //
        // let url = if cfg!(feature = "outdoor_sensor") {
        //     format!("http://{API_ADDRESS}/outdoor_sensor")
        // } else {
        //     format!("http://{API_ADDRESS}/indoor_sensor")
        // };
        //
        // let headers = [
        //     ("Content-Type", "application/json"),
        //     ("Content-Length", &result.len().to_string()),
        // ];
        // let mut request = client.post(&url, &headers)?;
        // request.write_all(result)?;
        // request.flush()?;
        // request.submit()?;

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
