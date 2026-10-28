//! DDC/CI message framing for the raw-I²C links (Linux i2c-dev, macOS IOAVService).
//!
//! Requests are the bytes after the 0x51 source address (the transport prepends or addresses it); replies are the bytes
//! read back, starting with the monitor's 0x6E source address. Checksums: request = `0x6E ^ 0x51 ^ payload`,
//! reply = `0x50 ^ reply bytes`. Some macOS tools leave 0x51 out of the request checksum; `include_source = false`
//! produces that legacy form.

use tvc_core::link::VcpReading;

/// 7-bit I²C address of the monitor's DDC/CI endpoint.
pub const MONITOR_ADDRESS: u8 = 0x37;
/// The host's source address, the first byte on the wire after the slave address.
pub const HOST_SOURCE: u8 = 0x51;
/// Bytes read back for a Get VCP reply.
pub const VCP_REPLY_LEN: usize = 11;
/// Bytes read back for one capabilities fragment (header 3 + up to 32 data + offset 2 + checksum 1).
pub const FRAGMENT_REPLY_LEN: usize = 3 + 32 + 3;

const DESTINATION_WRITE: u8 = 0x6E;
const DESTINATION_READ: u8 = 0x50;
const LENGTH_FLAG: u8 = 0x80;
const OP_GET_VCP: u8 = 0x01;
const OP_GET_VCP_REPLY: u8 = 0x02;
const OP_SET_VCP: u8 = 0x03;
const OP_CAPABILITIES: u8 = 0xF3;
const OP_CAPABILITIES_REPLY: u8 = 0xE3;

/// Get VCP request: `[0x82, 0x01, code, chk]`.
pub fn get_vcp(code: u8, include_source: bool) -> Vec<u8> {
    frame(&[OP_GET_VCP, code], include_source)
}

/// Set VCP request: `[0x84, 0x03, code, hi, lo, chk]`.
pub fn set_vcp(code: u8, value: u16, include_source: bool) -> Vec<u8> {
    let [hi, lo] = value.to_be_bytes();
    frame(&[OP_SET_VCP, code, hi, lo], include_source)
}

/// Capabilities request for the fragment at `offset`: `[0x83, 0xF3, hi, lo, chk]`.
pub fn capabilities_request(offset: u16, include_source: bool) -> Vec<u8> {
    let [hi, lo] = offset.to_be_bytes();
    frame(&[OP_CAPABILITIES, hi, lo], include_source)
}

/// Parses an 11-byte Get VCP reply `[6E, 88, 02, result, code, type, maxHi, maxLo, curHi, curLo, chk]`.
/// None on a bad checksum, a different opcode or code, or result ≠ 0 ("unsupported").
pub fn parse_vcp_reply(reply: &[u8], expected_code: u8) -> Option<VcpReading> {
    if reply.len() < VCP_REPLY_LEN
        || reply[1] != (LENGTH_FLAG | 8)
        || reply[2] != OP_GET_VCP_REPLY
        || !reply_checksum_ok(&reply[..VCP_REPLY_LEN])
        || reply[3] != 0
        || reply[4] != expected_code
    {
        return None;
    }
    Some(VcpReading {
        current: u16::from_be_bytes([reply[8], reply[9]]),
        max: u16::from_be_bytes([reply[6], reply[7]]),
    })
}

/// True for a well-formed Get VCP reply whose result code is 1: the monitor doesn't implement `code`.
pub fn is_unsupported_reply(reply: &[u8], code: u8) -> bool {
    reply.len() >= VCP_REPLY_LEN
        && reply[1] == (LENGTH_FLAG | 8)
        && reply[2] == OP_GET_VCP_REPLY
        && reply[3] == 0x01
        && reply[4] == code
        && reply_checksum_ok(&reply[..VCP_REPLY_LEN])
}

/// Parses one capabilities fragment `[6E, 80|len, E3, offHi, offLo, data…, chk]` and returns its data.
/// Empty data means the string is complete. None if malformed or for another offset.
pub fn parse_capabilities_fragment(reply: &[u8], expected_offset: u16) -> Option<&[u8]> {
    if reply.len() < 6 || reply[1] & LENGTH_FLAG == 0 {
        return None;
    }
    let length = usize::from(reply[1] & !LENGTH_FLAG); // opcode + 2 offset bytes + data
    if length < 3 || reply.len() < length + 3 || reply[2] != OP_CAPABILITIES_REPLY {
        return None;
    }
    if !reply_checksum_ok(&reply[..length + 3]) || u16::from_be_bytes([reply[3], reply[4]]) != expected_offset {
        return None;
    }
    Some(&reply[5..length + 2])
}

fn frame(body: &[u8], include_source: bool) -> Vec<u8> {
    let mut packet = Vec::with_capacity(body.len() + 2);
    packet.push(LENGTH_FLAG | body.len() as u8);
    packet.extend_from_slice(body);
    let seed = DESTINATION_WRITE ^ if include_source { HOST_SOURCE } else { 0 };
    packet.push(packet.iter().fold(seed, |c, b| c ^ b));
    packet
}

