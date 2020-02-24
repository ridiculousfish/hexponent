#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::dbg_macro)]

//! # Hexponent
//!
//! Hexponent is a hexadecimal literal parser for Rust based on the C11
//! specification section [6.4.4.2](http://port70.net/~nsz/c/c11/n1570.html#6.4.4.2).
//! 
//! ```rust
//! use hexponent::Float;
//! let float_repr: Float = "0x3.4".parse().unwrap();
//! let value = float_repr.into_f32().unwrap();
//! assert_eq!(value, 3.25);
//! ```
//! 
//! ## Features
//! - No dependencies
//! - Faster non-UTF-8 parser
//! - Precision warnings
//! 
//! ## Differences from the specification
//! There are two places where hexponent differs from the C11 specificaiton.
//! - An exponent is not required. (`0x1.2` is allowed)
//! - `floating-suffix` is *not* parsed. (`0x1p4l` is not allowed)

use std::fmt;

mod parse_utils;
use parse_utils::*;

/// Indicates the preicsision of a conversion
pub enum ConversionResult<T> {
    /// The conversion was precise and the result represents the original exactly.
    Precise(T),

    // I should be able to calculate how imprecise the conversion is too, which
    // might be useful. This might allow some subnormal numbers to be returned
    // as precise results.
    /// The conversion was imprecise and the result is as close to the original
    /// as possible.
    Imprecise(T),
}

impl<T> ConversionResult<T> {
    /// Convert the result to it's contained type.
    pub fn unwrap(self) -> T {
        match self {
            ConversionResult::Precise(f) => f,
            ConversionResult::Imprecise(f) => f,
        }
    }
}

/// Error type for parsing hexadecimal literals.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParseError {
    /// No prefix was found. Hexadecimal literals must start with a "0x" or "0X"
    /// prefix.
    MissingPrefix,
    /// No digits were found. Hexadecimals literals must have digits before or
    /// after the decimal point.
    MissingDigits,
    /// Hexadecimal literals with a "p" or "P" to indicate an exponent must have
    /// an exponent.
    MissingExponent,
    /// The exponent of a hexidecimal literal must fit into a signed 32-bit
    /// integer.
    ExponentOverflow,
    /// Extra bytes were found at the end of the hexadecimal literal.
    ExtraData,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseError::MissingPrefix => write!(f, "literal must have hex prefix"),
            ParseError::MissingDigits => write!(f, "literal must have digits"),
            ParseError::MissingExponent => write!(f, "exponent not present"),
            ParseError::ExponentOverflow => write!(f, "exponent too large to fit in integer"),
            ParseError::ExtraData => {
                write!(f, "extra bytes were found at the end of float literal")
            }
        }
    }
}

impl From<std::num::ParseIntError> for ParseError {
    fn from(_error: std::num::ParseIntError) -> ParseError {
        ParseError::ExponentOverflow
    }
}

/// Represents a floating point literal
///
/// This struct is a representation of the text, that can be used to convert to
/// both single- and double-precision floats.
#[derive(Debug, Clone)]
pub struct Float {
    is_positive: bool,
    ipart: Vec<u8>,
    fpart: Vec<u8>,
    exponent: i32,
}

