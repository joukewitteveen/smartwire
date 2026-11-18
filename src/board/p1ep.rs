use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_net_wiznet::{Device as EthDevice, Runner as EthRunner};
use embassy_time::Delay;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::clock::CpuClock;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::uart::{Uart, UartRx, UartTx};
use esp_hal::{Async, efuse, gpio};
use log::debug;
use rand_core::Rng;
use static_cell::StaticCell;

use crate::board::Get;

#[embassy_executor::task]
async fn ethernet_task(
    runner: EthRunner<
        'static,
        embassy_net_wiznet::chip::W5500,
        ExclusiveDevice<Spi<'static, Async>, gpio::Output<'static>, Delay>,
        gpio::Input<'static>,
        gpio::NoPin,
    >,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static>) -> ! {
    runner.run().await
}

impl Get<bool> for gpio::Input<'_> {
    fn get(&self) -> bool {
        self.is_high()
    }
}

pub struct Board {
    pub input: UartRx<'static, Async>,
    pub output: UartTx<'static, Async>,
    pub echo_request: gpio::Input<'static>,
    pub network: Stack<'static>,
}

impl Board {
    pub async fn init(spawner: Spawner) -> Self {
        let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
        let peripherals = esp_hal::init(config);
        let timg0 = TimerGroup::new(peripherals.TIMG0);
        let sleep = esp_rtos::sleep::configure(peripherals.LPWR);
        esp_rtos::start_with_idle_hook(
            timg0.timer0,
            peripherals.FROM_CPU_INTR0,
            sleep.light_sleep_hook,
        );

        let mut rng = esp_hal::rng::Rng::new();

        // The following pins are used to bootstrap the chip.
        let _gpio2 = peripherals.GPIO2;
        let _gpio8 = peripherals.GPIO8;
        let _gpio9 = peripherals.GPIO9;
        // These GPIO pins are in use by some feature of the module and should not be used.
        let _gpio11 = peripherals.GPIO11;
        let _gpio12 = peripherals.GPIO12;
        let _gpio13 = peripherals.GPIO13;
        let _gpio14 = peripherals.GPIO14;
        let _gpio15 = peripherals.GPIO15;
        let _gpio16 = peripherals.GPIO16;
        let _gpio17 = peripherals.GPIO17;

        // UART/P1
        let (gpio0_rx, _gpio0_tx) = unsafe { peripherals.GPIO0.split() };
        let (input, output) = Uart::new(peripherals.UART0, Default::default())
            .unwrap()
            .with_rx(gpio0_rx.with_input_inverter(true))
            .with_tx(peripherals.GPIO7)
            .into_async() // Async reading gives us an opportunity to sleep
            .split();

        let echo_request = gpio::Input::new(peripherals.GPIO3, gpio::InputConfig::default());

        // Ethernet
        // We are not using the built-in Wi-Fi, so we can use its MAC for Ethernet
        let mac_address = efuse::base_mac_address();
        debug!("efuse MAC address: {}", mac_address);
        let spi = Spi::new(
            peripherals.SPI2,
            SpiConfig::default().with_frequency(Rate::from_mhz(20)), // 40 MHz can be unstable
        )
        .unwrap()
        .with_sck(peripherals.GPIO10)
        .with_mosi(peripherals.GPIO5)
        .with_miso(peripherals.GPIO4)
        .into_async();

        // Only 1 Tx slot for unbuffered sending
        static STATE: StaticCell<embassy_net_wiznet::State<8, 1>> = StaticCell::new();
        let (ethernet_device, ethernet_runner) = embassy_net_wiznet::new(
            mac_address.as_bytes().try_into().unwrap(),
            STATE.init_with(embassy_net_wiznet::State::new),
            ExclusiveDevice::new(
                spi,
                gpio::Output::new(
                    peripherals.GPIO1,
                    gpio::Level::High,
                    gpio::OutputConfig::default(),
                ),
                Delay,
            )
            .unwrap(),
            gpio::Input::new(peripherals.GPIO6, gpio::InputConfig::default()),
            gpio::NoPin,
        )
        .await
        .unwrap();
        spawner.spawn(ethernet_task(ethernet_runner).unwrap());

        // Network
        static STACK: StaticCell<embassy_net::StackStorage> = StaticCell::new();
        // Because of the unbuffered send queue, a pool size of 9 suffices
        static POOL: embassy_net::StaticPool<1514, 9, 1> = embassy_net::StaticPool::new();
        let (network, net_runner) = embassy_net::Stack::new(
            STACK.init_with(embassy_net::StackStorage::new),
            &POOL,
            rng.next_u64(),
        );
        static DEVICE: StaticCell<EthDevice<'static>> = StaticCell::new();
        let _ = network
            .add_iface_borrowed(DEVICE.init(ethernet_device))
            .unwrap();
        spawner.spawn(net_task(net_runner).unwrap());

        // TODO: Button, LED, Flash

        Self {
            input,
            output,
            echo_request,
            network,
        }
    }
}
