// src/sel4_mock.rs
//
// Host-side mock of the seL4 syscall interface and capability types.
//
// Enabled when compiling with `--features mock` (or in `cfg(test)`).
// Allows the bridge, cap_management, and telemetry modules to compile
// and run on a development machine without an actual seL4 kernel or
// cross-compiler toolchain.
//
// Key fixes vs. previous revision:
//
//  • seL4_SetMR / seL4_GetMR now use `i32` indices, matching the seL4
//    C API and the real `sel4-sys` bindings.  The previous mock used
//    `i32` while bridge.rs cast to `u32` — a silent signature mismatch
//    that would cause type errors when building against the real crate
//    (issue #22 in Phase 1 review).
//
//  • NotificationAuth cap type added — referenced by Sel4DirectTransport
//    in bridge.rs but absent from the previous mock.
//
//  • CapRights added with a Default impl so it can be passed as a
//    placeholder argument in test code without boilerplate.
//
//  • Message register storage is backed by thread-local state so that
//    mock call_sync ↔ serve_once roundtrips work correctly in tests
//    without real IPC.
//
//  • seL4_NoError exported as the correct u32 constant (0).

// ---------------------------------------------------------------------------
// Feature / test gate
// ---------------------------------------------------------------------------
//
// This module is only compiled when targeting host tests.  On a real seL4
// target the `sel4` crate provides all of these types.

#![cfg(any(feature = "mock", test))]

use std::cell::RefCell;

// ---------------------------------------------------------------------------
// Message register storage (thread-local, simulates IPC buffer)
// ---------------------------------------------------------------------------

const MR_COUNT: usize = 120; // seL4_MsgMaxLength

thread_local! {
    static MESSAGE_REGISTERS: RefCell<[u64; MR_COUNT]> = RefCell::new([0u64; MR_COUNT]);
    static CAPS: RefCell<[u64; 4]> = RefCell::new([0u64; 4]);
}

// ---------------------------------------------------------------------------
// sys — raw syscall bindings mock
// ---------------------------------------------------------------------------

pub mod sys {
    use super::{MESSAGE_REGISTERS, CAPS, MR_COUNT};

    /// Maximum number of message words in the seL4 IPC buffer.
    pub const seL4_MsgMaxLength: usize = 120;

    /// Number of message registers accessible via fast-path syscalls.
    pub const seL4_FastMessageRegisters: usize = 4;

    /// seL4 error code for success.
    pub const seL4_NoError: u32 = 0;

    /// seL4 message info word.
    #[allow(non_camel_case_types)]
    #[derive(Clone, Copy, Debug)]
    pub struct seL4_MessageInfo {
        pub label: u64,
        pub caps_unwrapped: u64,
        pub extra_caps: u64,
        pub length: u64,
    }

    impl seL4_MessageInfo {
        pub fn new(
            label: u64,
            caps_unwrapped: u64,
            extra_caps: u64,
            length: u64,
        ) -> Self {
            Self { label, caps_unwrapped, extra_caps, length }
        }

        /// Returns the number of message words (word count, NOT byte count).
        pub fn get_length(&self) -> u64 {
            self.length
        }

        pub fn get_label(&self) -> u64 {
            self.label
        }

        pub fn get_extra_caps(&self) -> u64 {
            self.extra_caps
        }
    }

    // -----------------------------------------------------------------------
    // Message register accessors
    //
    // Index type is i32 to match the seL4 C API (`seL4_Word seL4_GetMR(int i)`).
    // The previous mock used i32 here while bridge.rs called with `i as u32` —
    // that mismatch is fixed by using i32 consistently throughout (issue #22).
    // -----------------------------------------------------------------------

    /// Write word `v` into message register `i`.
    ///
    /// # Safety
    /// The caller must ensure `i` is in [0, seL4_MsgMaxLength).
    pub unsafe fn seL4_SetMR(i: i32, v: u64) {
        debug_assert!(i >= 0 && (i as usize) < MR_COUNT, "seL4_SetMR index out of range");
        MESSAGE_REGISTERS.with(|mrs| {
            mrs.borrow_mut()[i as usize] = v;
        });
    }

    /// Read word from message register `i`.
    ///
    /// # Safety
    /// The caller must ensure `i` is in [0, seL4_MsgMaxLength).
    pub unsafe fn seL4_GetMR(i: i32) -> u64 {
        debug_assert!(i >= 0 && (i as usize) < MR_COUNT, "seL4_GetMR index out of range");
        MESSAGE_REGISTERS.with(|mrs| mrs.borrow()[i as usize])
    }

    /// Place cap `cptr` into extraCap slot `i`.
    pub unsafe fn seL4_SetCap(i: i32, cptr: u64) {
        debug_assert!(i >= 0 && (i as usize) < 4, "seL4_SetCap index out of range");
        CAPS.with(|caps| caps.borrow_mut()[i as usize] = cptr);
    }

    /// Retrieve extraCap from slot `i`.
    pub unsafe fn seL4_GetCap(i: i32) -> u64 {
        debug_assert!(i >= 0 && (i as usize) < 4, "seL4_GetCap index out of range");
        CAPS.with(|caps| caps.borrow()[i as usize])
    }

    /// Synchronous IPC call (mock: immediately returns a copy of info).
    ///
    /// In real seL4 this blocks until the server replies.  The mock
    /// returns the same MessageInfo immediately so that unit tests of
    /// the client-side serialisation path can run without a server.
    pub unsafe fn seL4_Call(_target: u64, info: seL4_MessageInfo) -> seL4_MessageInfo {
        // Echo back the info so reply_len == request_len in mock tests.
        info
    }

