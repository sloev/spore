#!/usr/bin/env python3
"""RNS companion for SPORE's `reticulum` bridge.

The SPORE daemon (`spore reticulum`) speaks **KISS-framed envelopes** on its
stdin/stdout; this companion moves them over a real **Reticulum** network as the
data of packets sent to a shared **PLAIN** destination — a broadcast bus that
every SPORE-over-RNS node listens on. Reticulum supplies the transport, path-
finding, and every interface it is configured with (LoRa, TCP, I2P, packet
radio); SPORE's envelope carries its own Ed25519 signature and optional
encryption, so nothing security-critical lives here.

The bus is the PLAIN destination ``spore.mesh`` (app name + aspect). Because a
PLAIN destination's hash is derived only from its name and aspects, every node
computes the same destination and hears every packet on it — exactly SPORE's
flood model. The daemon clamps its MTU so each envelope fits one RNS packet;
larger objects fountain-fragment above the bridge like on any medium.

Requires the Reticulum library:  ``pip install rns``

Wiring (bidirectional — two fifos connect the daemon and this companion)::

    mkfifo /tmp/spore_up /tmp/spore_down
    python3 tools/reticulum_companion.py < /tmp/spore_up > /tmp/spore_down &
    spore reticulum  < /tmp/spore_down > /tmp/spore_up

An optional argument is a Reticulum config path (defaults to RNS's own default).
"""
import sys
import threading

try:
    import RNS
except ImportError:
    sys.stderr.write(
        "reticulum_companion: the Reticulum library is required — `pip install rns`\n"
    )
    sys.exit(1)

APP_NAME = "spore"
ASPECT = "mesh"  # the shared bus; all SPORE-over-RNS nodes use the same

# --- KISS framing (matches src/kiss.rs and the daemon's stream bridges) --------
FEND, FESC, TFEND, TFESC = 0xC0, 0xDB, 0xDC, 0xDD


def kiss_encode(data: bytes) -> bytes:
    out = bytearray([FEND, 0x00])  # FEND + command byte (data, port 0)
    for b in data:
        if b == FEND:
            out += bytes([FESC, TFEND])
        elif b == FESC:
            out += bytes([FESC, TFESC])
        else:
            out.append(b)
    out.append(FEND)
    return bytes(out)


class KissDecoder:
    """Streaming KISS de-framer — yields whole frames across chunked reads."""

    def __init__(self):
        self.cur = bytearray()
        self.in_frame = False
        self.got_cmd = False
        self.esc = False

    def push(self, chunk: bytes):
        frames = []
        for b in chunk:
            if b == FEND:
                if self.in_frame and self.cur:
                    frames.append(bytes(self.cur))
                self.in_frame, self.got_cmd, self.esc = True, False, False
                self.cur = bytearray()
            elif not self.in_frame:
                continue
            elif not self.got_cmd:
                self.got_cmd = True  # drop the KISS command byte
            elif self.esc:
                self.cur.append(FEND if b == TFEND else FESC if b == TFESC else b)
                self.esc = False
            elif b == FESC:
                self.esc = True
            else:
                self.cur.append(b)
        return frames


def main():
    config_path = sys.argv[1] if len(sys.argv) > 1 else None
    RNS.Reticulum(config_path)

    # IN destination: receive packets on the shared bus.
    rx_dest = RNS.Destination(
        None, RNS.Destination.IN, RNS.Destination.PLAIN, APP_NAME, ASPECT
    )
    # OUT destination: same name/aspect ⇒ same hash ⇒ the same bus.
    tx_dest = RNS.Destination(
        None, RNS.Destination.OUT, RNS.Destination.PLAIN, APP_NAME, ASPECT
    )

    out = sys.stdout.buffer
    out_lock = threading.Lock()

    def on_packet(data, _packet):
        # An envelope arrived over RNS → hand it to the daemon, KISS-framed.
        with out_lock:
            out.write(kiss_encode(data))
            out.flush()

    rx_dest.set_packet_callback(on_packet)
    RNS.log("SPORE ⇄ RNS bus up on PLAIN destination "
            + RNS.prettyhexrep(rx_dest.hash), RNS.LOG_INFO)

    # Main thread: KISS frames from the daemon → RNS packets on the bus.
    dec = KissDecoder()
    stdin = sys.stdin.buffer
    while True:
        chunk = stdin.read(4096)
        if not chunk:
            break  # daemon closed the pipe
        for frame in dec.push(chunk):
            RNS.Packet(tx_dest, frame).send()


if __name__ == "__main__":
    main()
