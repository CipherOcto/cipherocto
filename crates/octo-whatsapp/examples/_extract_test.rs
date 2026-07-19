use octo_whatsapp::events::{parse_many, EventEnvelope};

fn main() {
    let s = std::env::args().nth(1).unwrap();
    let events = parse_many(
        EventEnvelope {
            raw: s,
            ts_unix_ms: 0,
            ts_mono_ns: 0,
        },
        None,
    );
    println!("got {} events", events.len());
}