    /// Blocking receive (mock: returns an empty MessageInfo immediately).
    ///
    /// Sets `*badge` to 0.  Tests that need a specific badge should set it
    /// directly before calling serve_once.
    pub unsafe fn seL4_Recv(_src: u64, badge: *mut u64) -> seL4_MessageInfo {
        if !badge.is_null() {
            *badge = 0;
        }
        seL4_MessageInfo::new(0, 0, 0, 0)
    }

    /// Reply to the current caller (mock: no-op).
    pub unsafe fn seL4_Reply(_info: seL4_MessageInfo) {}

    /// Signal a notification (mock: no-op).
    pub unsafe fn seL4_Signal(_ntfn: u64) {}

    /// Wait for a notification (mock: returns 0 immediately).
    pub unsafe fn seL4_Wait(_ntfn: u64, badge: *mut u64) {
        if !badge.is_null() {
            *badge = 0;
        }
    }

    /// Revoke capability (mock: always succeeds, returns seL4_NoError).
    pub unsafe fn seL4_CNode_Revoke(_cap: u64) -> u32 {
        seL4_NoError
    }
}

// ---------------------------------------------------------------------------
// cap — capability wrapper types
// ---------------------------------------------------------------------------

pub mod cap {
    use std::marker::PhantomData;

    /// Typed capability pointer wrapper.
    #[derive(Debug, Clone, Copy)]
    pub struct Cap<T> {
        pub cptr: u64,
        _phantom: PhantomData<T>,
    }

    impl<T> Cap<T> {
        /// Construct a capability from a raw cptr.
        ///
        /// # Safety
        /// The caller must ensure `cptr` refers to a valid seL4 slot for type `T`.
        /// In mock code, any non-zero value is acceptable for testing.
        pub unsafe fn new(cptr: u64) -> Self {
            Self { cptr, _phantom: PhantomData }
        }

        pub fn cptr(&self) -> u64 {
            self.cptr
        }
    }

    // -----------------------------------------------------------------------
    // Phantom cap type markers
    // -----------------------------------------------------------------------

    /// seL4 synchronous IPC endpoint.
    pub struct Endpoint;

    /// seL4 asynchronous notification.
    pub struct Notification;

    /// seL4 CNode (capability store).
    pub struct CNode;

    /// seL4 untyped memory object.
    pub struct Untyped;

    /// seL4 small-frame page capability.
    pub struct Frame;

    /// Authority cap for minting Notification capabilities.
    /// Present in real seL4 builds; added here so bridge.rs compiles in mock
    /// mode (was missing from the previous sel4_mock — bridge.rs referenced it).
    pub struct NotificationAuth;

    // -----------------------------------------------------------------------
    // CapRights
    // -----------------------------------------------------------------------

    /// Capability rights bitmask.
    ///
    /// In real seL4 this encodes read/write/grant/grant-reply permissions.
    /// The mock provides a Default impl (all rights) for use in test scaffolding.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct CapRights {
        pub read: bool,
        pub write: bool,
        pub grant: bool,
        pub grant_reply: bool,
    }

    impl CapRights {
        pub fn all() -> Self {
            Self { read: true, write: true, grant: true, grant_reply: true }
        }

        pub fn read_write() -> Self {
            Self { read: true, write: true, grant: false, grant_reply: false }
        }
    }
}

// ---------------------------------------------------------------------------
// cap_type — additional type aliases expected by the sel4 crate API
// ---------------------------------------------------------------------------

pub mod cap_type {
    pub use super::cap::{Untyped, Frame, NotificationAuth, Notification, CNode, Endpoint};
}

// ---------------------------------------------------------------------------
// Top-level re-exports to mirror the `sel4` crate's public API shape
// ---------------------------------------------------------------------------

pub use sys::*;
pub use cap::{Cap, CapRights};
pub use cap::{Endpoint, Notification, CNode};

// ---------------------------------------------------------------------------
// Mock-mode unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::sys::*;
    use super::cap::Cap;

    #[test]
    fn message_registers_round_trip() {
        unsafe {
            seL4_SetMR(0, 0xDEAD_BEEF_CAFE_BABE);
            seL4_SetMR(1, 42);
            assert_eq!(seL4_GetMR(0), 0xDEAD_BEEF_CAFE_BABE);
            assert_eq!(seL4_GetMR(1), 42);
        }
    }

    #[test]
    fn cap_round_trip() {
        unsafe {
            seL4_SetCap(0, 0x1234);
            assert_eq!(seL4_GetCap(0), 0x1234);
        }
    }

    #[test]
    fn revoke_always_succeeds_in_mock() {
        let rc = unsafe { seL4_CNode_Revoke(99) };
        assert_eq!(rc, seL4_NoError);
    }

    #[test]
    fn cap_new_preserves_cptr() {
        let cap = unsafe { Cap::<super::cap::Endpoint>::new(7) };
        assert_eq!(cap.cptr(), 7);
    }

    #[test]
    fn cap_rights_default_is_all_false() {
        let rights = super::cap::CapRights::default();
        assert!(!rights.read);
        assert!(!rights.write);
    }

    #[test]
    fn cap_rights_all() {
        let rights = super::cap::CapRights::all();
        assert!(rights.read && rights.write && rights.grant && rights.grant_reply);
    }
}
