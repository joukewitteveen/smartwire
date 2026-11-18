use embassy_futures::yield_now;
use embassy_net::Stack;
use embassy_net::udp::UdpSocket;
use embassy_net::wire::{IpAddr, ListenSocketAddr, SocketAddr};
use embassy_time::Instant;
use log::{debug, warn};

use crate::cosem::ElectricityValue::*;
use crate::cosem::{CosemSource, Obis};

// In the future, get a custom Susy-ID and have variable length data
static TEMPLATE: [u8; 600] = [
    b'S', b'M', b'A', b'\0', // magic signature
    // --- [4]
    0x00, 0x04, 0x02, 0xA0, // length: 4, tag: tag0
    0x00, 0x00, 0x00, 0x01, // default group
    // --- [12]
    0x02, 0x44, 0x00, 0x10, // length: 580, tag: SMA Net 2
    0x60, 0x69, // energy meter protocol
    0x01, 0x5D, // Susy-ID: Emeter-20
    0x71, 0x57, 0x9D, 0x2A, // [20] serial number
    0x00, 0x00, 0x00, 0x00, // [24] millisecond ticker
    // SUMS [28]
    0x00, 0x01, 0x04, 0x00, // OBIS(bcde): 1s average power (use 0.1.7.0, instantaneous)
    0x00, 0x00, 0x00, 0x00, // [32] 0.1W resolution
    0x00, 0x01, 0x08, 0x00, // OBIS: energy (meter reading) (sum 0.1.8.1 and 0.1.8.2)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // [40] 1J resolution
    0x00, 0x02, 0x04, 0x00, // OBIS: power returned (use 0.2.7.0)
    0x00, 0x00, 0x00, 0x00, // [52]
    0x00, 0x02, 0x08, 0x00, // OBIS: energy returned (sum 0.2.8.1 and 0.2.8.2)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // [60]
    0x00, 0x03, 0x04, 0x00, // OBIS: reactive power (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x03, 0x08, 0x00, // OBIS: reactive energy (set to zero)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x04, 0x04, 0x00, // OBIS: reactive power returned (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x04, 0x08, 0x00, // OBIS: reactive energy returned (set to zero)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x09, 0x04, 0x00, // OBIS: apparent power (use 0.1.7.0, active instantaneous)
    0x00, 0x00, 0x00, 0x00, // [112]
    0x00, 0x09, 0x08, 0x00, // OBIS: apparent energy (sum 0.1.8.1 and 0.1.8.2, active)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // [120]
    0x00, 0x0A, 0x04, 0x00, // OBIS: apparent power returned (use 0.2.7.0)
    0x00, 0x00, 0x00, 0x00, // [132]
    0x00, 0x0A, 0x08, 0x00, // OBIS: apparent energy returned (sum 0.2.8.1 and 0.2.8.2)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // [140]
    0x00, 0x0D, 0x04, 0x00, // OBIS: power factor (set to 1)
    0x00, 0x00, 0x03, 0xE8, // 0.001 resolution
    // L1 [156]
    0x00, 0x15, 0x04, 0x00, // OBIS: averaged power (use 0.21.7.0, instantaneous)
    0x00, 0x00, 0x00, 0x00, // [160]
    0x00, 0x15, 0x08, 0x00, // OBIS: energy (sum 0.1.8.1 and 0.1.8.2, total)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // [168]
    0x00, 0x16, 0x04, 0x00, // OBIS: power returned (use 0.22.7.0)
    0x00, 0x00, 0x00, 0x00, // [180]
    0x00, 0x16, 0x08, 0x00, // OBIS: energy returned (sum 0.2.8.1 and 0.2.8.2)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // [188]
    0x00, 0x17, 0x04, 0x00, // OBIS: reactive power (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x17, 0x08, 0x00, // OBIS: reactive energy (set to zero)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x18, 0x04, 0x00, // OBIS: reactive power returned (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x18, 0x08, 0x00, // OBIS: reactive energy returned (set to zero)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x1D, 0x04, 0x00, // OBIS: apparent power (use 0.21.7.0)
    0x00, 0x00, 0x00, 0x00, // [240]
    0x00, 0x1D, 0x08, 0x00, // OBIS: apparent energy (sum 0.1.8.1 and 0.1.8.2)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // [248]
    0x00, 0x1E, 0x04, 0x00, // OBIS: apparent power returned (use 0.22.7.0)
    0x00, 0x00, 0x00, 0x00, // [260]
    0x00, 0x1E, 0x08, 0x00, // OBIS: apparent energy returned (sum 0.2.8.1 and 0.2.8.2)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // [268]
    0x00, 0x1F, 0x04, 0x00, // OBIS: averaged current (use 0.31.7.0, instantaneous)
    0x00, 0x00, 0x00, 0x00, // [280] 0.001A resolution
    0x00, 0x20, 0x04, 0x00, // OBIS: averaged voltage (use 0.32.7.0)
    0x00, 0x00, 0x00, 0x00, // [288] 0.001V resolution
    0x00, 0x21, 0x04, 0x00, // OBIS: power factor (set to 1)
    0x00, 0x00, 0x03, 0xE8, //
    // L2 [300]
    0x00, 0x29, 0x04, 0x00, // OBIS: averaged power (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x29, 0x08, 0x00, // OBIS: energy (set to zero)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x2A, 0x04, 0x00, // OBIS: power returned (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x2A, 0x08, 0x00, // OBIS: energy returned (set to zero)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x2B, 0x04, 0x00, // OBIS: reactive power (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x2B, 0x08, 0x00, // OBIS: reactive energy (set to zero)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x2C, 0x04, 0x00, // OBIS: reactive power returned (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x2C, 0x08, 0x00, // OBIS: reactive energy returned (set to zero)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x31, 0x04, 0x00, // OBIS: apparent power (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x31, 0x08, 0x00, // OBIS: apparent energy (set to zero)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x32, 0x04, 0x00, // OBIS: apparent power returned (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x32, 0x08, 0x00, // OBIS: apparent energy returned (set to zero)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x33, 0x04, 0x00, // OBIS: current (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x34, 0x04, 0x00, // OBIS: voltage (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x35, 0x04, 0x00, // power factor (set to 0?!)
    0x00, 0x00, 0x00, 0x00, //
    // L3 [444]
    0x00, 0x3D, 0x04, 0x00, // OBIS: averaged power (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x3D, 0x08, 0x00, // OBIS: energy (set to zero)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x3E, 0x04, 0x00, // OBIS: power returned (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x3E, 0x08, 0x00, // OBIS: energy returned (set to zero)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x3F, 0x04, 0x00, // OBIS: reactive power (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x3F, 0x08, 0x00, // OBIS: reactive energy (set to zero)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x40, 0x04, 0x00, // OBIS: reactive power returned (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x40, 0x08, 0x00, // OBIS: reactive energy returned (set to zero)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x45, 0x04, 0x00, // OBIS: apparent power (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x45, 0x08, 0x00, // OBIS: apparent energy (set to zero)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x46, 0x04, 0x00, // OBIS: apparent power returned (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x46, 0x08, 0x00, // OBIS: apparent energy returned (set to zero)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, //
    0x00, 0x47, 0x04, 0x00, // OBIS: current (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x48, 0x04, 0x00, // OBIS: voltage (set to zero)
    0x00, 0x00, 0x00, 0x00, //
    0x00, 0x49, 0x04, 0x00, // power factor (set to 0?!)
    0x00, 0x00, 0x00, 0x00, //
    // OTHER [588]
    0x90, 0x00, 0x00, 0x00, // OBIS: software version
    0x02, 0x00, 0x12, b'R', //
    // --- [596]
    0x00, 0x00, 0x00, 0x00, // length: 0, tag: end
];

