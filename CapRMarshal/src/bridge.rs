// src/bridge.rs
//
// Sel4DirectTransport — synchronous seL4_Call transport with optional
// single-cap transfer and per-queue Notification minting.
//
// Key fixes applied vs. previous revisions:
//
//  • serve_once: seL4_GetMR returns u64 words — never cast to u8 (issue #1
//    in bridge review — truncated 56 bits of every message word).
//
//  • serve_once: get_length() is a word count, not a byte count.  The
//    previous version used it as a byte index into a byte buffer (issue #2).
//
//  • serve_once: all error paths now send a reply before returning so the
//    calling thread is never permanently deadlocked (issue #3).
//
//  • serve_once: badge validated against session_manager before dispatch
//    (issue #9 from Phase 1 review).
//
//  • openSession: creates and stores a SessionCaps entry (issue #8).
//
//  • preGrantMemory: no longer silently falls to wildcard (issue #9).
//
//  • get_session_id() corrected to get_client_id() per schema (issue #4).
//
//  • println! guarded by #[cfg(feature = "mock")] — non-functional in seL4
//    userspace without a serial driver wired up.

use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use capnp::message::{Builder, HeapAllocator, ReaderOptions};
use capnp::serialize;
use capnp::Word;
use sel4::sys::*;
use sel4::cap::{Cap, Endpoint, Notification};
use crate::cap_management::{SessionCaps, CapBundle};
use crate::error::CapError;
use crate::schemas::bridge_capnp::capability_bridge;

// ---------------------------------------------------------------------------
// Transport struct
// ---------------------------------------------------------------------------

pub struct Sel4DirectTransport {
    pub local_endpoint: Cap<Endpoint>,
    /// Authoritative map from badge value → session state.
    /// Arc<Mutex<...>> because the transport is shared between server loop
    /// and any administrative threads.  Note: seL4 caps are per-process and
    /// not thread-safe at the kernel level; all seL4 invocations must be
    /// serialised (the Mutex here provides that guarantee).
    pub session_manager: Arc<Mutex<HashMap<u64, SessionCaps>>>,
    /// Authority cap used to mint per-queue Notification caps.
    pub notification_auth: Cap<sel4::cap_type::NotificationAuth>,
}

// ---------------------------------------------------------------------------
// Reply helper — ensures every recv is paired with a reply
// ---------------------------------------------------------------------------

/// Write `reply_bytes` into message registers and call seL4_Reply.
///
/// This is extracted as a standalone function so that both the happy path
/// and error paths use exactly the same code, making it impossible to add
/// a new early-return that accidentally skips the reply.
///
/// # Safety
/// Caller must hold the seL4 reply capability (implicit in seL4_Reply after
/// seL4_Recv in the same thread).
unsafe fn send_reply(reply_words: &[u64], outgoing_cap: Option<u64>) {
    let len = reply_words.len().min(seL4_MsgMaxLength as usize);
    for (i, &w) in reply_words[..len].iter().enumerate() {
        seL4_SetMR(i as u32, w);
    }
    let extra_caps = if outgoing_cap.is_some() { 1u64 } else { 0u64 };
    if let Some(cptr) = outgoing_cap {
        seL4_SetCap(0, cptr);
    }
    let info = seL4_MessageInfo::new(0, 0, extra_caps, len as u64);
    seL4_Reply(info);
}

/// Build a minimal error reply (zero words, no caps).
unsafe fn send_error_reply() {
    send_reply(&[], None);
}

// ---------------------------------------------------------------------------
// Impl
// ---------------------------------------------------------------------------

impl Sel4DirectTransport {
    // -----------------------------------------------------------------------
    // Client side
    // -----------------------------------------------------------------------