impl Float {
    /// Convert the `self` to an `f32` and return the precision of the
    /// conversion.
    pub fn into_f32(self) -> ConversionResult<f32> {
        // This code should work for arbitrary values of the following
        // constants
        const EXPONENT_BITS: u32 = 8;
        const MANTISSA_BITS: u32 = 23;

        // The spec always gives an exponent bias that follows this formula.
        const EXPONENT_BIAS: u32 = (1 << (EXPONENT_BITS - 1)) - 1;

        // 4 bits for each digit of the ipart
        let mut exponent_offset: i32 = (self.ipart.len() as i32) * 4;

        // All the digits together, it doesn't matter where the (hexa)decimal
        // point was because it was accounted for in the exponent_offset.
        let mut digits = self.ipart;
        digits.extend_from_slice(&self.fpart);

        // If there were all
        if digits.is_empty() {
            return ConversionResult::Precise(0.0);
        }

        // This code is a work of art.
        let mut mantissa_result: u32 = 0;
        for (index, digit) in digits.iter().enumerate() {
            if index as u32 * 4 > MANTISSA_BITS {
                // TODO: Warn for excessive precision.
                // This should should technically return an Imprecise, but not
                // yet.
                break;
            }
            let mut digit_value = hex_digit_to_int(*digit).unwrap() as u32;
            digit_value <<= 32 - (index + 1) * 4;
            mantissa_result |= digit_value;
        }
        let leading_zeros = mantissa_result.leading_zeros();
        exponent_offset -= leading_zeros as i32 + 1;
        mantissa_result <<= leading_zeros + 1;
        mantissa_result >>= 32 - MANTISSA_BITS;

        let final_exponent = exponent_offset + self.exponent;

        // Check for underflows
        if final_exponent < std::f32::MIN_EXP - 1 {
            // TODO: Implement subnormal numbers.
            if self.is_positive {
                return ConversionResult::Imprecise(0.0);
            } else {
                return ConversionResult::Imprecise(-0.0);
            };
        }

        // Check for overflows
        if final_exponent > std::f32::MAX_EXP {
            if self.is_positive {
                return ConversionResult::Imprecise(std::f32::INFINITY);
            } else {
                return ConversionResult::Imprecise(std::f32::NEG_INFINITY);
            };
        }

        let exponent_result: u32 =
            ((final_exponent + EXPONENT_BIAS as i32) as u32) << MANTISSA_BITS;

        let sign_result: u32 = (!self.is_positive as u32) << (MANTISSA_BITS + EXPONENT_BITS);

        ConversionResult::Precise(f32::from_bits(
            sign_result | exponent_result | mantissa_result,
        ))

        // // This might be a bit faster.
        // let mut final_result = !self.is_positive as u32;
        // final_result <<= EXPONENT_BITS;
        // final_result |= (final_exponent + EXPONENT_BIAS as i32) as u32;
        // final_result <<= MANTISSA_BITS;
        // final_result |= mantissa_result;
        // f32::from_bits(final_result)
    }

    /// Parse a slice of bytes into a `Float`.
    ///
    /// This is based on hexadecimal floating constants in the C11 specification,
    /// section [6.4.4.2](http://port70.net/~nsz/c/c11/n1570.html#6.4.4.2).
    pub fn from_bytes(data: &[u8]) -> Result<Float, ParseError> {
        let (is_positive, data) = match data.get(0) {
            Some(b'+') => (true, &data[1..]),
            Some(b'-') => (false, &data[1..]),
            _ => (true, data),
        };

        let data = match data.get(0..2) {
            Some(b"0X") | Some(b"0x") => &data[2..],
            _ => return Err(ParseError::MissingPrefix),
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
            return Err(ParseError::MissingDigits);
        }

        // Trim leading zeros.
        let ipart: &[u8] = if let Some(first_digit) = ipart.iter().position(|&d| d != b'0') {
            &ipart[first_digit..]
        } else {
            &[]
        };

        // Trim trailing zeros
        let fpart: &[u8] = if let Some(last_digit) = fpart.iter().rposition(|&d| d != b'0') {
            &fpart[..=last_digit]
        } else {
            &[]
        };

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
                    return Err(ParseError::MissingExponent);
                }

                // The exponent should always contain valid utf-8 beacuse it
                // consumes a sign, and base-10 digits.
                // TODO: Maybe make this uft8 conversion unchecked. It should be
                // good, but I also don't want unsafe code.
                let exponent: i32 =
                    std::str::from_utf8(&data[..sign_offset + exponent_digits_offset])
                        .expect("exponent did not contain valid utf-8")
                        .parse()?;

                (exponent, &data[sign_offset + exponent_digits_offset..])
            }
            _ => (0, data),
        };

        if !data.is_empty() {
            return Err(ParseError::ExtraData);
        }

        Ok(Float {
            is_positive,
            ipart: ipart.to_vec(),
            fpart: fpart.to_vec(),
            exponent,
        })
    }
}

impl std::str::FromStr for Float {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Float, ParseError> {
        Float::from_bytes(s.as_bytes())
    }
}

impl Into<f32> for Float {
    fn into(self) -> f32 {
        self.into_f32().unwrap()
    }
}

#[cfg(test)]
mod tests;
