#![deny(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::dbg_macro)]
#![cfg_attr(not(feature = "std"), no_std)]

//! # Hexponent
//!
//! Hexponent is a hexadecimal literal parser for Rust based on the C11
//! specification section [6.4.4.2](http://port70.net/~nsz/c/c11/n1570.html#6.4.4.2).
//!
//! ```rust
//! use hexponent::FloatLiteral;
//! let float_repr: FloatLiteral = "0x3.4".parse().unwrap();
//! let value = float_repr.convert::<f32>().inner();
//! assert_eq!(value, 3.25);
//! ```
//! Hexponent has a minimum supported rust version of 1.34.
//!
//! ## Features
//! - No dependencies
//! - Non-UTF-8 parser
//! - Precision warnings
//! - `no_std` support (MSRV 1.36.0)
//!
//! ## Differences from the specification
//! There are two places where hexponent differs from the C11 specificaiton.
//! - An exponent is not required. (`0x1.2` is allowed)
//! - `floating-suffix` is *not* parsed. (`0x1p4l` is not allowed)
//!
//! ## `no_std` support
//! `no_std` support can be enabled by disabling the default `std` feature for
//! hexponent in your `Cargo.toml`.
//! ```toml
//! hexponent = {version = "0.2", default-features = false}
//! ```
//! `no_std` support is only possible in rustc version 1.36.0 and higher.
//!
//! Disabling the `std` feature can currently only disables the
//! `std::error::Error` implementation for `ParseError`.

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use core::fmt;

mod parse_utils;
use parse_utils::*;

mod fpformat;
pub use fpformat::FPFormat;

#[derive(Debug)]
/// Indicates the preicsision of a conversion
pub enum ConversionResult<T> {
    /// The conversion was precise and the result represents the original exactly.
    Precise(T),

    // TODO: I should be able to calculate how imprecise the conversion is too,
    // which might be useful. This also might allow some subnormal numbers to be
    // returned as precise results.
    /// The conversion was imprecise and the result is as close to the original
    /// as possible.
    Imprecise(T),
}

impl<T> ConversionResult<T> {
    /// Convert the result to it's contained type.
    pub fn inner(self) -> T {
        match self {
            ConversionResult::Precise(f) => f,
            ConversionResult::Imprecise(f) => f,
        }
    }
}

/// Error type for parsing hexadecimal literals.
///
/// See the [`ParseErrorKind`](enum.ParseErrorKind.html) documentation for more
/// details about the kinds of errors and examples.
///
/// `ParseError` only implements `std::error::Error` when the `std` feature is
/// enabled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ParseError {
    /// Kind of error
    pub kind: ParseErrorKind,
    /// Approximate index of the error in the source data. This will always be
    /// an index to the source, except for when something is expected and
    /// nothing is found, in this case, `index` will be the length of the input.
    pub index: usize,
}

/// Kind of parsing error.
///
/// Used in [`ParseError`](struct.ParseError.html)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParseErrorKind {
    /// No prefix was found. Hexadecimal literals must start with a "0x" or "0X"
    /// prefix.
    ///
    /// Example: `0.F`
    MissingPrefix,
    /// No digits were found. Hexadecimals literals must have digits before or
    /// after the decimal point.
    ///
    /// Example: `0x.` `0x.p1`
    MissingDigits,
    /// Hexadecimal literals with a "p" or "P" to indicate an float must have
    /// an exponent.
    ///
    /// Example: `0xb.0p` `0x1p-`
    MissingExponent,
    /// The exponent of a hexidecimal literal must fit into a signed 32-bit
    /// integer.
    ///
    /// Example: `0x1p3000000000`
    ExponentOverflow,
    /// The end of the literal was expected, but more bytes were found.
    ///
    /// Example: `0x1.g`
    MissingEnd,
}

impl ParseErrorKind {
    fn at(self, index: usize) -> ParseError {
        ParseError { kind: self, index }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            ParseErrorKind::MissingPrefix => write!(f, "literal must have hex prefix"),
            ParseErrorKind::MissingDigits => write!(f, "literal must have digits"),
            ParseErrorKind::MissingExponent => write!(f, "exponent not present"),
            ParseErrorKind::ExponentOverflow => write!(f, "exponent too large to fit in integer"),
            ParseErrorKind::MissingEnd => {
                write!(f, "extra bytes were found at the end of float literal")
            }
        }
    }
}

#[cfg(feature = "std")]
/// Only available with the `std` feature.
impl std::error::Error for ParseError {}

/// Represents a floating point literal
///
/// This struct is a representation of the text, that can be used to convert to
/// both single- and double-precision floats.
///
/// `FloatLiteral` is not `Copy`-able because it contains a vector of the
/// digits from the source data.
#[derive(Debug, Clone)]
pub struct FloatLiteral {
    is_positive: bool,
    // These are the values of the digits, not the digits in ascii form.
    digits: Vec<u8>,
    decimal_offset: i32,
    exponent: i32,
}

