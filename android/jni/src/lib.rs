//! JNI bridge — drive a native SPORE node from the Android app (Kotlin).
//!
//! Additive over the frozen core: this crate depends on `spore` and exposes a
//! small opaque-handle API. It builds for Android with `cargo-ndk` and also
//! `cargo check`s on the host (the `jni` crate is pure Rust), so the Rust side is
//! verifiable in normal CI.
//!
//! Kotlin side: `class SporeNative` with matching `external fun native*`.
use jni::objects::{JByteArray, JClass, JFloatArray, JString};
use jni::sys::{jboolean, jbyteArray, jfloatArray, jint, jlong, JNI_FALSE, JNI_TRUE};
use jni::JNIEnv;
use spore::bridge::hub::{Hub, Shared};
use spore::{addr_of, topic_of, Envelope, Forward, Iface, Node, Src};
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Mutex;
use std::thread;

/// Everything one node needs, behind a `jlong` handle.
struct Runtime {
    hub: Shared,
    inbox: Mutex<Receiver<Vec<u8>>>, // delivered envelope wires (drained by nativePollDelivery)
    // Kotlin-driven bridges (BLE, audio, Wi-Fi Direct, WebView): each registers a
    // hub iface and pumps it by polling for outbound frames + pushing inbound —
    // the same poll model as the delivery inbox, so no Rust→Kotlin callbacks.
    ifaces: Mutex<HashMap<i32, Receiver<Forward>>>,
    // Streaming audio demodulator state + frames it has completed. The mic PCM is
    // captured by Kotlin (AudioRecord) and fed here; the DSP is the same tested
    // `bridge::audio` the desktop daemon uses, so phone and laptop interoperate.
    demod: Mutex<spore::bridge::audio::Demod>,
    // Completed demod frames waiting for the poll loop to drain them. Bounded: the
    // mic thread produces continuously, so if the consumer stalls this must not
    // grow without limit. A demod backlog is stale audio, not data worth keeping,
    // so the oldest frames are dropped once it is full (see DEMOD_OUT_MAX).
    demod_out: Mutex<VecDeque<Vec<u8>>>,
}

/// Most completed demod frames to hold before dropping the oldest. A frame is one
/// short envelope; a few dozen is ample slack for a briefly busy poll loop without
/// letting a fast or hostile audio source grow the queue unboundedly.
const DEMOD_OUT_MAX: usize = 64;

/// Handles we've handed to Kotlin and not yet freed.
///
/// A raw `jlong` carries no way to tell a live runtime from a zero, a typo, or
/// one that was already freed — dereferencing any of those is undefined
/// behaviour. Registering every live handle turns all three into a lookup miss,
/// so the worst case becomes "this call does nothing" instead of a crash or
/// worse. (A handle freed by another thread *during* a call is still a race the
/// registry can't close; the app keeps one runtime for its lifetime and never
/// frees it, which is why that path stays theoretical.)
fn live() -> &'static Mutex<std::collections::HashSet<jlong>> {
    static LIVE: std::sync::OnceLock<Mutex<std::collections::HashSet<jlong>>> = std::sync::OnceLock::new();
    LIVE.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

/// Resolve a handle, or `None` if it is zero, unknown, or already freed.
fn rt<'a>(ptr: jlong) -> Option<&'a Runtime> {
    if ptr == 0 {
        return None;
    }
    let known = live().lock().ok()?.contains(&ptr);
    if !known {
        return None;
    }
    Some(unsafe { &*(ptr as *const Runtime) })
}

/// Create a runtime. `seed` is null for a fresh identity, or 32 bytes to restore.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeNew(
    env: JNIEnv,
    _class: JClass,
    seed: JByteArray,
) -> jlong {
    let node = if seed.is_null() {
        Node::new("android", &[])
    } else {
        match env.convert_byte_array(&seed) {
            Ok(b) if b.len() == 32 => {
                let mut s = [0u8; 32];
                s.copy_from_slice(&b);
                Node::from_seed("android", &[], &s)
            }
            _ => Node::new("android", &[]),
        }
    };
    let hub = Hub::new(node);
    let (tx, rx) = channel();
    hub.set_delivery_sink(tx);
    let ptr = Box::into_raw(Box::new(Runtime {
        hub,
        inbox: Mutex::new(rx),
        ifaces: Mutex::new(HashMap::new()),
        demod: Mutex::new(spore::bridge::audio::Demod::new()),
        demod_out: Mutex::new(VecDeque::new()),
    })) as jlong;
    if let Ok(mut set) = live().lock() {
        set.insert(ptr);
    }
    ptr
}

/// Destroy a runtime.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeFree(_env: JNIEnv, _class: JClass, ptr: jlong) {
    // Deregister first: any later call with this handle now misses the lookup
    // instead of touching freed memory. Freeing twice is a no-op.
    let was_live = live().lock().map(|mut s| s.remove(&ptr)).unwrap_or(false);
    if was_live {
        unsafe {
            drop(Box::from_raw(ptr as *mut Runtime));
        }
    }
}

