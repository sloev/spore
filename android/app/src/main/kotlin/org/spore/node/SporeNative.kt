package org.spore.node

/**
 * Thin Kotlin wrapper over the Rust JNI layer (`android/jni`, lib `spore_jni`).
 * Every `external fun` matches a `Java_org_spore_node_SporeNative_*` in Rust.
 */
object SporeNative {
    init {
        System.loadLibrary("spore_jni")
    }

    /** Create a runtime. `seed` = null for a fresh identity, or 32 bytes to restore. */
    external fun nativeNew(seed: ByteArray?): Long

    /** Destroy a runtime. */
    external fun nativeFree(ptr: Long)

    /** The node's 8-byte address. */
    external fun nativeAddr(ptr: Long): ByteArray

    /** The node's 32-byte signing seed (persist it, pass back to nativeNew). */
    external fun nativeSeed(ptr: Long): ByteArray

    /** Follow a topic so its traffic is delivered to us. */
    external fun nativeSubscribe(ptr: Long, topic: String)

    /** Originate a signed message to `dest` (8 bytes; all-zero = public). */
    external fun nativeSend(ptr: Long, dest: ByteArray, payload: ByteArray)

    /** Start the primary-subnet UDP broadcast bridge (port <= 0 = default). */
    external fun nativeStartUdp(ptr: Long, port: Int)

    /** Start a TCP bridge (empty target = listen; else "host:port"). */
    external fun nativeStartTcp(ptr: Long, target: String)

    /** Register a Kotlin-driven bridge interface; returns its iface id. */
    external fun nativeRegisterIface(ptr: Long): Int

    /** Poll one outbound frame the node wants sent on `iface`, or null. */
    external fun nativePollForward(ptr: Long, iface: Int): ByteArray?

    /** Feed an inbound frame from a Kotlin-driven bridge into the node. */
    external fun nativePushRx(ptr: Long, iface: Int, frame: ByteArray)

    /** Poll one delivered envelope's wire bytes, or null if the inbox is empty. */
    external fun nativePollDelivery(ptr: Long): ByteArray?

    /** The payload of an envelope wire. */
    external fun nativeEnvPayload(wire: ByteArray): ByteArray?

    /** Whether an envelope wire carries a valid signature. */
    external fun nativeEnvVerify(wire: ByteArray): Boolean

    /** The sender's 8-byte address, or null if the envelope is unsigned. */
    external fun nativeEnvSrc(wire: ByteArray): ByteArray?

    /** The envelope's 8-byte destination (topic addr for feed posts). */
    external fun nativeEnvDest(wire: ByteArray): ByteArray?

    /** The 8-byte topic address for a topic name. */
    external fun nativeTopicAddr(topic: String): ByteArray?

    /** Send and return the number of wire fragments the payload became. */
    external fun nativeSendCounted(ptr: Long, dest: ByteArray, payload: ByteArray): Int

    /** Modulate one frame to 48 kHz mono f32 PCM (audio modem TX). */
    external fun nativeAudioModulate(payload: ByteArray): FloatArray?

    /** Feed captured mic PCM into the streaming demodulator (audio modem RX). */
    external fun nativeAudioDemodPush(ptr: Long, samples: FloatArray)

    /** Pop one frame the demodulator completed, or null. */
    external fun nativeAudioDemodPop(ptr: Long): ByteArray?

    /** Wrap an envelope as a Meshtastic MeshPacket (portnum 256, broadcast). */
    external fun nativeMeshtasticWrap(envWire: ByteArray, fromNode: Int, packetId: Int): ByteArray?

    /** Unwrap a MeshPacket; the SPORE envelope if it rides portnum 256, else null. */
    external fun nativeMeshtasticUnwrap(frame: ByteArray): ByteArray?

    /** Start the plain limited-broadcast UDP bridge (for Wi-Fi Direct groups). */
    external fun nativeStartUdpLimited(ptr: Long, port: Int)
}
