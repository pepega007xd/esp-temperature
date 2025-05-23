# ESP32C6 temperature sensor

Firmware that reads data from a sensor and sends them to an HTTP endpoint as JSON.

## Usage

- Install the ESP-IDF toolchain for ESP32C6 according to [this guide](https://docs.esp-rs.org/book/installation/index.html).

- Connect either the AHT10 or the BME280 sensor via I2C (GPIO 4 -> SCL, GPIO 5 -> SDA)

- create a .env file in the root of this repository and specify the wifi credentials:
  ```
  WIFI_SSID=network-name
  WIFI_PASS=network-password
  ```

- Build and flash with a feature flag corresponding to the connected sensor: `cargo run --feature bme280` or `cargo run --feature aht10`
