package org.spore.node

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Bring the node up in a foreground service, then render the UI.
        ContextCompat.startForegroundService(this, Intent(this, NodeService::class.java))
        setContent { App() }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun App() {
    MaterialTheme {
        val log by NodeController.log.collectAsState()
        val addr by NodeController.address.collectAsState()
        var msg by remember { mutableStateOf("the dam holds") }

        Scaffold(topBar = { TopAppBar(title = { Text("🍄 SPORE — web node") }) }) { pad ->
            Column(
                Modifier
                    .padding(pad)
                    .padding(16.dp)
                    .fillMaxSize()
            ) {
                Text("addr $addr", style = MaterialTheme.typography.bodySmall)
                Spacer(Modifier.height(8.dp))
                Column(
                    Modifier
                        .weight(1f)
                        .verticalScroll(rememberScrollState())
                ) {
                    log.forEach { Text(it, style = MaterialTheme.typography.bodyMedium) }
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    OutlinedTextField(
                        value = msg,
                        onValueChange = { msg = it },
                        modifier = Modifier.weight(1f)
                    )
                    Spacer(Modifier.width(8.dp))
                    Button(onClick = { NodeController.send(msg) }) { Text("Send") }
                }
            }
        }
    }
}
