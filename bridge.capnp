struct ServiceID {
  sessionId @0 :UInt64;
  capIndex @1 :UInt32;
  badge @2 :UInt32;  # Pre-computed badge for O(1) routing
}

interface CapabilityBridge {
  openSession @0 (clientId :UInt32) -> (session :Session);
  preGrantMemory @1 (bundle :CapBundle) -> (status :UInt16);
}

interface Session {
  registerQueue @0 (queueId :UInt16) -> (status :UInt16);
  acquireService @1 (serviceType :Text) -> (serviceHandle :ServiceHandle);
}

interface ServiceHandle {
  call @0 (payload :Data) -> (response :Data);
}

struct CapBundle {
  basePaddr @0 :UInt64;
  numFrames @1 :UInt32;
}
