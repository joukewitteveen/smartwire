#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_unopt_stack_frames)]

mod board;
mod cosem;
mod crc;
mod emeter;
mod line_reader;
mod number_reader;
mod p1;

use embassy_executor::Spawner;
use esp_backtrace as _;
use log::{debug, info};

use board::Board;
use emeter::Runner;
use p1::P1Reader;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_unopt_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let mut board = Board::init(spawner).await;
    debug!("Embassy initialized");

    board.network.set_hostname("SmartWire-Dongle").unwrap();
    let first_iface = board.network.ifaces().next().unwrap();
    first_iface.set_dhcpv4(Some(Default::default())).unwrap();
    info!("Waiting for DHCP");
    first_iface.wait_config_up().await;
    info!(
        "IP address: {:?}",
        first_iface.dhcpv4_lease().unwrap().address.address()
    );

    // Output speedwire telegrams for DSMR telegrams
    let mut p1_reader = P1Reader::new(board.input, &mut board.output, &board.echo_request);
    let mut emeter = Runner::new(board.network);
    emeter.run(&mut p1_reader).await;
}
