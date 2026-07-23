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

    /** Poll one delivered envelope's wire bytes, or null if the inbox is empty. */
    external fun nativePollDelivery(ptr: Long): ByteArray?

    /** The payload of an envelope wire. */
    external fun nativeEnvPayload(wire: ByteArray): ByteArray?

    /** Whether an envelope wire carries a valid signature. */
    external fun nativeEnvVerify(wire: ByteArray): Boolean
}
