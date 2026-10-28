//! DDC/CI over the private IOAVService I²C calls (Apple Silicon). Timing and framing follow MonitorControl and m1ddc
//! (both MIT): every request is written twice, each write preceded by 10 ms; a read waits, reads the reply and retries
//! up to 5 times, 20 ms apart. The engine reconnects after wake or a display change: the service entry can change.

use std::thread::sleep;
use std::time::Duration;

use tvc_core::link::{LinkError, LinkFailure, MonitorLink, MonitorMatch, VcpReading};

use super::ffi::{self, Cf, IoAv};
use super::locator::locate;
use super::{
    fallback_checksum_mode, is_unsupported_port, reply_outcome, transport_order, ExchangeOutcome, Transport, ATTEMPTS,
    CAPABILITIES_FRAGMENT_LEN, CAPABILITIES_MAX_BYTES, RETRY_DELAY_MS, WRITE_CYCLES, WRITE_DELAY_MS,
};
use crate::{ascii, edid, not_connected, packet};

const VERIFY_CODE: u8 = 0x10; // brightness: every MCCS monitor has it

/// The last failing IOAVService call and its IOReturn.
type CallError = (&'static str, i32);

/// The IOAVService link: one owned service while connected.
pub(crate) struct IoAvLink {
    target: MonitorMatch,
    service: Option<Service>,
    transport: Transport,
    /// The checksum form that produced a valid reply (true = 0x51 included, per spec); None until one did.
    include_source: Option<bool>,
}

struct Service {
    cf: Cf,
    io_av: IoAv,
}

// SAFETY: an IOAVServiceRef is a CF object; CF retain/release is thread-safe and the engine uses the link from one
// thread at a time.
unsafe impl Send for Service {}

impl IoAvLink {
    pub(crate) fn new(target: MonitorMatch) -> Self {
        IoAvLink { target, service: None, transport: Transport::STANDARD, include_source: None }
    }

    fn service(&self) -> Result<&Service, LinkError> {
        self.service.as_ref().ok_or_else(not_connected)
    }

    /// Sends a request and reads the reply until `accept` takes it, trying the checksum forms per
    /// `fallback_checksum_mode` and remembering the one that worked. Once the form is known, a reply that `unsupported`
    /// recognises ends the exchange with `LinkError::Unsupported`.
    fn exchange(
        &mut self,
        what: &str,
        request: impl Fn(bool) -> Vec<u8>,
        reply_len: usize,
        accept: impl Fn(&[u8]) -> bool,
        unsupported: impl Fn(&[u8]) -> bool,
    ) -> Result<Vec<u8>, LinkError> {
        let mut mode = self.include_source.unwrap_or(true);
        loop {
            let (outcome, reply, error) = self.attempts(&request(mode), reply_len, &accept, &unsupported)?;
            if outcome == ExchangeOutcome::Ok {
                self.include_source = Some(mode);
                return Ok(reply);
            }
            if let Some(next) = fallback_checksum_mode(self.include_source, mode, outcome) {
                mode = next;
                continue;
            }
            return Err(match outcome {
                ExchangeOutcome::Unsupported => LinkError::Unsupported(format!("{what} is not supported by the monitor")),
                ExchangeOutcome::IoError => {
                    LinkError::Transient(format!("{} {what} failed (IOReturn 0x{:08X})", error.0, error.1))
                }
                _ => LinkError::Transient(format!(
                    "No valid reply to {what} (chip 0x{:02X})",
                    self.transport.chip_address
                )),
            });
        }
    }

    /// One request over all attempts. The error is the last failing call and its IOReturn.
    fn attempts(
        &self,
        payload: &[u8],
        reply_len: usize,
        accept: &impl Fn(&[u8]) -> bool,
        unsupported: &impl Fn(&[u8]) -> bool,
    ) -> Result<(ExchangeOutcome, Vec<u8>, CallError), LinkError> {
        let service = self.service()?;
        let mut saw_reply = false;
        let mut error = ("", 0);
        for attempt in 0..ATTEMPTS {
            if attempt > 0 {
                sleep(Duration::from_millis(RETRY_DELAY_MS));
            }
            let ret = self.write_twice(service, payload);
            if ret != 0 {
                error = ("IOAVServiceWriteI2C", ret);
                continue;
            }
            sleep(Duration::from_millis(self.transport.read_wait_ms));
            let mut reply = vec![0u8; reply_len];
            // SAFETY: a live service and a buffer of exactly `reply_len` bytes.
            let ret = unsafe {
                (service.io_av.read_i2c)(
                    service.cf.0,
                    self.transport.chip_address,
                    self.transport.read_offset,
                    reply.as_mut_ptr().cast(),
                    reply_len as u32,
                )
            };
            if ret != 0 {
                error = ("IOAVServiceReadI2C", ret);
                continue;
            }
            let accepted = accept(&reply);
            match reply_outcome(self.include_source.is_some(), accepted, !accepted && unsupported(&reply)) {
                Some(ExchangeOutcome::Ok) => return Ok((ExchangeOutcome::Ok, reply, error)),
                Some(outcome) => return Ok((outcome, Vec::new(), error)),
                None => saw_reply = true,
            }
        }
        let outcome = if saw_reply { ExchangeOutcome::InvalidReply } else { ExchangeOutcome::IoError };
        Ok((outcome, Vec::new(), error))
    }

    /// Writes the payload to data address 0x51 twice, each preceded by 10 ms. Returns the first IOReturn error, or 0.
    fn write_twice(&self, service: &Service, payload: &[u8]) -> i32 {
        let mut buffer = payload.to_vec();
        for _ in 0..WRITE_CYCLES {
            sleep(Duration::from_millis(WRITE_DELAY_MS));
            // SAFETY: a live service and a buffer of exactly `buffer.len()` bytes.
            let ret = unsafe {
                (service.io_av.write_i2c)(
                    service.cf.0,
                    self.transport.chip_address,
                    packet::HOST_SOURCE.into(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as u32,
                )
            };
            if ret != 0 {
                return ret;
            }
        }
        0
    }
}

impl MonitorLink for IoAvLink {
    fn connect(&mut self) -> Result<(), LinkFailure> {
        self.service = None;
        let Some((pnp, product)) = self.target.pnp_and_product() else { return Err(LinkFailure::NotFound) };
        let Some(vendor) = edid::pack_pnp(pnp) else { return Err(LinkFailure::NotFound) };
        let Some(io_av) = ffi::io_av() else { return Err(LinkFailure::NotFound) };
        let Ok(lookup) = locate(io_av, vendor, product, pnp) else { return Err(LinkFailure::NotFound) };
        self.service = Some(Service { cf: lookup.service, io_av });

        // Connect-verify: the first read of 0x10 also settles the checksum form. Try the MCDP29xx (HDMI) variant when
        // the standard path stays silent, or first when the service's provider says it is that chip.
        let order = transport_order(lookup.mcdp29xx);
        for transport in order {
            self.transport = transport;
            if self.read(VERIFY_CODE).is_ok() {
                return Ok(());
            }
        }

        if is_unsupported_port(lookup.port) {
            self.service = None;
            return Err(LinkFailure::UnsupportedPort);
        }

        // Present but silent (DDC/CI off in the OSD, a dock, asleep): stay connected so commands report not responding.
        self.transport = order[0];
        Ok(())
    }

    fn disconnect(&mut self) {
        self.service = None;
    }

    fn capabilities(&mut self) -> Result<String, LinkError> {
        self.service()?;
        let mut text = Vec::new();
        let mut offset: u16 = 0;
        while text.len() < CAPABILITIES_MAX_BYTES {
            let at = offset;
            let reply = self.exchange(
                &format!("capabilities at {at}"),
                |mode| packet::capabilities_request(at, mode),
                CAPABILITIES_FRAGMENT_LEN,
                |bytes| packet::parse_capabilities_fragment(bytes, at).is_some(),
                |_| false, // capability replies have no "unsupported" form
            )?;
            let data = packet::parse_capabilities_fragment(&reply, at).unwrap_or_default();
            if data.is_empty() {
                break;
            }
            text.extend_from_slice(data);
            offset = offset.wrapping_add(data.len() as u16);
        }
        let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
        Ok(ascii(&text[..end]))
    }

    fn read(&mut self, code: u8) -> Result<VcpReading, LinkError> {
        self.service()?;
        let reply = self.exchange(
            &format!("VCP 0x{code:02X}"),
            |mode| packet::get_vcp(code, mode),
            self.transport.vcp_reply_len,
            |bytes| packet::parse_vcp_reply(bytes, code).is_some(),
            |bytes| packet::is_unsupported_reply(bytes, code),
        )?;
        packet::parse_vcp_reply(&reply, code).ok_or_else(|| LinkError::Transient("invalid reply".into()))
    }

    fn write(&mut self, code: u8, value: u16) -> Result<(), LinkError> {
        let service = self.service()?;
        let ret = self.write_twice(service, &packet::set_vcp(code, value, self.include_source.unwrap_or(true)));
        if ret != 0 {
            return Err(LinkError::Transient(format!("IOAVServiceWriteI2C VCP 0x{code:02X} failed (IOReturn 0x{ret:08X})")));
        }
        Ok(())
    }
}
