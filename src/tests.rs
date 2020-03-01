use crate::{FloatLiteral, ParseError};
use std::ffi;

// This macros serves two functions:
// 1. It avoids the float_cmp clippy lint
// 2. It is able to tell the difference between floats that are equal, but
// are not the same. (ex: zero and negative zero)
macro_rules! assert_eq_float {
    ($left: expr, $right: expr) => {
        let left_val: f32 = $left;
        let right_val: f32 = $right;
        if left_val.to_bits() != right_val.to_bits() {
            panic!(
                r#"float assertion failed: `(left == right)`
left: `{:?}` (`{:08x}`)
right: `{:?}` (`{:08x}`)"#,
                left_val,
                left_val.to_bits(),
                right_val,
                right_val.to_bits()
            );
        }
    };
}

macro_rules! assert_eq_double {
    ($left: expr, $right: expr) => {
        let left_val: f64 = $left;
        let right_val: f64 = $right;
        if left_val.to_bits() != right_val.to_bits() {
            panic!(
                r#"float assertion failed: `(left == right)`
left: `{:?}` (`{:016x}`)
right: `{:?}` (`{:016x}`)"#,
                left_val,
                left_val.to_bits(),
                right_val,
                right_val.to_bits()
            );
        }
    };
}

fn test_float(s: &str, result: f32) {
    let float_repr = s.parse::<FloatLiteral>().unwrap();
    let float_result: f32 = float_repr.convert().inner();
    assert_eq_float!(float_result, result);

    let libc_result = string_to_f32(s.as_ref()).unwrap();
    assert_eq_float!(float_result, libc_result);
}

fn test_double(s: &str, result: f64) {
    let float_repr = s.parse::<FloatLiteral>().unwrap();
    let double_result: f64 = float_repr.convert().inner();
    assert_eq_double!(double_result, result);

    let libc_result = string_to_f64(s.as_ref()).unwrap();
    assert_eq_double!(double_result, libc_result);
}

fn test_both(s: &str, float_result: f32){
    let double_result = float_result as f64;
    test_float(s, float_result);
    test_double(s, double_result);
}

fn test_parse_error(s: &str, error: ParseError) {
    assert_eq!(s.parse::<FloatLiteral>().unwrap_err(), error);
}

#[test]
fn test_zero() {
    test_both("0x0", 0.0);
    test_both("0x0.", 0.0);
    test_both("0x.0", 0.0);
    test_both("0x0.0", 0.0);
    test_both("0x0000.0000", 0.0);
}

#[test]
fn test_integers() {
    test_both("0x11", 17.0);
    test_both("0x21", 33.0);
    test_both("0x22", 34.0);

    test_both("0xDEAD", 57005.0);
    test_both("0xBEEF", 48879.0);
}

#[test]
fn test_fractions() {
    test_both("0x0.2", 0.125);
    test_both("0x0.4", 0.25);
    test_both("0x0.8", 0.5);
    test_both("0x0.c", 0.75);
    test_both("0x0.e", 0.875);
}

#[test]
fn test_exponents() {
    test_both("0x0.01", 0.003_906_25);
    test_both("0x0.1", 0.0625);
    test_both("0x1", 1.0);
    test_both("0x10", 16.0);
    test_both("0x100", 256.0);

    test_both("0x1p-8", 0.003_906_25);
    test_both("0x1p-4", 0.0625);
    test_both("0x1p0", 1.0);
    test_both("0x1p4", 16.0);
    test_both("0x1p8", 256.0);

    test_both("0x0.01p8", 1.0);
    test_both("0x0.1p4", 1.0);
    test_both("0x1p0", 1.0);
    test_both("0x10p-4", 1.0);
    test_both("0x100p-8", 1.0);
}

#[test]
fn test_overflow_underflow() {
    test_float("0x1p1000", std::f32::INFINITY);
    test_float("-0x1p1000", std::f32::NEG_INFINITY);
    test_float("0x1p-1000", 0.0);
    test_float("-0x1p-1000", -0.0);
}

#[test]
#[ignore]
fn test_subnormal() {
    // I haven't implemented subnormal numbers yet.
    test_float("0x1p-128", 0.0);
    test_float("-0x1p-128", -0.0);
}