/// The node's 8-byte address.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeAddr(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jbyteArray {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    let a = r.hub.addr();
    env.byte_array_from_slice(&a).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// The node's 32-byte signing seed (persist it; pass back to nativeNew to restore).
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeSeed(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jbyteArray {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    let s = r.hub.with_node(|n| n.seed());
    env.byte_array_from_slice(&s).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// The prekey ring (§7) — persist it *beside* the seed.
///
/// The seed restores the identity. It does not restore prekey secrets: those are
/// random, which is precisely what makes deleting them mean something (S-022). An
/// app that saves only the seed comes back unable to open mail sealed to any
/// prekey the node had rotated to.
///
/// Secret material. Store it wherever the seed is stored, and no less carefully.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativePrekeyRing(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jbyteArray {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    let s = r.hub.with_node(|n| n.prekey_ring());
    env.byte_array_from_slice(&s).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Restore a ring from `nativePrekeyRing`. Returns true on success; a malformed
/// blob is refused and leaves the node untouched.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeRestorePrekeyRing(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    blob: JByteArray,
) -> jboolean {
    let Some(r) = rt(ptr) else {
        return JNI_FALSE;
    };
    let Ok(bytes) = env.convert_byte_array(&blob) else {
        return JNI_FALSE;
    };
    if r.hub.with_node(|n| n.restore_prekey_ring(&bytes)) {
        JNI_TRUE
    } else {
        JNI_FALSE
    }
}

/// Ring health for the Advanced screen's status readout: `"count:oldestAge:nextMintIn"`,
/// all in seconds — `oldestAge` is `-1` when the oldest secret is an unstamped
/// bootstrap entry whose true age is unknowable (SPORE §7).
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativePrekeyHealth(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jni::sys::jstring {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    let now = spore::bridge::hub::now();
    let (count, oldest_age, next_mint_in) = r.hub.with_node(|n| n.prekey_health(now));
    let s = format!("{count}:{}:{next_mint_in}", oldest_age.map(|a| a as i64).unwrap_or(-1));
    env.new_string(s).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Follow a topic so its traffic is delivered to us.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeSubscribe(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    topic: JString,
) {
    let Some(r) = rt(ptr) else {
        return;
    };
    if let Ok(s) = env.get_string(&topic) {
        let s: String = s.into();
        r.hub.with_node(|n| n.subscribe(&s));
    }
}

/// Originate a signed message to `dest` (8 bytes; all-zero = public).
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeSend(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    dest: JByteArray,
    payload: JByteArray,
) {
    let Some(r) = rt(ptr) else {
        return;
    };
    let d = env.convert_byte_array(&dest).unwrap_or_default();
    let p = env.convert_byte_array(&payload).unwrap_or_default();
    if d.len() != 8 {
        return;
    }
    let mut addr = [0u8; 8];
    addr.copy_from_slice(&d);
    let _ = r.hub.send(addr, p); // oversized payloads are the caller's to avoid
}

/// Start the primary-subnet UDP broadcast bridge on a background thread. `port`
/// <= 0 uses the default.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeStartUdp(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    port: jint,
) {
    let Some(r) = rt(ptr) else {
        return;
    };
    let (iface, rx) = r.hub.register();
    let hub = r.hub.clone();
    let port = if port > 0 { Some(port as u16) } else { None };
    thread::spawn(move || {
        let _ = spore::bridge::udp::run_primary(hub, iface, rx, port);
    });
}

/// Start a TCP bridge on a background thread. `target` empty = listen; otherwise
/// connect to `host:port`.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeStartTcp(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    target: JString,
) {
    let Some(r) = rt(ptr) else {
        return;
    };
    let t: Option<String> = env.get_string(&target).ok().map(|s| s.into()).filter(|s: &String| !s.is_empty());
    let (iface, rx) = r.hub.register();
    let hub = r.hub.clone();
    thread::spawn(move || {
        let _ = spore::bridge::tcp::run(hub, iface, rx, t);
    });
}

/// Register a Kotlin-driven bridge interface; returns its iface id. Pump it with
/// nativePollForward (outbound) + nativePushRx (inbound).
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeRegisterIface(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jint {
    let Some(r) = rt(ptr) else {
        return 0;
    };
    let (iface, rx) = r.hub.register();
    r.ifaces.lock().unwrap().insert(iface as i32, rx);
    iface as jint
}

/// Retire a Kotlin-driven bridge's interface: a stopped or removed bridge.
///
/// Drops our end of its forward queue (so the bridge's drain loop sees the channel
/// disconnect and exits) and tells the hub to stop routing to that slot. The iface
/// id is not recycled — the hub keeps the slot as a hole to preserve every other
/// interface's id — so a bridge must obtain a fresh id from `nativeRegisterIface`
/// if it is ever restarted. Safe to call twice, and on an id that was never ours.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeUnregisterIface(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    iface: jint,
) {
    let Some(r) = rt(ptr) else {
        return;
    };
    r.ifaces.lock().unwrap().remove(&iface);
    r.hub.unregister(iface as Iface);
}

/// Register a Kotlin-driven bridge that will carry at most `bulkBytesPerSec` of
/// other people's file chunks; returns its iface id.
///
/// For slow links — sound, LoRa — so a big transfer somewhere in the mesh cannot
/// conscript them. Messages, announces and manifests are never counted, so the
/// link stays fully useful for talking. 0 refuses bulk outright.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeRegisterIfaceLimited(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    bulk_bytes_per_sec: jint,
) -> jint {
    let Some(r) = rt(ptr) else {
        return 0;
    };
    let (iface, rx) = r.hub.register_limited(bulk_bytes_per_sec.max(0) as u32);
    r.ifaces.lock().unwrap().insert(iface as i32, rx);
    iface as jint
}

