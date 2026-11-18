use core::hint::cold_path;

use embedded_io::Read;
use log::warn;

pub struct Lines<R, const CAPACITY: usize = 2048> {
    source: R,
    buf: [u8; CAPACITY],
    size: usize,
    next: usize,
}

impl<R: Read, const CAPACITY: usize> Lines<R, CAPACITY> {
    pub fn new(source: R) -> Self {
        Self {
            source,
            buf: [0; CAPACITY],
            size: 0,
            next: 0,
        }
    }

    pub fn next(&mut self) -> &[u8] {
        // Move the beginning of the next line to the front of the buffer
        self.buf.copy_within(self.next..self.size, 0);
        self.size -= self.next;
        self.next = 0;

        loop {
            if let Some(position) = self.buf[self.next..self.size]
                .iter()
                .position(|&c| c == b'\n')
            {
                self.next += position + 1;
                break;
            }
            match self.source.read(&mut self.buf[self.size..]) {
                Ok(advance) => {
                    self.next = self.size;
                    self.size += advance;
                    if self.next == self.buf.len() {
                        cold_path();
                        break;
                    }
                }
                Err(error) => {
                    warn!("Failure reading line: {:?}", error); // Otherwise ignored
                }
            }
        }
        &self.buf[..self.next]
    }
}