fn reply_checksum_ok(reply: &[u8]) -> bool {
    let (last, body) = reply.split_last().expect("non-empty reply");
    body.iter().fold(DESTINATION_READ, |c, b| c ^ b) == *last
}

#[cfg(test)]
mod tests {
    use super::*;

    const P32P20_CAPABILITIES: &str = concat!(
        "(prot(monitor)type(LCD)model(ThinkVision P32p-20)cmds(01 02 03 07 0C E3 F3)vcp(02 04(4C 42 31 2E 32) 05 08 10 12 ",
        "14(01 05 06 08 0B 0E 0F 12) 16 18 1A 52 60(11 0F 31) 86(02 05) AC AE B2 B6 ",
        "C6(02 01 C3 00 A0 CA 00 F8 00 EA 00 00 DC 00 46 00 00) C8 C9 CA(01 02) CC(02 03 04 05 06 09 0A 0D) D6(01 04 05) DF ",
        "E0(00 01 02) EA(00 01) EB(00 01) EC(00 01) F6(07) F7(02 04 07 08 0D) F8(02 04 07 08 0D) FA(10 12 EA 14))",
        "mswhql(1)asset_eep(40)mccs_ver(2.2))"
    );

    /// Appends the reply checksum (0x50 ^ every byte).
    fn reply(bytes: &[u8]) -> Vec<u8> {
        let mut out = bytes.to_vec();
        out.push(bytes.iter().fold(0x50, |c, b| c ^ b));
        out
    }

    #[test]
    fn get_request_matches_the_wire_bytes() {
        assert_eq!(get_vcp(0x10, true), [0x82, 0x01, 0x10, 0xAC]); // spec checksum (0x51 included)
        assert_eq!(get_vcp(0x10, false), [0x82, 0x01, 0x10, 0xFD]); // legacy macOS form
    }

    #[test]
    fn set_request_encodes_high_and_low_bytes() {
        assert_eq!(set_vcp(0x10, 50, true), [0x84, 0x03, 0x10, 0x00, 0x32, 0x9A]);
    }

    #[test]
    fn valid_reply_parses_current_and_max() {
        let r = reply(&[0x6E, 0x88, 0x02, 0x00, 0x10, 0x00, 0x00, 0x64, 0x00, 0x28]);
        assert_eq!(parse_vcp_reply(&r, 0x10), Some(VcpReading { current: 40, max: 100 }));
    }

    #[test]
    fn bad_checksum_wrong_code_or_unsupported_is_rejected() {
        let good = reply(&[0x6E, 0x88, 0x02, 0x00, 0x10, 0x00, 0x00, 0x64, 0x00, 0x28]);
        let mut bad_checksum = good.clone();
        *bad_checksum.last_mut().unwrap() ^= 0xFF;
        let unsupported = reply(&[0x6E, 0x88, 0x02, 0x01, 0x10, 0x00, 0x00, 0x64, 0x00, 0x28]);

        assert_eq!(parse_vcp_reply(&bad_checksum, 0x10), None);
        assert_eq!(parse_vcp_reply(&good, 0x12), None);
        assert_eq!(parse_vcp_reply(&unsupported, 0x10), None);
        assert_eq!(parse_vcp_reply(&good[..10], 0x10), None);

        assert!(is_unsupported_reply(&unsupported, 0x10));
        assert!(!is_unsupported_reply(&unsupported, 0x12));
        assert!(!is_unsupported_reply(&good, 0x10));
    }

    #[test]
    fn capabilities_fragments_reassemble_the_real_string() {
        let caps = P32P20_CAPABILITIES.as_bytes();
        let mut assembled = Vec::new();
        let mut offset: u16 = 0;
        loop {
            let request = capabilities_request(offset, true);
            assert_eq!(request[2], (offset >> 8) as u8);
            assert_eq!(request[3], offset as u8);

            let start = usize::from(offset).min(caps.len());
            let chunk = &caps[start..(start + 32).min(caps.len())];
            let mut body = vec![0x6E, 0x80 | (chunk.len() as u8 + 3), 0xE3, (offset >> 8) as u8, offset as u8];
            body.extend_from_slice(chunk);
            let r = reply(&body);
            let data = parse_capabilities_fragment(&r, offset).expect("valid fragment");
            if data.is_empty() {
                break;
            }
            assembled.extend_from_slice(data);
            offset += data.len() as u16;
        }
        assert_eq!(String::from_utf8(assembled).unwrap(), P32P20_CAPABILITIES);
    }

    #[test]
    fn fragment_for_another_offset_or_with_bad_checksum_is_rejected() {
        let r = reply(&[0x6E, 0x85, 0xE3, 0x00, 0x20, b'a', b'b']);
        assert_eq!(parse_capabilities_fragment(&r, 0x20), Some(&b"ab"[..]));
        assert_eq!(parse_capabilities_fragment(&r, 0x00), None);
        let mut bad = r.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert_eq!(parse_capabilities_fragment(&bad, 0x20), None);
    }
}
