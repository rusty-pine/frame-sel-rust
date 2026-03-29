# src/schemas/bridge.capnp
#
# Cap'n Proto schema for the seL4 capability-marshaling bridge.
#
# Changes vs. previous revision:
#
#  • @0x... file ID annotation added — required by the Cap'n Proto compiler
#    to produce stable type IDs across schema evolution.  Without this,
#    capnp compile assigns a random ID each run, breaking binary
#    compatibility and making Kani TYPE_ID constants unstable.
#
#  • CapBundle moved before the interfaces that reference it so the
#    compiler sees the definition before the use (avoids forward-ref
#    warnings on some capnp compiler versions).
#
#  • ServiceID gains an optional `timestamp` field (@3) for tracing.
#
#  • All field and method names are documented inline.

@0xb5d23b5c9a8f4e12;  # Stable file ID — do NOT change after first commit.

# ---------------------------------------------------------------------------
# Structs
# ---------------------------------------------------------------------------

struct ServiceID {
  # Identifies an active session endpoint.
  sessionId  @0 :UInt64;   # Assigned by server on openSession
  capIndex   @1 :UInt32;   # Slot index within the session CNode
  badge      @2 :UInt32;   # Pre-computed badge for O(1) routing
  timestamp  @3 :UInt64;   # Creation time (ns since boot, optional — 0 if unused)
}

struct CapBundle {
  # A temporary CNode containing a contiguous range of frame capabilities.
  # Transferred as a single extraCap to avoid per-frame IPC round-trips.
  basePaddr  @0 :UInt64;   # Physical base address of the first frame
  numFrames  @1 :UInt32;   # Number of frame caps in the bundle CNode
}

# ---------------------------------------------------------------------------
# Interfaces
# ---------------------------------------------------------------------------

interface CapabilityBridge {
  # Root interface.  The client holds an endpoint cap to this interface.

  # Open a new session for the given client ID.
  # The server creates a SessionCaps entry and returns a Session capability.
  # Parameter name is `clientId` — the generated accessor is `get_client_id()`.
  openSession    @0 (clientId :UInt32) -> (session :Session);

  # Pre-grant a bundle of memory frames to the server.
  # The caller transfers a temporary CNode (the bundle) as extraCap[0].
  preGrantMemory @1 (bundle :CapBundle) -> (status :UInt16);
}

interface Session {
  # Per-session interface returned by CapabilityBridge.openSession.

  # Register a queue and receive a badged Notification cap in the reply's
  # extraCap[0].  The badge encodes (sessionId << 16 | queueId) for O(1)
  # routing on the server side.
  registerQueue  @0 (queueId :UInt16) -> (status :UInt16);

  # Acquire a handle to a named service.
  acquireService @1 (serviceType :Text) -> (serviceHandle :ServiceHandle);
}

interface ServiceHandle {
  # Handle to a specific service instance within a session.

  # Synchronous call: serialise `payload`, send via seL4_Call, receive reply.
  call @0 (payload :Data) -> (response :Data);
}
