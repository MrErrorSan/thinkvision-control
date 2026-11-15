//! Read-only smoke test against an attached monitor: list the displays, connect to the first one with that EDID id,
//! print capabilities, read brightness, contrast, input. Never writes. `cargo run -p tvc-ddc --example probe [EDID_ID]`

fn main() {
    let edid_id = std::env::args().nth(1).unwrap_or_else(|| "LEN62A2".into());
    let mut discovery = tvc_ddc::discovery();
    let displays = match discovery.enumerate() {
        Ok(displays) => displays,
        Err(e) => {
            println!("enumerate: {e:?}");
            return;
        }
    };
    for d in &displays {
        println!("display: {:?} {:?} {:?} {:?} target {}", d.name, d.identity, d.kind, d.backend, d.target);
    }
    let Some(display) = displays.iter().find(|d| d.identity.edid_id.eq_ignore_ascii_case(&edid_id)) else {
        println!("no display with EDID id {edid_id}");
        return;
    };
    let mut link = match discovery.open(&display.target) {
        Ok(link) => link,
        Err(failure) => {
            println!("open: {failure:?}");
            return;
        }
    };
    let t = std::time::Instant::now();
    let ms = |t: &std::time::Instant| t.elapsed().as_millis();

    if let Err(failure) = link.connect() {
        println!("connect: {failure:?}");
        return;
    }
    println!("connect: ok ({} ms)", ms(&t));
    let t = std::time::Instant::now();
    match link.capabilities() {
        Ok(caps) => println!("capabilities ({} chars, {} ms): {caps}", caps.len(), ms(&t)),
        Err(e) => println!("capabilities: {e:?}"),
    }
    for code in [0x10u8, 0x12, 0x60] {
        let t = std::time::Instant::now();
        match link.read(code) {
            Ok(r) => println!("0x{code:02X}: current {} max {} ({} ms)", r.current, r.max, ms(&t)),
            Err(e) => println!("0x{code:02X}: {e:?}"),
        }
    }
    link.disconnect();
}
