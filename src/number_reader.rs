use core::iter::Peekable;
use core::slice::Iter;

pub type PeekableBytes<'a> = Peekable<Iter<'a, u8>>;

// Like Iter::as_slice
pub fn remainder_subslice<'a>(iter: &mut PeekableBytes, origin: &'a [u8]) -> &'a [u8] {
    &origin[iter
        .peek()
        .and_then(|element| origin.element_offset(element))
        .unwrap_or(origin.len())..]
}

struct Digits<'a, 'b>(&'a mut PeekableBytes<'b>);

impl Iterator for Digits<'_, '_> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next_if_map(|b| match b {
            b'0'..=b'9' => Ok(b - b'0'),
            _ => Err(b),
        })
    }
}

fn append_digit(value: u32, digit: u8) -> Option<u32> {
    value.checked_mul(10)?.checked_add(digit.into())
}

// Returns `None` on empty or overflowing input
pub fn next_number(source: &mut PeekableBytes) -> Option<u32> {
    let mut digits = Digits(source);
    let first = digits.next()?; // At least one digit
    digits.try_fold(first.into(), append_digit).or_else(|| {
        // Overflow: consume remaining digits
        digits.for_each(drop);
        None
    })
}

// 10^decimals times the input
pub fn next_number_decimals(source: &mut PeekableBytes, mut decimals: u8) -> Option<u32> {
    let mut maybe_number = next_number(source);
    if source.next_if_eq(&&b'.').is_some() {
        // Read all digits from the input
        for digit in Digits(source) {
            if decimals > 0 {
                maybe_number = maybe_number.and_then(|number| append_digit(number, digit));
                decimals -= 1;
            }
        }
    }
    maybe_number.and_then(|number| 10u32.checked_pow(decimals.into())?.checked_mul(number))
}