/// The bulk budget each slow bridge suggests for itself, so the app doesn't have
/// to hard-code numbers the core already reasons about.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeSuggestedBulkBudget(
    mut env: JNIEnv,
    _class: JClass,
    kind: JString,
) -> jint {
    let Ok(k) = env.get_string(&kind) else { return -1 };
    let k: String = k.into();
    match k.as_str() {
        "audio" => spore::bridge::audio::BULK_BYTES_PER_SEC as jint,
        "meshtastic" => spore::bridge::meshtastic::BULK_BYTES_PER_SEC as jint,
        "reticulum" => spore::bridge::reticulum::BULK_BYTES_PER_SEC as jint,
        _ => -1, // unknown: the caller should register unlimited
    }
}

/// Poll one outbound frame the node wants transmitted on `iface`, or null.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativePollForward(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    iface: jint,
) -> jbyteArray {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    let map = r.ifaces.lock().unwrap();
    if let Some(rx) = map.get(&iface) {
        if let Ok(f) = rx.try_recv() {
            let bytes = match f {
                Forward::Flood { bytes, .. } => bytes,
                Forward::Directed { bytes, .. } => bytes,
            };
            return env.byte_array_from_slice(&bytes).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut());
        }
    }
    std::ptr::null_mut()
}

/// Feed an inbound frame received by a Kotlin-driven bridge into the node.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativePushRx(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    iface: jint,
    frame: JByteArray,
) {
    let Some(r) = rt(ptr) else {
        return;
    };
    let bytes = env.convert_byte_array(&frame).unwrap_or_default();
    r.hub.on_rx(iface as Iface, &bytes, None);
}

/// Poll for one delivered envelope's wire bytes, or null if the inbox is empty.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativePollDelivery(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jbyteArray {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    match r.inbox.lock().unwrap().try_recv() {
        Ok(wire) => env.byte_array_from_slice(&wire).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

