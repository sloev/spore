//! JNI bridge — drive a native SPORE node from the Android app (Kotlin).
//!
//! Additive over the frozen core: this crate depends on `spore` and exposes a
//! small opaque-handle API. It builds for Android with `cargo-ndk` and also
//! `cargo check`s on the host (the `jni` crate is pure Rust), so the Rust side is
//! verifiable in normal CI.
//!
//! Kotlin side: `class SporeNative` with matching `external fun native*`.
use jni::objects::{JByteArray, JClass, JString};
use jni::sys::{jboolean, jbyteArray, jint, jlong, JNI_FALSE, JNI_TRUE};
use jni::JNIEnv;
use spore::bridge::hub::{Hub, Shared};
use spore::{Envelope, Node};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Mutex;
use std::thread;

/// Everything one node needs, behind a `jlong` handle.
struct Runtime {
    hub: Shared,
    inbox: Mutex<Receiver<Vec<u8>>>, // delivered envelope wires (drained by nativePollDelivery)
}

fn rt<'a>(ptr: jlong) -> &'a Runtime {
    unsafe { &*(ptr as *const Runtime) }
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
    Box::into_raw(Box::new(Runtime { hub, inbox: Mutex::new(rx) })) as jlong
}

/// Destroy a runtime.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeFree(_env: JNIEnv, _class: JClass, ptr: jlong) {
    if ptr != 0 {
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
    let a = rt(ptr).hub.addr();
    env.byte_array_from_slice(&a).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// The node's 32-byte signing seed (persist it; pass back to nativeNew to restore).
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeSeed(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jbyteArray {
    let s = rt(ptr).hub.with_node(|n| n.seed());
    env.byte_array_from_slice(&s).map(|o| o.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Follow a topic so its traffic is delivered to us.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeSubscribe(
    mut env: JNIEnv,
    _class: JClass,
    ptr: jlong,
    topic: JString,
) {
    if let Ok(s) = env.get_string(&topic) {
        let s: String = s.into();
        rt(ptr).hub.with_node(|n| n.subscribe(&s));
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
    let d = env.convert_byte_array(&dest).unwrap_or_default();
    let p = env.convert_byte_array(&payload).unwrap_or_default();
    if d.len() != 8 {
        return;
    }
    let mut addr = [0u8; 8];
    addr.copy_from_slice(&d);
    rt(ptr).hub.send(addr, p);
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
    let r = rt(ptr);
    let (iface, rx) = r.hub.register();
    let hub = r.hub.clone();
    let port = if port > 0 { Some(port as u16) } else { None };
    thread::spawn(move || {
        let _ = spore::bridge::udp::run_primary(hub, iface, rx, port);
    });
}

/// Poll for one delivered envelope's wire bytes, or null if the inbox is empty.
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativePollDelivery(
    env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jbyteArray {
    match rt(ptr).inbox.lock().unwrap().try_recv() {
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
