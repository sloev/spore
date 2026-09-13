package org.spore.node

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow

/**
 * Local address book: petname ⇄ 8-byte address (as hex). Never on the wire — this
 * is purely how *you* label peers.
 *
 * **This is now a shim.** The store is `src/communicator/contact.rs`, reached
 * through [Comm], so this app and the browser node run the same address book
 * rather than two implementations that agree by coincidence. What is left here
 * is the `StateFlow` Compose recomposes on, and a one-time migration off
 * SharedPreferences.
 *
 * The rule it enforced survives the move, because it moved with it: a label is
 * only ever what *this user typed*. An announced name is a claim — anyone may
 * announce any name — so it is offered as a default when the user is choosing a
 * petname and is never written in as though they had chosen it.
 *
 * The public surface is unchanged, so no screen changed.
 */
object Petnames {
    private var ptr: Long = 0L
    private var prefs: android.content.SharedPreferences? = null

    val map = MutableStateFlow<Map<String, String>>(emptyMap()) // addrHex -> petname

    /**
     * Adopt the core's address book, migrating the old SharedPreferences one on
     * the first run that has both.
     *
     * Must be called **after** `nativeNew`: the store lives behind the node's
     * handle, because it has exactly the node's lifetime.
     */
    fun init(ctx: Context, nodePtr: Long) {
        ptr = nodePtr
        val p = ctx.getSharedPreferences("petnames", Context.MODE_PRIVATE)
        prefs = p

        // The core's blob is the source of truth once it exists.
        val stored = p.getString(BLOB_KEY, null)
        if (stored != null) {
            val blob = runCatching { android.util.Base64.decode(stored, android.util.Base64.NO_WRAP) }.getOrNull()
            // A blob that will not decode is left exactly where it is rather than
            // cleared. These are names the user typed and nothing else holds a
            // copy; a later build may read what this one cannot, and wiping them
            // because one parse failed would be the worse failure.
            if (blob != null) Comm.load(ptr, blob)
        }

        migrateFromPrefs(p)
        map.value = Comm.contactLabels(ptr)
    }

    /**
     * Copy petnames written by the pre-M10 build into the core, once.
     *
     * **The old entries are kept, not deleted.** They cost a few hundred bytes
     * and they are the only copy if anything here is wrong — including a user who
     * installs an older build tomorrow. The flag is what stops this re-running,
     * not the absence of the data.
     *
     * Existing labels win: if the core already has one for an address, the user
     * has set it since migrating and the old value is stale.
     */
    private fun migrateFromPrefs(p: android.content.SharedPreferences) {
        if (p.getBoolean(MIGRATED_KEY, false)) return
        val existing = Comm.contactLabels(ptr)
        var moved = 0
        for ((key, value) in p.all) {
            if (key == BLOB_KEY || key == MIGRATED_KEY) continue
            val name = (value as? String)?.trim() ?: continue
            if (name.isEmpty() || existing.containsKey(key)) continue
            Comm.contactSetLabel(ptr, key, name)
            moved++
        }
        if (moved > 0) persist()
        p.edit().putBoolean(MIGRATED_KEY, true).apply()
    }

    fun set(addrHex: String, name: String) {
        // An empty name clears the label and keeps the row, because the row also
        // carries follow and block state that renaming must not throw away.
        Comm.contactSetLabel(ptr, addrHex, name.trim())
        persist()
        map.value = Comm.contactLabels(ptr)
    }

    /**
     * Write the core's blob back to the host.
     *
     * Petnames change rarely and are small, so this is eager — unlike drafts,
     * which are written on every keystroke and reach storage on an ordinary save.
     */
    private fun persist() {
        val blob = Comm.save(ptr) ?: return
        prefs?.edit()
            ?.putString(BLOB_KEY, android.util.Base64.encodeToString(blob, android.util.Base64.NO_WRAP))
            ?.apply()
    }

    /** A display label for a peer: its petname if set, else a short hex. */
    fun label(addrHex: String): String =
        map.value[addrHex] ?: if (addrHex == PUBLIC) "everyone" else "…${addrHex.takeLast(6)}"

    const val PUBLIC = "public"
    private const val BLOB_KEY = "__comm_blob"
    private const val MIGRATED_KEY = "__migrated_to_comm"
}