/// The payload of an envelope wire (for rendering a delivered message).
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeEnvPayload(
    env: JNIEnv,
    _class: JClass,
    wire: JByteArray,
) -> jbyteArray {
    let w = env.convert_byte_array(&wire).unwrap_or_default();
    match Envelope::decode(&w) {
        Ok((e, _)) => {
            env.byte_array_from_slice(&e.payload).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Whether an envelope wire carries a valid signature.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeEnvVerify(
    env: JNIEnv,
    _class: JClass,
    wire: JByteArray,
) -> jboolean {
    let w = env.convert_byte_array(&wire).unwrap_or_default();
    match Envelope::decode(&w) {
        Ok((e, _)) if e.verify() => JNI_TRUE,
        _ => JNI_FALSE,
    }
}

/// The sender's 8-byte address (for conversations / petnames), or null if the
/// envelope is unsigned.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeEnvSrc(
    env: JNIEnv,
    _class: JClass,
    wire: JByteArray,
) -> jbyteArray {
    let w = env.convert_byte_array(&wire).unwrap_or_default();
    let addr = match Envelope::decode(&w) {
        Ok((e, _)) => match e.src {
            Src::Full(pk) => Some(addr_of(&pk)),
            Src::Short(a) => Some(a),
            Src::None => None,
        },
        Err(_) => None,
    };
    match addr {
        Some(a) => env.byte_array_from_slice(&a).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

/// The envelope's 8-byte destination (topic address for feed posts, our address
/// for DMs, all-zero for public floods).
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeEnvDest(
    env: JNIEnv,
    _class: JClass,
    wire: JByteArray,
) -> jbyteArray {
    let w = env.convert_byte_array(&wire).unwrap_or_default();
    match Envelope::decode(&w) {
        Ok((e, _)) => {
            env.byte_array_from_slice(&e.dest).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// The 8-byte topic address for a topic name (feeds/microblogging).
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeTopicAddr(
    mut env: JNIEnv,
    _class: JClass,
    topic: JString,
) -> jbyteArray {
    let s: String = match env.get_string(&topic) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let a = topic_of(&s);
    env.byte_array_from_slice(&a).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Originate like nativeSend, but return how many wire frames (fragments) the
/// payload became — the UI's send-side fragmentation status.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeSendCounted(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    dest: JByteArray,
    payload: JByteArray,
) -> jint {
    let Some(r) = rt(ptr) else {
        return 0;
    };
    let d = env.convert_byte_array(&dest).unwrap_or_default();
    let p = env.convert_byte_array(&payload).unwrap_or_default();
    if d.len() != 8 {
        return 0;
    }
    let mut addr = [0u8; 8];
    addr.copy_from_slice(&d);
    let Ok(forwards) = r.hub.with_node(|n| n.send(addr, p, spore::bridge::hub::now())) else {
        return 0; // too large for one fountain set
    };
    let count = forwards.len() as jint;
    r.hub.originate(forwards);
    count
}

// -- audio modem (16-FSK, same DSP as the desktop daemon) ---------------------

/// Modulate one frame to 48 kHz mono f32 PCM for AudioTrack.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeAudioModulate(
    env: JNIEnv,
    _class: JClass,
    payload: JByteArray,
) -> jfloatArray {
    let p = env.convert_byte_array(&payload).unwrap_or_default();
    let pcm = spore::bridge::audio::modulate(&p);
    match env.new_float_array(pcm.len() as i32) {
        Ok(arr) => {
            let _ = env.set_float_array_region(&arr, 0, &pcm);
            arr.into_raw()
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Feed captured mic PCM (48 kHz mono f32) into the streaming demodulator.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeAudioDemodPush(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    samples: JFloatArray,
) {
    let Some(r) = rt(ptr) else {
        return;
    };
    let len = env.get_array_length(&samples).unwrap_or(0) as usize;
    if len == 0 {
        return;
    }
    let mut buf = vec![0f32; len];
    if env.get_float_array_region(&samples, 0, &mut buf).is_err() {
        return;
    }
    let frames = r.demod.lock().unwrap().push(&buf);
    if !frames.is_empty() {
        let mut q = r.demod_out.lock().unwrap();
        q.extend(frames);
        // Drop the oldest beyond the cap: if the poll loop can't keep up, the
        // freshest frames are the ones worth delivering, and an unbounded queue
        // would let a fast (or malicious) audio feed grow memory without limit.
        while q.len() > DEMOD_OUT_MAX {
            q.pop_front();
        }
    }
}

/// Pop one frame the demodulator completed, or null.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeAudioDemodPop(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jbyteArray {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    match r.demod_out.lock().unwrap().pop_front() {
        Some(f) => env.byte_array_from_slice(&f).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

// -- Meshtastic codec (same protobuf as the desktop bridge) -------------------

/// Wrap an envelope as a Meshtastic MeshPacket (portnum 256) for BLE ToRadio.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeMeshtasticWrap(
    env: JNIEnv,
    _class: JClass,
    env_wire: JByteArray,
    from_node: jint,
    packet_id: jint,
) -> jbyteArray {
    let w = env.convert_byte_array(&env_wire).unwrap_or_default();
    let pkt = spore::bridge::meshtastic::encode(
        &w,
        from_node as u32,
        spore::bridge::meshtastic::BROADCAST,
        packet_id as u32,
    );
    env.byte_array_from_slice(&pkt).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Unwrap a MeshPacket; returns the SPORE envelope if it rides portnum 256,
/// else null.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeMeshtasticUnwrap(
    env: JNIEnv,
    _class: JClass,
    frame: JByteArray,
) -> jbyteArray {
    let f = env.convert_byte_array(&frame).unwrap_or_default();
    match spore::bridge::meshtastic::decode(&f) {
        Some((_from, port, payload)) if port == spore::bridge::meshtastic::PORT_PRIVATE_APP => {
            env.byte_array_from_slice(&payload).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
        }
        _ => std::ptr::null_mut(),
    }
}

// -- peers, encrypted direct messages, delivery receipts ---------------------

/// Flood this node's ANNOUNCE on every interface. Peers learn our address,
/// prekey (so they can encrypt to us) and a path back — call it periodically.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeBeacon(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    let Some(r) = rt(ptr) else {
        return;
    };
    r.hub.beacon();
}

/// Send the link-local HELLO (`hops = 0`) on every interface.
///
/// The cheap beacon. `nativeBeacon` floods mesh-wide and every node that hears it
/// relays it, which is why §5.4b holds that to roughly once an hour; this reaches
/// direct neighbours and stops. The housekeeping loop used to call `nativeBeacon`
/// every 2-30 s — the same mistake the daemon made (S-023).
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeHello(_env: JNIEnv, _class: JClass, ptr: jlong) {
    let Some(r) = rt(ptr) else {
        return;
    };
    r.hub.hello();
}

/// Peers we've heard from, freshest first, one per line:
/// `addrhex:secondsAgo:hasPrekey:announcedName` (the name may be empty and is
/// last, so a name containing ':' survives a limit-4 split).
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativePeers(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jni::sys::jstring {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    let now = spore::bridge::hub::now();
    let s = r.hub.with_node(|n| {
        n.peers(now)
            .iter()
            .map(|(a, age, key)| {
                let hex: String = a.iter().map(|b| format!("{b:02x}")).collect();
                let name = n.peer_name(a).unwrap_or("").replace('\n', " ");
                format!("{hex}:{age}:{}:{name}", if *key { 1 } else { 0 })
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    env.new_string(s).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Set the name this node announces to the mesh (a display hint others may use
/// as the default petname for us). Takes effect on the next beacon.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeSetName(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    name: JString,
) {
    let Some(r) = rt(ptr) else {
        return;
    };
    if let Ok(s) = env.get_string(&name) {
        let s: String = s.into();
        let s: String = s.chars().filter(|c| !c.is_control()).take(32).collect();
        r.hub.with_node(|n| n.petname = s);
    }
}

/// Build a shareable invite ("here's how to reach me") for this node: address,
/// the name we announce, and the given bridge specs (one per line).
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeInviteEncode(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    bridges: JString,
) -> jni::sys::jstring {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    let raw: String = env.get_string(&bridges).map(|s| s.into()).unwrap_or_default();
    let list: Vec<String> = raw.lines().filter(|l| !l.trim().is_empty()).map(|l| l.to_string()).collect();
    let addr = r.hub.addr();
    let name = r.hub.with_node(|n| n.petname.clone());
    let s = spore::invite::encode(&addr, &name, &list);
    env.new_string(s).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Parse a scanned or pasted invite. Returns `addrhex\nname\nbridge…` (bridges
/// one per line), or null if it isn't a valid invite — a mistyped or truncated
/// string fails its checksum rather than yielding a plausible wrong address.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeInviteDecode(
    mut env: JNIEnv,
    _class: JClass,
    text: JString,
) -> jni::sys::jstring {
    let s: String = match env.get_string(&text) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let Some(inv) = spore::invite::decode(&s) else {
        return std::ptr::null_mut();
    };
    let hex: String = inv.addr.iter().map(|b| format!("{b:02x}")).collect();
    let mut out = format!("{hex}\n{}", inv.name);
    for b in &inv.bridges {
        out.push('\n');
        out.push_str(b);
    }
    env.new_string(out).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Send a direct message: sealed to the peer's prekey when known, and flagged
/// for a delivery receipt. Returns "idhex:1" (encrypted) or "idhex:0"
/// (cleartext — we haven't heard their ANNOUNCE yet).
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeSendDirect(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    dest: JByteArray,
    payload: JByteArray,
) -> jni::sys::jstring {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    let d = env.convert_byte_array(&dest).unwrap_or_default();
    let p = env.convert_byte_array(&payload).unwrap_or_default();
    if d.len() != 8 {
        return std::ptr::null_mut();
    }
    let mut addr = [0u8; 8];
    addr.copy_from_slice(&d);
    let (id, forwards, encrypted) = r.hub.with_node(|n| n.send_direct(addr, &p, spore::bridge::hub::now()));
    r.hub.originate(forwards);
    let hex: String = id.iter().map(|b| format!("{b:02x}")).collect();
    let s = format!("{hex}:{}", if encrypted { 1 } else { 0 });
    env.new_string(s).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Has a delivery receipt for this envelope id (hex) come back?
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeAcked(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    id_hex: JString,
) -> jboolean {
    let Some(r) = rt(ptr) else {
        return JNI_FALSE;
    };
    let s: String = match env.get_string(&id_hex) {
        Ok(s) => s.into(),
        Err(_) => return JNI_FALSE,
    };
    if s.len() != 32 {
        return JNI_FALSE;
    }
    let mut id = [0u8; 16];
    for (i, b) in id.iter_mut().enumerate() {
        match u8::from_str_radix(&s[i * 2..i * 2 + 2], 16) {
            Ok(v) => *b = v,
            Err(_) => return JNI_FALSE,
        }
    }
    if r.hub.with_node(|n| n.acked(&id)) {
        JNI_TRUE
    } else {
        JNI_FALSE
    }
}

/// Resend ACKREQ messages whose backoff elapsed without a receipt (§5.6).
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeResendUnacked(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    let Some(r) = rt(ptr) else {
        return;
    };
    let forwards = r.hub.with_node(|n| n.resend_unacked(spore::bridge::hub::now()));
    r.hub.originate(forwards);
}

/// The readable payload of a delivered envelope: decrypted with our prekey
/// secret when it is ENCRYPTED, otherwise the payload as-is. Null if it is
/// sealed to someone else (or malformed).
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeEnvPlaintext(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    wire: JByteArray,
) -> jbyteArray {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    let w = env.convert_byte_array(&wire).unwrap_or_default();
    let Ok((e, _)) = Envelope::decode(&w) else {
        return std::ptr::null_mut();
    };
    let out = if e.flags & spore::fl::ENCRYPTED != 0 {
        match r.hub.with_node(|n| n.open(&e.payload)) {
            Some(p) => p,
            None => return std::ptr::null_mut(),
        }
    } else {
        e.payload
    };
    env.byte_array_from_slice(&out).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Whether an envelope was sealed (for the UI's lock indicator).
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeEnvEncrypted(
    env: JNIEnv,
    _class: JClass,
    wire: JByteArray,
) -> jboolean {
    let w = env.convert_byte_array(&wire).unwrap_or_default();
    match Envelope::decode(&w) {
        Ok((e, _)) if e.flags & spore::fl::ENCRYPTED != 0 => JNI_TRUE,
        _ => JNI_FALSE,
    }
}

/// How many envelopes this node is currently storing and relaying for others.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeStoreLen(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jint {
    let Some(r) = rt(ptr) else {
        return 0;
    };
    r.hub.with_node(|n| n.store_len()) as jint
}

// -- files: the protocol's manifest/chunk layer, sealed when we can ----------

fn id_from_hex(s: &str) -> Option<spore::Id> {
    if s.len() != 32 {
        return None;
    }
    let mut id = [0u8; 16];
    for (i, b) in id.iter_mut().enumerate() {
        *b = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(id)
}

/// Publish a file through the manifest/chunk layer, **sealed to `dest`'s prekey
/// when we have it** (contents *and* name encrypted). `dest` empty = public.
/// Returns "magnethex:1" (sealed) or "magnethex:0" (cleartext).
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativePublishFile(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    name: JString,
    bytes: JByteArray,
    dest_hex: JString,
) -> jni::sys::jstring {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    let name: String = env.get_string(&name).map(|s| s.into()).unwrap_or_default();
    let data = env.convert_byte_array(&bytes).unwrap_or_default();
    let dhex: String = env.get_string(&dest_hex).map(|s| s.into()).unwrap_or_default();

    let mut dest = [0u8; 8];
    let mut unicast = false;
    if dhex.len() == 16 {
        for (i, b) in dest.iter_mut().enumerate() {
            match u8::from_str_radix(&dhex[i * 2..i * 2 + 2], 16) {
                Ok(v) => *b = v,
                Err(_) => return std::ptr::null_mut(),
            }
        }
        unicast = true;
    }

    let now = spore::bridge::hub::now();
    let (magnet, forwards, sealed) = r.hub.with_node(|n| {
        if unicast {
            if let Some((m, f)) = n.publish_file_sealed(&name, &data, dest, now) {
                return (m, f, true);
            }
        }
        let (m, f) = n.publish_file(&name, &data, dest, now);
        (m, f, false)
    });
    r.hub.originate(forwards);
    let hex: String = magnet.iter().map(|b| format!("{b:02x}")).collect();
    let s = format!("{hex}:{}", if sealed { 1 } else { 0 });
    env.new_string(s).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// The largest file this node can share right now.
///
/// Manifests are trees, so the protocol itself no longer bounds a file at any
/// size a phone cares about — what bounds it is the store every chunk has to
/// live in. Clamped into a `jint`, which the protocol figure would overflow.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeMaxFileBytes(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jint {
    let Some(r) = rt(ptr) else { return 0 };
    r.hub.with_node(|n| n.max_storable_file_bytes().min(jint::MAX as usize)) as jint
}

/// Keep the store's bytes in `dir`, holding only `memBytes` of them resident,
/// and adopt anything left there by a previous run. Returns how many envelopes
/// were adopted (-1 if the directory could not be used).
///
/// This is what makes a big transfer survive the app being killed: the chunks
/// are already on disk, and the manifests among them are re-learned, so a fetch
/// resumes instead of starting over.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeSetSpillDir(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    dir: JString,
    mem_bytes: jint,
    now: jint,
) -> jint {
    let Some(r) = rt(ptr) else { return -1 };
    let Ok(dir) = env.get_string(&dir) else { return -1 };
    let dir: String = dir.into();
    r.hub.with_node(|n| {
        if mem_bytes > 0 {
            n.set_mem_budget(mem_bytes as usize);
        }
        match n.set_spill_dir(std::path::Path::new(&dir), now.max(0) as u32) {
            Ok(k) => k as jint,
            Err(_) => -1,
        }
    })
}

/// Set how many bytes this node keeps for stored traffic (its own files
/// included). Bounds what it can share and how much it can relay for others.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeSetStoreBudget(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    bytes: jint,
) {
    let Some(r) = rt(ptr) else { return };
    if bytes > 0 {
        r.hub.with_node(|n| n.set_store_budget(bytes as usize));
    }
}

/// Files we hold a manifest for, one per line:
/// `magnethex:totalBytes:chunksHeld:chunksTotal:advertisedName`.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeFiles(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jni::sys::jstring {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    let rows = r.hub.with_node(|n| n.files());
    let s = rows
        .iter()
        .map(|(m, name, total, have, count)| {
            let hex: String = m.iter().map(|b| format!("{b:02x}")).collect();
            format!("{hex}:{total}:{have}:{count}:{}", name.replace('\n', " "))
        })
        .collect::<Vec<_>>()
        .join("\n");
    env.new_string(s).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Ask the mesh for the parts we're still missing for this file.
///
/// One WANT frame can only name so many ids, and a big file is a tree whose
/// deeper levels are not even nameable until the levels above arrive — so this
/// asks for a burst of frames per call and gets called again each housekeeping
/// tick. It converges rather than completing in one shot.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeFetchFile(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    magnet_hex: JString,
) {
    // Frames of ids to ask for per call: ~1400 ids in flight, which is a few MB
    // of chunks. Small enough not to bury a slow bridge in replies.
    const BURST: usize = 16;

    let Some(r) = rt(ptr) else {
        return;
    };
    let s: String = match env.get_string(&magnet_hex) {
        Ok(s) => s.into(),
        Err(_) => return,
    };
    let Some(magnet) = id_from_hex(&s) else { return };
    let forwards = r.hub.with_node(|n| n.fetch_n(&magnet, BURST));
    r.hub.originate(forwards);
}

/// The file's real name, decrypted from the sealed header when it was sealed to
/// us. Null if we don't know it, or it was sealed to someone else. Does not
/// touch the file's bytes.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeFileName(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    magnet_hex: JString,
) -> jni::sys::jstring {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    let s: String = match env.get_string(&magnet_hex) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let Some(magnet) = id_from_hex(&s) else {
        return std::ptr::null_mut();
    };
    let Some(name) = r.hub.with_node(|n| n.file_name(&magnet)) else {
        return std::ptr::null_mut();
    };
    env.new_string(name).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Write a completed file straight to `path`, decrypting as it goes, and return
/// the bytes written (-1 on failure). Nothing larger than one chunk is ever held
/// in memory, and the bytes never cross into the JVM heap — which is the whole
/// point of having it here rather than returning an array.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeSaveFile(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    magnet_hex: JString,
    path: JString,
) -> jlong {
    let Some(r) = rt(ptr) else { return -1 };
    let (Ok(magnet_s), Ok(path_s)) = (env.get_string(&magnet_hex), env.get_string(&path)) else {
        return -1;
    };
    let (magnet_s, path_s): (String, String) = (magnet_s.into(), path_s.into());
    let Some(magnet) = id_from_hex(&magnet_s) else { return -1 };

    let Ok(f) = std::fs::File::create(&path_s) else { return -1 };
    let mut w = std::io::BufWriter::new(f);
    let written = r.hub.with_node(|n| n.open_file_to(&magnet, &mut w)).map(|(_, n)| n);
    let flushed = std::io::Write::flush(&mut w).is_ok();
    match written {
        Some(n) if flushed => n as jlong,
        // Don't leave a half-written file behind looking like the real thing.
        _ => {
            let _ = std::fs::remove_file(&path_s);
            -1
        }
    }
}

/// A complete file as `u16 nameLen · name · bytes`, decrypted if it was sealed
/// to us. Null while chunks are missing, or if it was sealed to someone else.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeOpenFile(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    magnet_hex: JString,
) -> jbyteArray {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    let s: String = match env.get_string(&magnet_hex) {
        Ok(s) => s.into(),
        Err(_) => return std::ptr::null_mut(),
    };
    let Some(magnet) = id_from_hex(&s) else {
        return std::ptr::null_mut();
    };
    let Some((name, bytes)) = r.hub.with_node(|n| n.open_file(&magnet)) else {
        return std::ptr::null_mut();
    };
    let nb = name.as_bytes();
    let nlen = nb.len().min(u16::MAX as usize);
    let mut out = Vec::with_capacity(2 + nlen + bytes.len());
    out.extend_from_slice(&(nlen as u16).to_be_bytes());
    out.extend_from_slice(&nb[..nlen]);
    out.extend_from_slice(&bytes);
    env.byte_array_from_slice(&out).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Receive-side fragmentation status as "idhex:have/count" lines joined by
/// '\n' (empty string when nothing is reassembling) — the UI's "receiving X/N".
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeFragStatus(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jni::sys::jstring {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    let rows = r.hub.with_node(|n| n.frag_progress());
    let s = rows
        .iter()
        .map(|(id, have, count)| {
            let hex: String = id.iter().map(|b| format!("{b:02x}")).collect();
            format!("{hex}:{have}/{count}")
        })
        .collect::<Vec<_>>()
        .join("\n");
    env.new_string(s).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Start the plain limited-broadcast UDP bridge (255.255.255.255) — used on a
/// Wi-Fi Direct group where the subnet-directed broadcast isn't discoverable.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeStartUdpLimited(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    port: jint,
) {
    let Some(r) = rt(ptr) else {
        return;
    };
    let (iface, rx) = r.hub.register();
    let hub = r.hub.clone();
    let port = if port > 0 { port as u16 } else { 7373 };
    thread::spawn(move || {
        let _ = spore::bridge::udp::run(hub, iface, rx, port);
    });
}

// -- L4 request/response (used app-side for the profile pull) -----------------
//
// These expose the core's existing RPC layer to Kotlin without adding anything
// to the wire: a REQUEST/RESPONSE is an ordinary signed DATA envelope, so the
// profile feature is entirely an application on top of primitives the frozen
// protocol already ships. The app defines the request path and the reply body
// format; this glue only carries bytes and preserves the reply's authenticated
// sender so the app can reject a forged one.

/// Ask `dest` for `path` (method is always GET here). Returns the request id to
/// match the reply with [`nativeRpcTakeResponse`], or 0 on a bad address.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeRpcRequest(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    dest: JByteArray,
    path: JString,
    body: JByteArray,
) -> jlong {
    let Some(r) = rt(ptr) else { return 0 };
    let d = env.convert_byte_array(&dest).unwrap_or_default();
    if d.len() != 8 {
        return 0;
    }
    let mut addr = [0u8; 8];
    addr.copy_from_slice(&d);
    let path: String = env.get_string(&path).map(|s| s.into()).unwrap_or_default();
    let body = env.convert_byte_array(&body).unwrap_or_default();
    let req = spore::rpc::Request { method: "GET".into(), path, body };
    let now = spore::bridge::hub::now();
    let (id, forwards) = r.hub.with_node(|n| n.request(addr, req, now));
    r.hub.originate(forwards);
    id as jlong
}

/// Drain requests delivered to us as a service, or null if there are none.
/// Each is packed as `from[8] · id[8 BE] · pathLen[2 BE] · path · bodyLen[4 BE]
/// · body`, concatenated; Kotlin walks the buffer.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeRpcPollRequests(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jbyteArray {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    let reqs = r.hub.with_node(|n| n.poll_requests());
    if reqs.is_empty() {
        return std::ptr::null_mut();
    }
    let mut out = Vec::new();
    for (from, id, req) in reqs {
        out.extend_from_slice(&from);
        out.extend_from_slice(&id.to_be_bytes());
        let pb = req.path.as_bytes();
        out.extend_from_slice(&(pb.len().min(u16::MAX as usize) as u16).to_be_bytes());
        out.extend_from_slice(&pb[..pb.len().min(u16::MAX as usize)]);
        out.extend_from_slice(&(req.body.len() as u32).to_be_bytes());
        out.extend_from_slice(&req.body);
    }
    env.byte_array_from_slice(&out).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Reply to request `req_id` from `to` with `status` and `body`.
///
/// A reply can be tens of KB (a profile carries an avatar), which does not fit
/// one envelope, so — unlike the core's `respond` — this sends the RESPONSE
/// payload through the fountain-fragmenting `send` path. The receiver reassembles
/// it and re-enters the RPC demux exactly as a one-shot reply would. The payload
/// bytes match `rpc::encode_response`: `[0x03][id:8 BE][status:2 BE][body]`.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeRpcRespond(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    to: JByteArray,
    req_id: jlong,
    status: jint,
    body: JByteArray,
) {
    let Some(r) = rt(ptr) else { return };
    let d = env.convert_byte_array(&to).unwrap_or_default();
    if d.len() != 8 {
        return;
    }
    let mut addr = [0u8; 8];
    addr.copy_from_slice(&d);
    let body = env.convert_byte_array(&body).unwrap_or_default();
    let mut payload = Vec::with_capacity(11 + body.len());
    payload.push(spore::rpc::RESPONSE_TAG);
    payload.extend_from_slice(&(req_id as u64).to_be_bytes());
    payload.extend_from_slice(&(status.max(0) as u16).to_be_bytes());
    payload.extend_from_slice(&body);
    let now = spore::bridge::hub::now();
    if let Ok(forwards) = r.hub.with_node(|n| n.send(addr, payload, now)) {
        r.hub.originate(forwards);
    }
}

/// Take the reply to `req_id` if it has arrived, packed as
/// `from[8] · status[2 BE] · body`; null if nothing has come back yet. `from` is
/// the reply's **authenticated** sender — the app checks it equals the address it
/// asked, so a flooded forgery for a contact's profile is rejected.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeRpcTakeResponse(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    req_id: jlong,
) -> jbyteArray {
    let Some(r) = rt(ptr) else {
        return std::ptr::null_mut();
    };
    let Some((from, resp)) = r.hub.with_node(|n| n.take_response_from(req_id as u64)) else {
        return std::ptr::null_mut();
    };
    let mut out = Vec::with_capacity(10 + resp.body.len());
    out.extend_from_slice(&from);
    out.extend_from_slice(&resp.status.to_be_bytes());
    out.extend_from_slice(&resp.body);
    env.byte_array_from_slice(&out).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}