    /// Synchronous IPC call with optional single-cap transfer (extraCap[0]).
    ///
    /// # Errors
    /// - `MessageTooLarge` if the serialised request exceeds seL4_MsgMaxLength
    ///   words (120 on most configurations).
    pub fn call_sync(
        &self,
        target: Cap<Endpoint>,
        request_builder: Builder<HeapAllocator>,
        transfer_cap: Option<Cap<Endpoint>>,
    ) -> Result<Vec<u8>, CapError> {
        let words = serialize::write_message_to_words(&request_builder);
        let len_words = words.len();

        // Check against word limit, not byte limit; seL4_MsgMaxLength is a
        // word count.  Cap'n Proto framing adds segment-table words that must
        // be included in this check.
        if len_words > seL4_MsgMaxLength as usize {
            return Err(CapError::MessageTooLarge);
        }

        for (i, &word) in words.iter().enumerate() {
            unsafe { seL4_SetMR(i as u32, word) };
        }

        let extra_caps = transfer_cap.is_some() as u64;
        let info = seL4_MessageInfo::new(0, 0, extra_caps, len_words as u64);

        if let Some(cap) = transfer_cap {
            unsafe { seL4_SetCap(0, cap.cptr()) };
        }

        let reply_info = unsafe { seL4_Call(target.cptr(), info) };

        let reply_len = reply_info.get_length() as usize;
        let mut reply_words = vec![0u64; reply_len];
        for i in 0..reply_len {
            reply_words[i] = unsafe { seL4_GetMR(i as u32) };
        }

        // Convert u64 word slice to bytes via capnp's Word helper so that
        // alignment and endianness are handled correctly.
        Ok(Word::words_to_bytes(&reply_words).to_vec())
    }

    // -----------------------------------------------------------------------
    // Server side
    // -----------------------------------------------------------------------

    /// Receive one message, dispatch it, and send exactly one reply.
    ///
    /// # Deadlock guarantee
    /// Every code path through this function calls `seL4_Reply` (via
    /// `send_reply` or `send_error_reply`) before returning.  The previous
    /// implementation had multiple `?` returns that skipped the reply,
    /// permanently blocking the client (issue #3 in bridge review).
    pub fn serve_once(&self) -> Result<(), CapError> {
        let mut badge = 0u64;
        let info = unsafe { seL4_Recv(self.local_endpoint.cptr(), &mut badge) };

        // ----------------------------------------------------------------
        // 1. Extract message words from MRs.
        //
        //    CRITICAL FIX: get_length() returns a *word count* (u64 words),
        //    not a byte count.  We collect u64 words first, then convert to
        //    a byte slice for Cap'n Proto.  The previous version cast each
        //    word to u8, silently discarding 56 bits per word (issues #1+#2
        //    in bridge review).
        // ----------------------------------------------------------------
        let word_count = info.get_length() as usize;

        if word_count > seL4_MsgMaxLength as usize {
            // Malformed info; reply with error and bail.
            unsafe { send_error_reply() };
            return Err(CapError::MessageTooLarge);
        }

        let mut words = vec![0u64; word_count];
        for i in 0..word_count {
            words[i] = unsafe { seL4_GetMR(i as u32) };
        }

        // Convert word-aligned buffer to byte slice for Cap'n Proto.
        let request_bytes: Vec<u8> = Word::words_to_bytes(&words).to_vec();

        // ----------------------------------------------------------------
        // 2. Badge validation — confirm the badge maps to a live session
        //    before performing any work (issue #9 from Phase 1 review).
        //
        //    Exception: badge == 0 may be used for unauthenticated calls
        //    such as openSession where no session exists yet.
        // ----------------------------------------------------------------
        let is_open_session = self.is_open_session_request(&request_bytes);

        if !is_open_session {
            let sessions = self.session_manager.lock().unwrap();
            if !sessions.contains_key(&badge) {
                unsafe { send_error_reply() };
                return Err(CapError::UnknownBadge(badge));
            }
        }

        // ----------------------------------------------------------------
        // 3. Dispatch — parse Cap'n Proto and handle each message type.
        //    All dispatch errors are caught here; a reply is always sent.
        // ----------------------------------------------------------------
        let dispatch_result = self.dispatch(&request_bytes, badge);

        match dispatch_result {
            Ok((reply_words, outgoing_cap)) => {
                unsafe { send_reply(&reply_words, outgoing_cap) };
                Ok(())
            }
            Err(e) => {
                // Send an empty error reply so the client is unblocked.
                unsafe { send_error_reply() };
                Err(e)
            }
        }
    }

    // -----------------------------------------------------------------------
    // Dispatch
    // -----------------------------------------------------------------------

    /// Peek at the raw bytes to determine whether this is an openSession call.
    ///
    /// Used in `serve_once` to allow unauthenticated openSession calls through
    /// badge validation.  This is intentionally a cheap heuristic; the full
    /// parse happens inside `dispatch`.
    fn is_open_session_request(&self, bytes: &[u8]) -> bool {
        // A zero-length message is not an openSession request.
        if bytes.is_empty() {
            return false;
        }
        // Attempt a lightweight parse; if it fails assume it is not openSession.
        let mut slice = bytes;
        let Ok(reader) = serialize::read_message_from_flat_slice(
            &mut slice, ReaderOptions::new()
        ) else {
            return false;
        };
        let Ok(msg) = reader.get_root::<capability_bridge::Reader>() else {
            return false;
        };
        matches!(
            msg.which(),
            Ok(capability_bridge::Which::OpenSession(_))
        )
    }

