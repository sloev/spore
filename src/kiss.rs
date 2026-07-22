const FEND: u8 = 0xC0;
const FESC: u8 = 0xDB;
const TFEND: u8 = 0xDC;
const TFESC: u8 = 0xDD;

pub fn encode(frame: &[u8]) -> Vec<u8> {
    let mut o = vec![FEND, 0x00]; // FEND + command byte
    for &b in frame {
        match b {
            FEND => o.extend_from_slice(&[FESC, TFEND]),
            FESC => o.extend_from_slice(&[FESC, TFESC]),
            _ => o.push(b),
        }
    }
    o.push(FEND);
    o
}

/// Extract complete frames from a stream buffer (command byte stripped).
pub fn decode(stream: &[u8]) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut cur = Vec::new();
    let mut in_frame = false;
    let mut got_cmd = false;
    let mut esc = false;
    for &b in stream {
        if b == FEND {
            if in_frame && !cur.is_empty() {
                frames.push(std::mem::take(&mut cur));
            }
            in_frame = true;
            got_cmd = false;
            esc = false;
            cur.clear();
            continue;
        }
        if !in_frame {
            continue;
        }
        if !got_cmd {
            got_cmd = true; // skip the command byte
            continue;
        }
        if esc {
            cur.push(if b == TFEND {
                FEND
            } else if b == TFESC {
                FESC
            } else {
                b
            });
            esc = false;
        } else if b == FESC {
            esc = true;
        } else {
            cur.push(b);
        }
    }
    frames
}
