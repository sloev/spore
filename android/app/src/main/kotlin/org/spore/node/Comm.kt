package org.spore.node

/**
 * The Kotlin half of the **application-layer command ABI** (M10-C).
 *
 * Conversations, contacts, topics and drafts live in the Rust core. This file is
 * a codec and nothing else: it builds command bytes, hands them to
 * `SporeNative.nativeCommCall`, and reads the response back. It holds no
 * application state, because the point is that there is only one copy of that
 * state and it is not here.
 *
 * **Why bytes rather than a JNI method per operation.** `android/jni` already
 * exports sixty-five functions, `wasm.rs` thirty-four and `ffi.rs` twenty —
 * three overlapping subsets of one kernel — and the communicator was then
 * written a fourth time in Kotlin. Commands are bytes, so an application feature
 * adds behaviour without adding a symbol, and this app gets exactly what the
 * browser node gets.
 *
 * Mirrors `web/app/communicator.mjs`. The two must agree on the tag numbers,
 * which is why they are written out rather than derived from an order.
 */
object Comm {
    const val OK = 0

    const val CMD_CONTACT_SET_LABEL = 0x10
    const val CMD_CONTACT_ROWS = 0x14
    const val CMD_SAVE = 0x30
    const val CMD_LOAD = 0x31

    const val VIEW_CONTACTS = 0
    const val VIEW_SEEN = 1

    /** Builds a command. Every length prefix is a big-endian u32. */
    class W(tag: Int) {
        private val b = java.io.ByteArrayOutputStream()

        init {
            b.write(tag)
        }

        fun u8(v: Int): W { b.write(v and 0xff); return this }
        fun bool(v: Boolean): W = u8(if (v) 1 else 0)

        fun u32(v: Int): W {
            b.write((v ushr 24) and 0xff); b.write((v ushr 16) and 0xff)
            b.write((v ushr 8) and 0xff); b.write(v and 0xff)
            return this
        }

        fun bytes(a: ByteArray): W { b.write(a, 0, a.size); return this }
        fun str(s: String): W { val e = s.toByteArray(Charsets.UTF_8); u32(e.size); return bytes(e) }
        fun hex(h: String): W = bytes(unhex(h))
        fun out(): ByteArray = b.toByteArray()
    }

    /** Reads a response body (the OK tag already stripped). */
    class R(private val b: ByteArray) {
        var i = 0; private set

        fun u8(): Int = b[i++].toInt() and 0xff
        fun bool(): Boolean = u8() == 1

        fun u32(): Int {
            val v = ((b[i].toInt() and 0xff) shl 24) or ((b[i + 1].toInt() and 0xff) shl 16) or
                ((b[i + 2].toInt() and 0xff) shl 8) or (b[i + 3].toInt() and 0xff)
            i += 4
            return v
        }

        fun bytes(n: Int): ByteArray { val v = b.copyOfRange(i, i + n); i += n; return v }
        fun hex(n: Int): String = toHex(bytes(n))
        fun str(): String = String(bytes(u32()), Charsets.UTF_8)
        fun rest(): ByteArray = b.copyOfRange(i, b.size)
        fun atEnd(): Boolean = i == b.size
    }

    /**
     * Run one command. Returns null when the handle is dead or the core refused
     * the command.
     *
     * A refusal is a bug in *this file* — the core answers with an error byte
     * rather than crashing precisely so a drifted caller gets an answer — so it
     * is returned as null rather than thrown, and every caller here treats it as
     * "no data" and carries on.
     */
    fun call(ptr: Long, w: W): R? {
        if (ptr == 0L) return null
        val out = SporeNative.nativeCommCall(ptr, w.out()) ?: return null
        if (out.isEmpty() || out[0].toInt() != OK) return null
        return R(out.copyOfRange(1, out.size))
    }

    // -- the operations this app currently needs -----------------------------

    fun contactSetLabel(ptr: Long, addrHex: String, label: String) {
        call(ptr, W(CMD_CONTACT_SET_LABEL).hex(addrHex).str(label))
    }

    /** `addrHex -> label`, for every address the user has labelled. */
    fun contactLabels(ptr: Long): Map<String, String> {
        val r = call(ptr, W(CMD_CONTACT_ROWS).u8(VIEW_CONTACTS).str("").u32(0)) ?: return emptyMap()
        val out = LinkedHashMap<String, String>()
        val n = r.u32()
        repeat(n) {
            val addr = r.hex(8)
            r.bool(); r.bool(); r.bool(); r.bool(); r.bool(); r.bool() // flags
            r.u32(); r.bool()                                          // age, hasAge
            val label = r.str()
            r.str(); r.str()                                           // claimed, name
            if (label.isNotEmpty()) out[addr] = label
        }
        return out
    }

    /** Everything, as one blob for the host to keep. */
    fun save(ptr: Long): ByteArray? = call(ptr, W(CMD_SAVE))?.rest()

    /**
     * Replace everything from a blob. Returns whether it was accepted.
     *
     * A refused blob leaves every store exactly as it was — the core decodes all
     * of them before committing any — so the caller carries on with what it has
     * and leaves the stored bytes alone.
     */
    fun load(ptr: Long, blob: ByteArray): Boolean =
        blob.isNotEmpty() && call(ptr, W(CMD_LOAD).bytes(blob)) != null

    // -- hex, which both sides of this boundary speak ------------------------

    fun toHex(b: ByteArray): String {
        val sb = StringBuilder(b.size * 2)
        for (x in b) sb.append("%02x".format(x))
        return sb.toString()
    }

    fun unhex(s: String): ByteArray {
        val out = ByteArray(s.length / 2)
        for (i in out.indices) out[i] = ((s[i * 2].digitToInt(16) shl 4) or s[i * 2 + 1].digitToInt(16)).toByte()
        return out
    }
}
