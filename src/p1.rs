use embedded_io::{Read, Write};
use log::warn;

use crate::board::Get;
use crate::cosem::{CosemSource, DateTime, ElectricityValue, Obis, ReceiveError};
use crate::crc::{CrcState, try_parse as try_parse_crc};
use crate::line_reader::Lines;
use crate::number_reader::{next_number, next_number_decimals, remainder_subslice};

impl DateTime {
    fn parse_component(bytes: &[u8; 2]) -> Option<u8> {
        let mut reader = bytes.iter().peekable();
        let value = next_number(&mut reader)?;
        // Both digits are read, the value cannot overflow a byte
        reader.peek().is_none().then_some(value as u8)
    }

    pub fn try_parse_p1(bytes: &[u8]) -> Option<Self> {
        if let (&[year, month, day, hour, minute, second], &[dst]) = bytes.as_chunks::<2>() {
            Some(Self {
                year: u16::from(Self::parse_component(&year)?) + 2000,
                month: Self::parse_component(&month)?,
                day: Self::parse_component(&day)?,
                hour: Self::parse_component(&hour)?,
                minute: Self::parse_component(&minute)?,
                second: Self::parse_component(&second)?,
                dst: dst == b'S',
            })
        } else {
            None
        }
    }
}

impl ElectricityValue {
    fn checked_wh_to_kj(wh: u32) -> Option<u32> {
        // A maximally accurate multiplication by 3.6
        let triple = wh.checked_mul(3)?;
        triple.checked_add(triple / 5)
    }

    pub fn try_parse_p1(bytes: &[u8]) -> Option<Self> {
        let (value, unit) = bytes.split_once(|&b| b == b'*')?;
        let mut reader = value.iter().peekable();
        let parsed = match unit {
            // P1 resolution: 0.001 kWh
            b"kWh" => next_number_decimals(&mut reader, 3)
                .and_then(Self::checked_wh_to_kj)
                .map(Self::KiloJoule),
            // P1 resolution: 0.001 W
            b"kW" => next_number_decimals(&mut reader, 6).map(Self::MilliWatt),
            // P1 resolution: 0.1 V
            b"V" => next_number_decimals(&mut reader, 3).map(Self::MilliVolt),
            // P1 resolution: 1 A
            b"A" => next_number_decimals(&mut reader, 3).map(Self::MilliAmpere),
            _ => None,
        };
        if reader.peek().is_some() {
            // Incomplete read
            None
        } else {
            parsed
        }
    }
}

enum CrcResult {
    Match,
    Mismatch,
}

enum ReadResult<'a> {
    Line(&'a [u8]),
    End(CrcResult),
}

struct P1Telegram<'a, R, W> {
    lines: &'a mut Lines<R>,
    forward: Option<&'a mut W>,
    crc_state: CrcState,
}

impl<'a, R: Read, W: Write> P1Telegram<'a, R, W> {
    // Discard lines until the next telegram
    fn from_next_begin(
        lines: &'a mut Lines<R>,
        output: &'a mut W,
        echo_request: &impl Get<bool>,
    ) -> Self {
        let mut line;
        loop {
            line = lines.next();
            if Some(&b'/') == line.first() {
                // Start of a telegram
                break;
            } else {
                warn!("Line outside of P1 telegram: {}", unsafe {
                    str::from_utf8_unchecked(line.trim_ascii_end())
                });
            }
        }
        // Determine whether this telegram will be forwarded
        let mut forward = echo_request.get().then_some(output);
        if let Some(output) = forward.as_deref_mut() {
            let _ = output.write(line);
        }
        let mut crc_state = CrcState::default();
        crc_state.update(line);
        Self {
            lines,
            forward,
            crc_state,
        }
    }

    fn next_line<'s>(&'s mut self) -> ReadResult<'s> {
        let line = self.lines.next();
        if let Some(output) = self.forward.as_deref_mut() {
            let _ = output.write(line);
        }
        if let [b'!', crc @ .., b'\r', b'\n'] = line {
            // End of telegram
            self.crc_state.update(b"!");
            ReadResult::End(if Some(self.crc_state.get()) == try_parse_crc(crc) {
                CrcResult::Match
            } else {
                CrcResult::Mismatch
            })
        } else {
            self.crc_state.update(line);
            ReadResult::Line(line)
        }
    }
}

pub struct P1Reader<'a, R, W, E> {
    input: Lines<R>,
    output: &'a mut W,
    echo_request: &'a E,
}

impl<'a, R: Read, W, E> P1Reader<'a, R, W, E> {
    pub fn new(input: R, output: &'a mut W, echo_request: &'a E) -> Self {
        Self {
            input: Lines::new(input),
            output,
            echo_request,
        }
    }
}

impl<R: Read, W: Write, E: Get<bool>> CosemSource for P1Reader<'_, R, W, E> {
    fn receive_message(
        &mut self,
        mut electricity: impl FnMut(Obis, ElectricityValue),
    ) -> Result<Option<DateTime>, ReceiveError> {
        let mut telegram =
            P1Telegram::from_next_begin(&mut self.input, self.output, self.echo_request);
        match telegram.next_line() {
            ReadResult::Line(b"\r\n") => {}
            ReadResult::Line(unexpected) => {
                warn!("Unexpected first P1 telegram line: {}", unsafe {
                    str::from_utf8_unchecked(unexpected.trim_ascii_end())
                });
            }
            ReadResult::End(_) => return Err(ReceiveError::MessageCorrupted),
        }
        let mut timestamp: Option<DateTime> = None;
        loop {
            match telegram.next_line() {
                ReadResult::Line(line) => {
                    let mut reader = line.iter().peekable();
                    if let Some(obis) = Obis::try_parse(&mut reader)
                        && let [b'(', value @ .., b')', b'\r', b'\n'] =
                            remainder_subslice(&mut reader, line)
                    {
                        if matches!(obis, Obis(1, _, _, _, _)) {
                            if let Some(e_value) = ElectricityValue::try_parse_p1(value) {
                                electricity(obis, e_value);
                            }
                        } else if obis == Obis(0, 0, 1, 0, 0) {
                            timestamp = DateTime::try_parse_p1(value);
                        }
                    }
                }
                ReadResult::End(CrcResult::Match) => break Ok(timestamp),
                ReadResult::End(CrcResult::Mismatch) => {
                    if let Some(datetime) = timestamp {
                        warn!("P1 telegram CRC mismatch at {:?}", datetime);
                    } else {
                        warn!("P1 telegram CRC mismatch");
                    }
                    break Err(ReceiveError::MessageCorrupted);
                }
            }
        }
    }
}