#[test]
fn rcc_tests() {
    test_both("0x.ep0", 0.875);
    test_both("0x.ep-0", 0.875);
    test_both("0xe.p-4", 0.875);
    test_both("0xep-4", 0.875);

    // Hexf crashes on this one.
    "0x.000000000000000000102".parse::<FloatLiteral>().unwrap();
}

#[test]
fn test_incomplete() {
    test_parse_error("", ParseError::MissingPrefix);
    test_parse_error("-", ParseError::MissingPrefix);
    test_parse_error("+", ParseError::MissingPrefix);
    test_parse_error("-3.2", ParseError::MissingPrefix);
    test_parse_error("0x", ParseError::MissingDigits);
    test_parse_error("-0x", ParseError::MissingDigits);
    test_parse_error("+0x", ParseError::MissingDigits);
    test_parse_error("0x.", ParseError::MissingDigits);
    test_parse_error("0xp", ParseError::MissingDigits);
    test_parse_error("0x.p1", ParseError::MissingDigits);
    test_parse_error("0x1p", ParseError::MissingExponent);
    test_parse_error("0x1p+", ParseError::MissingExponent);
    test_parse_error("0x1p-", ParseError::MissingExponent);
    test_parse_error("0x1p10000000000", ParseError::ExponentOverflow);
    test_parse_error("0x1p-10000000000", ParseError::ExponentOverflow);
    test_parse_error("0xbaddata", ParseError::ExtraData);
}

#[test]
fn test_fuzzer_finds() {
    // Found by Byter on 2020-02-24
    "0X.0000002".parse::<FloatLiteral>().unwrap();

    // Found by Byter on 2020-02-29
    let literal = "0x3p127".parse::<FloatLiteral>().unwrap();
    println!("{:?}", literal.convert::<f32>());

}

#[test]
fn test_zero_trimming() {
    test_both("0x0.0000000001p+40", 1.0);
    test_both("0x10000000000p-40", 1.0);

    // Right now these can only be tested to not crash because my rounding is
    // incorrect.
    "0x10000000000".parse::<FloatLiteral>().unwrap();
    "0x.0000000001".parse::<FloatLiteral>().unwrap();
}

#[test]
fn test_double_precision() {
    // test that float rounds and double doesn't
    test_float("0x1000000001", 68_719_480_000.0);
    test_double("0x1000000001", 68_719_476_737.0);
}

// I had both of these functions checked over by jynelson

#[allow(unsafe_code)]
fn f32_to_string(f: f32) -> Result<Vec<u8>, ()> {
    let mut dest = [0u8; 32];
    let format = ffi::CString::new("%a").unwrap();
    let number = f as libc::c_double;
    let check =
        unsafe { libc::snprintf(dest.as_mut_ptr() as *mut i8, 32, format.as_ptr(), number) };
    if check >= 0 && check < 32 {
        Ok(dest[..check as usize].to_vec())
    } else {
        Err(())
    }
}

#[allow(unsafe_code)]
fn f64_to_string(f: f64) -> Result<Vec<u8>, ()> {
    let mut dest = [0u8; 32];
    let format = ffi::CString::new("%a").unwrap();
    let number = f as libc::c_double;
    let check =
        unsafe { libc::snprintf(dest.as_mut_ptr() as *mut i8, 32, format.as_ptr(), number) };
    if check >= 0 && check < 32 {
        Ok(dest[..check as usize].to_vec())
    } else {
        Err(())
    }
}

#[allow(unsafe_code)]
fn string_to_f32(string: &[u8]) -> Result<f32, ()> {
    let source = ffi::CString::new(string).unwrap();
    let format = ffi::CString::new("%a").unwrap();
    let mut dest: f32 = 0.0;
    let check = unsafe { libc::sscanf(source.as_ptr(), format.as_ptr(), &mut dest as *mut _) };
    if check == 1 {
        Ok(dest)
    } else {
        Err(())
    }
}

#[allow(unsafe_code)]
fn string_to_f64(string: &[u8]) -> Result<f64, ()> {
    let source = ffi::CString::new(string).unwrap();
    let format = ffi::CString::new("%la").unwrap();
    let mut dest: f64 = 0.0;
    let check = unsafe { libc::sscanf(source.as_ptr(), format.as_ptr(), &mut dest as *mut _) };
    if check == 1 {
        Ok(dest)
    } else {
        Err(())
    }
}