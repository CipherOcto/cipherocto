use octo_determin::{Dfp, DfpClass, dfp_add, dfp_sub, dfp_mul, dfp_div, dfp_sqrt};
use std::env;

fn parse_signed_mantissa(s: &str) -> Option<(u128, bool)> {
    // Handle optional leading minus sign
    let (abs_str, is_negative) = if s.starts_with('-') {
        (&s[1..], true)
    } else {
        (s, false)
    };

    let mantissa: u128 = abs_str.parse().ok()?;
    Some((mantissa, is_negative))
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: dfp_cli <op> <mantissa_a> <exponent_a> [mantissa_b exponent_b]");
        eprintln!("Ops: add, sub, mul, div, sqrt");
        eprintln!("Note: Mantissa can be negative (e.g., -3 or 3)");
        std::process::exit(1);
    }

    let op = &args[1];

    // Parse first operand
    if args.len() < 4 {
        eprintln!("Error: Need at least mantissa_a and exponent_a");
        std::process::exit(1);
    }

    let (mantissa_a, sign_a) = match parse_signed_mantissa(&args[2]) {
        Some((m, s)) => (m, s),
        None => {
            eprintln!("Error: Invalid mantissa_a: {}", args[2]);
            std::process::exit(1);
        }
    };

    let exponent_a: i32 = match args[3].parse() {
        Ok(e) => e,
        Err(_) => {
            eprintln!("Error: Invalid exponent_a: {}", args[3]);
            std::process::exit(1);
        }
    };

    let a = Dfp {
        mantissa: mantissa_a,
        exponent: exponent_a,
        class: DfpClass::Normal,
        sign: sign_a,
    };

    // Parse second operand (optional)
    let b = if args.len() >= 6 {
        let (mantissa_b, sign_b) = match parse_signed_mantissa(&args[4]) {
            Some((m, s)) => (m, s),
            None => {
                eprintln!("Error: Invalid mantissa_b: {}", args[4]);
                std::process::exit(1);
            }
        };

        let exponent_b: i32 = match args[5].parse() {
            Ok(e) => e,
            Err(_) => {
                eprintln!("Error: Invalid exponent_b: {}", args[5]);
                std::process::exit(1);
            }
        };

        Some(Dfp {
            mantissa: mantissa_b,
            exponent: exponent_b,
            class: DfpClass::Normal,
            sign: sign_b,
        })
    } else {
        None
    };

    let result = match op.as_str() {
        "add" => {
            let b = b.expect("add requires two operands");
            dfp_add(a, b)
        }
        "sub" => {
            let b = b.expect("sub requires two operands");
            dfp_sub(a, b)
        }
        "mul" => {
            let b = b.expect("mul requires two operands");
            dfp_mul(a, b)
        }
        "div" => {
            let b = b.expect("div requires two operands");
            dfp_div(a, b)
        }
        "sqrt" => {
            dfp_sqrt(a)
        }
        _ => {
            eprintln!("Error: Unknown operation: {}", op);
            std::process::exit(1);
        }
    };

    // Output in format: <sign> <mantissa> <exponent>
    // sign is 0 for positive, 1 for negative
    let sign_bit = if result.sign { 1 } else { 0 };
    println!("{} {} {}", sign_bit, result.mantissa, result.exponent);
}
