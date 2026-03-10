//! Tanh LUT generator - DETERMINISTIC using pure integer arithmetic

fn tanh_q8_8(x_scaled: i32) -> i16 {
    let abs_x = x_scaled.abs();
    if abs_x >= 1024 { return if x_scaled > 0 { 256 } else { -256 }; }

    let z = x_scaled as i64;
    let z2 = (z * z) >> 8;
    let z3 = (z2 * z) >> 8;
    let z5 = (z2 * z2 * z) >> 16;
    let z7 = (z2 * z2 * z2 * z) >> 24;

    let mut result = z;
    result += (z3 * -(256 / 3)) >> 8;
    result += (z5 * ((2 * 256) / 15)) >> 8;
    result += (z7 * -((17 * 256) / 315)) >> 8;

    // Preserve sign correctly
    let result_shifted = result >> 8;
    if x_scaled >= 0 { result_shifted as i16 } else { -result_shifted as i16 }
}

fn main() {
    let values: Vec<i16> = (0..801)
        .map(|i| {
            let x = -4.0 + (i as f64 * 0.01);
            let x_scaled = (x * 256.0) as i32;
            tanh_q8_8(x_scaled)
        })
        .collect();

    println!("const TANH_LUT_V1: [i16; 801] = [");
    for (i, &v) in values.iter().enumerate() {
        if i % 8 == 0 { print!("    "); }
        print!("{:5}, ", v);
        if i % 8 == 7 || i == 800 { println!(); }
    }
    println!("];");

    // Debug: print first, middle, last
    println!("\n// Debug: tanh(-4.0)={}, tanh(0)={}, tanh(4.0)={}",
             values[0], values[400], values[800]);
}
