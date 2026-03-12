//! Tanh LUT generator - DETERMINISTIC
//!
//! For Q8.8 format: input x is scaled by 256 (x_scaled = x * 256)
//! Output is also Q8.8 (tanh(x) * 256)
//!
//! Range: [-4.0, 4.0] with step 0.01 = 801 values
//! Output: Q8.8 clamped to [-256, 256]

/// Compute tanh and quantize to Q8.8
/// x: floating point input
/// Returns: tanh(x) * 256, rounded to nearest integer, clamped to [-256, 256]
fn tanh_q8_8(x: f64) -> i16 {
    let tanh_x = x.tanh();
    // Quantize to Q8.8: multiply by 256 and round to nearest
    let quantized = (tanh_x * 256.0).round();
    // Clamp to valid Q8.8 range for tanh output
    quantized.clamp(-256.0, 256.0) as i16
}

fn main() {
    let values: Vec<i16> = (0..801)
        .map(|i| {
            let x = -4.0 + (i as f64 * 0.01);
            tanh_q8_8(x)
        })
        .collect();

    println!("const TANH_LUT_V1: [i16; 801] = [");
    for (i, &v) in values.iter().enumerate() {
        if i % 8 == 0 {
            print!("    ");
        }
        print!("{:5}, ", v);
        if i % 8 == 7 || i == 800 {
            println!();
        }
    }
    println!("];");

    // Debug: print first, middle, last
    println!(
        "\n// Debug: tanh(-4.0)={}, tanh(0)={}, tanh(4.0)={}",
        values[0], values[400], values[800]
    );
}
