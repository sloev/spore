package org.spore.node

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow

/**
 * Local address book: petname ⇄ 8-byte address (as hex). Never on the wire — this
 * is purely how *you* label peers. Backed by SharedPreferences.
 */
object Petnames {
    private lateinit var prefs: android.content.SharedPreferences
    val map = MutableStateFlow<Map<String, String>>(emptyMap()) // addrHex -> petname

    fun init(ctx: Context) {
        prefs = ctx.getSharedPreferences("petnames", Context.MODE_PRIVATE)
        @Suppress("UNCHECKED_CAST")
        map.value = (prefs.all as Map<String, String?>).filterValues { it != null }
            .mapValues { it.value!! }
    }

    fun set(addrHex: String, name: String) {
        if (name.isBlank()) prefs.edit().remove(addrHex).apply()
        else prefs.edit().putString(addrHex, name.trim()).apply()
        map.value = map.value.toMutableMap().also {
            if (name.isBlank()) it.remove(addrHex) else it[addrHex] = name.trim()
        }
    }

    /** A display label for a peer: its petname if set, else a short hex. */
    fun label(addrHex: String): String =
        map.value[addrHex] ?: if (addrHex == PUBLIC) "everyone" else "…${addrHex.takeLast(6)}"

    const val PUBLIC = "public"
}