#[repr(usize)]
enum Channel {
    SerialNumber = 20,
    MillisecondTicker = 24,
    Power = 32,
    Energy = 40,
    PowerReturned = 52,
    EnergyReturned = 60,
    PowerApparent = 112,
    EnergyApparent = 120,
    PowerApparentReturned = 132,
    EnergyApparentReturned = 140,
    L1Power = 160,
    L1Energy = 168,
    L1PowerReturned = 180,
    L1EnergyReturned = 188,
    L1PowerApparent = 240,
    L1EnergyApparent = 248,
    L1PowerApparentReturned = 260,
    L1EnergyApparentReturned = 268,
    L1Current = 280,
    L1Voltage = 288,
}

fn write_value(buf: &mut [u8], channel: Channel, value: &[u8]) {
    let offset = channel as usize;
    buf[offset..(offset + value.len())].copy_from_slice(value)
}

// Unfortunately, this is very DSMR-specific
fn fill_from(buf: &mut [u8], source: &mut impl CosemSource) -> bool {
    use Channel::*;

    const ENERGY_IMPORTED: u8 = 1 << 0; // 1-0:1.8.1 + 1-0:1.8.2
    const ENERGY_EXPORTED: u8 = 1 << 1; // 1-0:2.8.1 + 1-0:2.8.2
    const POWER_IMPORTED: u8 = 1 << 2; // 1-0:1.7.0
    const POWER_EXPORTED: u8 = 1 << 3; // 1-0:2.7.0
    const L1_POWER_IMPORTED: u8 = 1 << 4; // 1-0:21.7.0
    const L1_POWER_EXPORTED: u8 = 1 << 5; // 1-0:22.7.0
    const L1_CURRENT: u8 = 1 << 6; // 1-0:31.7.0
    const L1_VOLTAGE: u8 = 1 << 7; // 1-0:32.7.0

    let mut populated: u8 = 0;
    let mut energy_imported_other: Option<u64> = None;
    let mut energy_exported_other: Option<u64> = None;
    source
        .receive_message(|obis, value| {
            debug!("{obis}: {value:?}");
            match obis {
                Obis(1, 0, 1, 8, 1 | 2) => {
                    if let KiloJoule(kilojoule) = value {
                        if let Some(other_tariff) = energy_imported_other {
                            let joule: [u8; 8] = other_tariff
                                .wrapping_add(kilojoule.into())
                                .wrapping_mul(1000)
                                .to_be_bytes();
                            for channel in [Energy, EnergyApparent, L1Energy, L1EnergyApparent] {
                                write_value(buf, channel, &joule);
                            }
                            populated |= ENERGY_IMPORTED;
                        } else {
                            energy_imported_other = Some(kilojoule.into());
                        }
                    }
                }
                Obis(1, 0, 2, 8, 1 | 2) => {
                    if let KiloJoule(kilojoule) = value {
                        if let Some(other_tariff) = energy_exported_other {
                            let joule: [u8; 8] = other_tariff
                                .wrapping_add(kilojoule.into())
                                .wrapping_mul(1000)
                                .to_be_bytes();
                            for channel in [
                                EnergyReturned,
                                EnergyApparentReturned,
                                L1EnergyReturned,
                                L1EnergyApparentReturned,
                            ] {
                                write_value(buf, channel, &joule);
                            }
                            populated |= ENERGY_EXPORTED;
                        } else {
                            energy_exported_other = Some(kilojoule.into());
                        }
                    }
                }
                Obis(1, 0, 1, 7, 0) => {
                    if let MilliWatt(milliwatt) = value {
                        let deciwatt: [u8; 4] = (milliwatt / 100).to_be_bytes();
                        for channel in [Power, PowerApparent] {
                            write_value(buf, channel, &deciwatt);
                        }
                        populated |= POWER_IMPORTED;
                    }
                }
                Obis(1, 0, 2, 7, 0) => {
                    if let MilliWatt(milliwatt) = value {
                        let deciwatt: [u8; 4] = (milliwatt / 100).to_be_bytes();
                        for channel in [PowerReturned, PowerApparentReturned] {
                            write_value(buf, channel, &deciwatt);
                        }
                        populated |= POWER_EXPORTED;
                    }
                }
                Obis(1, 0, 21, 7, 0) => {
                    if let MilliWatt(milliwatt) = value {
                        let deciwatt: [u8; 4] = (milliwatt / 100).to_be_bytes();
                        for channel in [L1Power, L1PowerApparent] {
                            write_value(buf, channel, &deciwatt);
                        }
                        populated |= L1_POWER_IMPORTED;
                    }
                }
                Obis(1, 0, 22, 7, 0) => {
                    if let MilliWatt(milliwatt) = value {
                        let deciwatt: [u8; 4] = (milliwatt / 100).to_be_bytes();
                        for channel in [L1PowerReturned, L1PowerApparentReturned] {
                            write_value(buf, channel, &deciwatt);
                        }
                        populated |= L1_POWER_EXPORTED;
                    }
                }
                Obis(1, 0, 31, 7, 0) => {
                    if let MilliAmpere(milliampere) = value {
                        let milliampere: [u8; 4] = milliampere.to_be_bytes();
                        write_value(buf, L1Current, &milliampere);
                        populated |= L1_CURRENT;
                    }
                }
                Obis(1, 0, 32, 7, 0) => {
                    if let MilliVolt(millivolt) = value {
                        let millivolt: [u8; 4] = millivolt.to_be_bytes();
                        write_value(buf, L1Voltage, &millivolt);
                        populated |= L1_VOLTAGE;
                    }
                }
                _ => {} // Discarded
            }
        })
        .inspect(|maybe_datetime| {
            maybe_datetime
                .as_ref()
                .inspect(|datetime| debug!("{datetime:?}"));
        })
        .is_ok()
        && (populated
            == (ENERGY_IMPORTED
                | ENERGY_EXPORTED
                | POWER_IMPORTED
                | POWER_EXPORTED
                | L1_POWER_IMPORTED
                | L1_POWER_EXPORTED
                | L1_CURRENT
                | L1_VOLTAGE))
}