/// Get the byte index of the start of `sub_slice` in `master_slice`
fn get_cursed_index(master_slice: &[u8], sub_slice: &[u8]) -> usize {
    (sub_slice.as_ptr() as usize).saturating_sub(master_slice.as_ptr() as usize)
}

impl FloatLiteral {
    /// Convert the `self` to an `f32` or `f64` and return the precision of the
    /// conversion.
    pub fn convert<F: FPFormat>(self) -> ConversionResult<F> {
        F::from_literal(self)
    }

    /// Parse a slice of bytes into a `FloatLiteral`.
    ///
    /// This is based on hexadecimal floating constants in the C11 specification,
    /// section [6.4.4.2](http://port70.net/~nsz/c/c11/n1570.html#6.4.4.2).
    pub fn from_bytes(data: &[u8]) -> Result<FloatLiteral, ParseError> {
        let original_data = data;

        let (is_positive, data) = match data.get(0) {
            Some(b'+') => (true, &data[1..]),
            Some(b'-') => (false, &data[1..]),
            _ => (true, data),
        };

        let data = match data.get(0..2) {
            Some(b"0X") | Some(b"0x") => &data[2..],
            _ => return Err(ParseErrorKind::MissingPrefix.at(0)),
        };

        let (ipart, data) = consume_hex_digits(data);

        let (fpart, data): (&[_], _) = if data.get(0) == Some(&b'.') {
            let (fpart, data) = consume_hex_digits(&data[1..]);
            (fpart, data)
        } else {
            (b"", data)
        };

        // Must have digits before or after the decimal point.
        if fpart.is_empty() && ipart.is_empty() {
            return Err(ParseErrorKind::MissingDigits.at(get_cursed_index(original_data, data)));
        }

        let (exponent, data) = match data.get(0) {
            Some(b'P') | Some(b'p') => {
                let data = &data[1..];

                let sign_offset = match data.get(0) {
                    Some(b'+') | Some(b'-') => 1,
                    _ => 0,
                };

                let exponent_digits_offset = data[sign_offset..]
                    .iter()
                    .position(|&b| match b {
                        b'0'..=b'9' => false,
                        _ => true,
                    })
                    .unwrap_or_else(|| data[sign_offset..].len());

                if exponent_digits_offset == 0 {
                    return Err(
                        ParseErrorKind::MissingExponent.at(get_cursed_index(original_data, data))
                    );
                }

                // The exponent should always contain valid utf-8 beacuse it
                // consumes a sign, and base-10 digits.
                // TODO: Maybe make this uft8 conversion unchecked. It should be
                // good, but I also don't want unsafe code.
                let exponent: i32 =
                    core::str::from_utf8(&data[..sign_offset + exponent_digits_offset])
                        .expect("exponent did not contain valid utf-8")
                        .parse()
                        .map_err(|_| {
                            ParseErrorKind::ExponentOverflow
                                .at(get_cursed_index(original_data, data))
                        })?;

                (exponent, &data[sign_offset + exponent_digits_offset..])
            }
            _ => (0, data),
        };

        if !data.is_empty() {
            return Err(ParseErrorKind::MissingEnd.at(get_cursed_index(original_data, data)));
        }

        let mut raw_digits = ipart.to_vec();
        raw_digits.extend_from_slice(fpart);

        let first_digit = raw_digits.iter().position(|&d| d != b'0');

        let (digits, decimal_offset) = if let Some(first_digit) = first_digit {
            // Unwrap is safe because there is at least one digit.
            let last_digit = raw_digits.iter().rposition(|&d| d != b'0').unwrap();
            let decimal_offset = (ipart.len() as i32) - (first_digit as i32);

            // Trim off the leading zeros
            raw_digits.truncate(last_digit + 1);
            // Trim off the trailing zeros
            raw_digits.drain(..first_digit);

            // Convert all the digits from ascii to their values.
            for item in raw_digits.iter_mut() {
                *item = hex_digit_to_int(*item).unwrap();
            }

            (raw_digits, decimal_offset)
        } else {
            (Vec::new(), 0)
        };

        Ok(FloatLiteral {
            is_positive,
            digits,
            decimal_offset,
            exponent,
        })
    }
}

impl core::str::FromStr for FloatLiteral {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<FloatLiteral, ParseError> {
        FloatLiteral::from_bytes(s.as_bytes())
    }
}

impl From<FloatLiteral> for f32 {
    fn from(literal: FloatLiteral) -> f32 {
        literal.convert().inner()
    }
}

impl From<FloatLiteral> for f64 {
    fn from(literal: FloatLiteral) -> f64 {
        literal.convert().inner()
    }
}

#[cfg(test)]
mod tests;
