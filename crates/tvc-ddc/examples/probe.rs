//! Read-only smoke test against the attached monitor: connect, print capabilities, read brightness, contrast, input.
//! Never writes. `cargo run -p tvc-ddc --example probe [EDID_ID] [DESCRIPTION_HINT]`

use tvc_core::link::MonitorMatch;

fn main() {
    let mut args = std::env::args().skip(1);
    let edid_id = args.next().unwrap_or_else(|| "LEN62A2".into());
    let description_hint = Some(args.next().unwrap_or_else(|| "P32p-20".into()));
    let mut link = tvc_ddc::open(MonitorMatch { edid_id, description_hint });

    if let Err(failure) = link.connect() {
        println!("connect: {failure:?}");
        return;
    }
    println!("connect: ok");
    match link.capabilities() {
        Ok(caps) => println!("capabilities ({} chars): {caps}", caps.len()),
        Err(e) => println!("capabilities: {e:?}"),
    }
    for code in [0x10u8, 0x12, 0x60] {
        match link.read(code) {
            Ok(r) => println!("0x{code:02X}: current {} max {}", r.current, r.max),
            Err(e) => println!("0x{code:02X}: {e:?}"),
        }
    }
    link.disconnect();
}