pub struct Runner<'a> {
    socket: UdpSocket<'a>,
    serial: Option<[u8; 4]>,
}

impl<'a> Runner<'a> {
    pub fn new(network: Stack<'a>) -> Self {
        let mut speedwire_socket = UdpSocket::new(network).unwrap();
        speedwire_socket
            .bind(
                ListenSocketAddr::UNSPECIFIED,
                ListenSocketAddr {
                    addr: Some(IpAddr::v4(239, 12, 255, 254)),
                    port: 9522,
                },
            )
            .unwrap();
        let first_iface = network.ifaces().next().unwrap();

        Self {
            socket: speedwire_socket,
            serial: first_iface
                .hardware_addr()
                .ethernet()
                .and_then(|addr| addr.as_bytes()[2..].try_into().ok()),
        }
    }

    pub async fn run(&mut self, source: &mut impl CosemSource) -> ! {
        let mut send_failures = 0u32;
        loop {
            // Get a packet buffer before sending so that we don't wait for one later
            if let Err(error) = self
                .socket
                .send_to_with(TEMPLATE.len(), SocketAddr::UNSPECIFIED, |buf| {
                    buf.copy_from_slice(&TEMPLATE);
                    // Serial number
                    if let Some(serial) = self.serial {
                        write_value(buf, Channel::SerialNumber, &serial);
                    }
                    while !fill_from(buf, source) {}
                    // Millisecond ticker
                    write_value(
                        buf,
                        Channel::MillisecondTicker,
                        &(Instant::now().as_millis() as u32).to_be_bytes(),
                    );
                    (TEMPLATE.len(), ())
                })
                .await
            {
                send_failures += 1;
                warn!("Could not send telegram ({}): {:?}", send_failures, error);
            }
            // iface.flush();
            // There is only one Tx slot so waiting for a packet buffer at the top
            // of the loop flushes the send queue.
            yield_now().await; // Give other tasks a chance
        }
    }
}
