#![allow(dead_code)]
//! Sel4DirectTransport: synchronous `seL4_Call` transport with optional single-cap transfer
//! and notification minting for async completion signals.
//!
//! This file is a scaffold: production use requires implementing the raw
//! retype/mint operations (untyped->notification, cnode allocation) using
//! libsel4 or higher-level wrappers. The dispatch logic demonstrates where
//! Cap'n Proto messages are parsed and how a notification cap is returned.

use std::sync::{Arc, Mutex};
use capnp::message::{Builder, ReaderOptions};
use capnp::serialize;
use sel4::sys::*;
use sel4::cap::{Cap, Endpoint, Notification};
use crate::cap_management::{SessionCaps, CapBundle, CapError};
use crate::schemas::bridge_capnp::{capability_bridge};

pub struct Sel4DirectTransport {
    pub local_endpoint: Cap<Endpoint>,
    pub session_manager: Arc<Mutex<std::collections::HashMap<u64, SessionCaps>>>,
}

impl Sel4DirectTransport {
    /// Client: synchronous call with optional single-cap transfer (extraCap[0]).
    /// request_builder: Cap'n Proto message builder.
    /// transfer_cap: optional Notification or bundle cap to attach as extraCap[0].
    pub fn call_sync(
        &self,
        target: &Cap<Endpoint>,
        request_builder: Builder<capnp::message::HeapAllocator>,
        transfer_cap: Option<Cap<Notification>>,
    ) -> Result<Vec<u8>, CapError> {
        // Serialize message to word buffer
        let mut words = serialize::write_message_to_words(&request_builder);
        let msg_words = words.len();
        if msg_words > seL4_MsgMaxLength as usize {
            return Err(CapError::MessageTooLarge);
        }

        // Load message into message registers
        for (i, w) in words.iter().enumerate() {
            unsafe { seL4_SetMR(i as i32, *w) };
        }

        let info = seL4_MessageInfo::new(0, 0, if transfer_cap.is_some() {1} else {0}, msg_words as u64);

        if let Some(cap) = transfer_cap {
            unsafe { seL4_SetCap(0, cap.cptr) };
        }

        // Perform synchronous call
        let reply_info = unsafe { seL4_Call(target.cptr, info) };

        // Extract reply words
        let reply_words = reply_info.get_length() as usize;
        let mut reply_vec = vec![0u64; reply_words];
        for i in 0..reply_words {
            reply_vec[i] = unsafe { seL4_GetMR(i as i32) };
        }
        Ok(serialize::words_to_bytes(&reply_vec))
    }

    /// Server: receive, dispatch, and reply once. Returns after replying.
    pub fn serve_once(&self) -> Result<(), CapError> {
        let mut badge: u64 = 0;
        // Blocking receive: kernel returns message info; badge gets set
        let _info = unsafe { seL4_Recv(self.local_endpoint.cptr, &mut badge) };

        // NOTE: production code should fetch message length from message info; this is simplified.
        // Read a fixed number of message registers (fast-path)
        let max_words = seL4_FastMessageRegisters as usize;
        let mut words = Vec::with_capacity(max_words);
        for i in 0..max_words {
            let w = unsafe { seL4_GetMR(i as i32) };
            words.push(w);
        }

        let request_bytes = serialize::words_to_bytes(&words);

        // Detect extra cap presence via message info if available; simplified here
        // In a production binding you'd use the message info to know count and retrieve with seL4_GetCap
        let has_extra_cap = false;
        let received_cap = if has_extra_cap { Some(unsafe { Cap::new(seL4_GetCap(0)) }) } else { None };

        // Dispatch and optionally get a cap to return in reply (notification or bundle)
        let (reply_bytes, outgoing_cap) = self.dispatch(request_bytes, received_cap, badge)?;

        // Write reply into MRs
        let reply_words = serialize::bytes_to_words(&reply_bytes);
        for (i, w) in reply_words.iter().enumerate() {
            unsafe { seL4_SetMR(i as i32, *w) };
        }

        let reply_info = seL4_MessageInfo::new(0, 0, if outgoing_cap.is_some() {1} else {0}, reply_words.len() as u64);

        if let Some(cap) = outgoing_cap {
            // Attach as extraCap[0]
            unsafe { seL4_SetCap(0, cap.cptr) };
        }

        // Use Reply to return to the caller
        unsafe { seL4_Reply(reply_info) };
        Ok(())
    }

    /// Dispatch a Cap'n Proto request and optionally return a Notification cap to include in the reply.
    fn dispatch(
        &self,
        request_bytes: Vec<u8>,
        _received_cap: Option<Cap<Notification>>,
        badge: u64,
    ) -> Result<(Vec<u8>, Option<Cap<Notification>>), CapError> {
        let reader = serialize::read_message_from_bytes(&request_bytes, ReaderOptions::new()).map_err(|_| CapError::TransferFailed)?;
        let message = reader.get_root::<capability_bridge::Reader>().map_err(|_| CapError::TransferFailed)?;

        match message.which().map_err(|_| CapError::TransferFailed)? {
            capability_bridge::OpenSession(_) => {
                // Create session and root cap.
                // Placeholder: in production mint endpoint/CNode and store SessionCaps.
                let mut builder = Builder::new_default();
                // build response...
                let response_words = serialize::write_message_to_words(&builder);
                return Ok((serialize::words_to_bytes(&response_words), None));
            }
            capability_bridge::PreGrantMemory(params) => {
                let _bundle = params?.get_bundle()?;
                // Expect an extraCap (bundle CNode) — map frames and record in session.
                let mut builder = Builder::new_default();
                let response_words = serialize::write_message_to_words(&builder);
                return Ok((serialize::words_to_bytes(&response_words), None));
            }
            capability_bridge::RegisterQueue(params) => {
                let queue_id = params?.get_queueId();
                // Mint notification cap badged with session+queue and return it to caller
                let notify_cap = self.mint_notification_cap(badge as u64, queue_id)?;
                let mut builder = Builder::new_default();
                let response_words = serialize::write_message_to_words(&builder);
                return Ok((serialize::words_to_bytes(&response_words), Some(notify_cap)));
            }
            _ => Err(CapError::TransferFailed),
        }
    }

    /// Mint a notification cap for session/queue. Placeholder: implement via retype/mint.
    fn mint_notification_cap(&self, _session_id: u64, _queue_id: u16) -> Result<Cap<Notification>, CapError> {
        // TODO: allocate untyped, retype->Notification, then mint into CSpace slot with badge.
        Err(CapError::TransferFailed)
    }
}