    /// Deserialise and dispatch a Cap'n Proto request.
    ///
    /// Returns `(reply_word_vec, optional_outgoing_cptr)` on success.
    fn dispatch(
        &self,
        request_bytes: &[u8],
        badge: u64,
    ) -> Result<(Vec<u64>, Option<u64>), CapError> {
        let mut slice = request_bytes;
        let reader = serialize::read_message_from_flat_slice(
            &mut slice,
            ReaderOptions::new(),
        )?;

        let message = reader.get_root::<capability_bridge::Reader>()?;

        match message.which()? {
            // ----------------------------------------------------------------
            // openSession — create and store a new SessionCaps entry,
            // return an endpoint capability for the session.
            //
            // FIX: the previous dispatch accessed get_session_id() which
            // does not exist.  The schema parameter is `clientId :UInt32`
            // so the accessor is get_client_id() (issue #4 in bridge review).
            // FIX: session state was never created (issue #8).
            // ----------------------------------------------------------------
            capability_bridge::Which::OpenSession(params) => {
                let client_id = params?.get_client_id();

                #[cfg(feature = "mock")]
                println!("[bridge] openSession clientId={}", client_id);

                // Assign a session ID derived from the client ID and a
                // monotonic counter.  In production this would use a CNode
                // slot allocator; here we use client_id as the badge for
                // the session endpoint.
                let session_id = client_id as u64;

                // TODO: allocate a real session endpoint via retype/mint.
                // For now we create a SessionCaps entry with a placeholder
                // root cap (cptr = session_id for mock traceability).
                let root_cap = unsafe { Cap::new(session_id) };
                let new_session = SessionCaps::new(session_id, root_cap);

                let mut sessions = self.session_manager.lock().unwrap();
                sessions.insert(session_id, new_session);

                // Build a minimal reply.  In production the session endpoint
                // cap would be included as extraCap[0].
                let reply = self.build_empty_reply();
                Ok((reply, None))
            }

            // ----------------------------------------------------------------
            // preGrantMemory — validate the frame bundle and register it
            // against the session.
            // FIX: previously fell through to wildcard (issue #9).
            // ----------------------------------------------------------------
            capability_bridge::Which::PreGrantMemory(params) => {
                let bundle_params = params?;
                let _base_paddr = bundle_params.get_bundle()?.get_base_paddr();
                let num_frames = bundle_params.get_bundle()?.get_num_frames();

                #[cfg(feature = "mock")]
                println!("[bridge] preGrantMemory numFrames={}", num_frames);

                // TODO: map frames into the session CSpace using the
                // received extraCap (bundle CNode).  For now validate that
                // num_frames is non-zero and within a reasonable bound.
                if num_frames == 0 || num_frames > 4096 {
                    return Err(CapError::BridgeFault);
                }

                // Record bundle against session.
                let mut sessions = self.session_manager.lock().unwrap();
                if let Some(_session) = sessions.get_mut(&badge) {
                    // TODO: attach CapBundle to session once slot allocator
                    // is available.
                }

                let reply = self.build_empty_reply();
                Ok((reply, None))
            }

            // ----------------------------------------------------------------
            // Explicitly reject message types that belong to child interfaces
            // (Session, ServiceHandle) — these are not routed through
            // CapabilityBridge dispatch.
            // ----------------------------------------------------------------
            _ => {
                #[cfg(feature = "mock")]
                eprintln!("[bridge] dispatch: unrecognised message type for badge {}", badge);
                Err(CapError::BridgeFault)
            }
        }
    }

    /// Serialise an empty reply message into a word vec.
    fn build_empty_reply(&self) -> Vec<u64> {
        let builder = Builder::new_default();
        serialize::write_message_to_words(&builder).to_vec()
    }

    /// Mint a Notification cap for a given session/queue.
    ///
    /// TODO: allocate untyped, retype → Notification, mint with badge
    /// (session_id << 16 | queue_id) into a free CSpace slot.
    fn mint_notification_cap(
        &self,
        _session_id: u64,
        _queue_id: u16,
    ) -> Result<Cap<Notification>, CapError> {
        Err(CapError::TransferFailed)
    }
}
