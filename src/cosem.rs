// Based on IEC 62056

use core::fmt;

use crate::number_reader::{PeekableBytes, next_number};

pub enum ReceiveError {
    MessageCorrupted,
}

pub trait CosemSource {
    // Calls the callback on objects in the massage.
    // Returns any message timestamp if reception was good.
    fn receive_message(
        &mut self,
        electricity: impl FnMut(Obis, ElectricityValue),
    ) -> Result<Option<DateTime>, ReceiveError>;
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub dst: bool,
}

// Chosen for maximum resolution inside a u32
#[derive(Debug, Copy, Clone)]
pub enum ElectricityValue {
    KiloJoule(u32),
    MilliAmpere(u32),
    MilliVolt(u32),
    MilliWatt(u32),
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct Obis(pub u8, pub u8, pub u8, pub u8, pub u8);

impl fmt::Display for Obis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}:{}.{}.{}", self.0, self.1, self.2, self.3, self.4)
    }
}

impl Obis {
    pub fn try_parse(bytes: &mut PeekableBytes) -> Option<Self> {
        let a = next_number(bytes).and_then(|a| (a <= 15).then_some(a as u8))?;
        bytes.next_if_eq(&&b'-')?;
        let b = next_number(bytes)?.try_into().ok()?;
        bytes.next_if_eq(&&b':')?;
        let c = next_number(bytes)?.try_into().ok()?;
        bytes.next_if_eq(&&b'.')?;
        let d = next_number(bytes)?.try_into().ok()?;
        bytes.next_if_eq(&&b'.')?;
        let e = next_number(bytes)?.try_into().ok()?;
        Some(Self(a, b, c, d, e))
    }
}
