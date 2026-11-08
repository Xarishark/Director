//! Android TV Remote v2 wire subset. See docs/TVS.md for the protocol scope and references.
//! Deliberately excludes generic keys, text injection and menu sequences.
use std::io::{Read, Write};
pub const LIMIT: usize = 65536;
pub fn varint(mut n: u64, out: &mut Vec<u8>) {
    while n >= 128 {
        out.push((n as u8 & 127) | 128);
        n >>= 7;
    }
    out.push(n as u8);
}
pub fn integer(field: u32, n: u64) -> Vec<u8> {
    let mut v = vec![];
    varint(u64::from(field) << 3, &mut v);
    varint(n, &mut v);
    v
}
pub fn bytes(field: u32, b: &[u8]) -> Vec<u8> {
    let mut v = vec![];
    varint((u64::from(field) << 3) | 2, &mut v);
    varint(b.len() as u64, &mut v);
    v.extend(b);
    v
}
fn number(data: &[u8], pos: &mut usize) -> Result<u64, String> {
    let mut n = 0;
    for shift in (0..70).step_by(7) {
        let b = *data.get(*pos).ok_or("Truncated Android TV message.")?;
        *pos += 1;
        if shift == 63 && b > 1 {
            return Err("Invalid Android TV integer.".into());
        }
        n |= u64::from(b & 127) << shift;
        if b & 128 == 0 {
            return Ok(n);
        }
    }
    Err("Invalid Android TV integer.".into())
}
#[derive(Debug, PartialEq)]
pub enum Value<'a> {
    Int(u64),
    Bytes(&'a [u8]),
}
pub fn fields(data: &[u8]) -> Result<Vec<(u32, Value<'_>)>, String> {
    if data.len() > LIMIT {
        return Err("Android TV message is too large.".into());
    }
    let mut pos = 0;
    let mut out = vec![];
    while pos < data.len() {
        let tag = number(data, &mut pos)?;
        let field = u32::try_from(tag >> 3).map_err(|_| "Invalid field.")?;
        if field == 0 || field > 0x1fff_ffff {
            return Err("Invalid field.".into());
        }
        let value = match tag & 7 {
            0 => Value::Int(number(data, &mut pos)?),
            2 => {
                let len =
                    usize::try_from(number(data, &mut pos)?).map_err(|_| "Invalid length.")?;
                let end = pos
                    .checked_add(len)
                    .filter(|end| *end <= data.len())
                    .ok_or("Truncated Android TV field.")?;
                let b = &data[pos..end];
                pos = end;
                Value::Bytes(b)
            }
            1 | 5 => {
                let end = pos + if tag & 7 == 1 { 8 } else { 4 };
                if end > data.len() {
                    return Err("Truncated Android TV field.".into());
                }
                let b = &data[pos..end];
                pos = end;
                Value::Bytes(b)
            }
            _ => return Err("Unsupported Android TV wire type.".into()),
        };
        out.push((field, value));
        if out.len() > 1024 {
            return Err("Too many Android TV fields.".into());
        }
    }
    Ok(out)
}
pub fn int(data: &[u8], field: u32) -> Result<Option<u64>, String> {
    Ok(fields(data)?.into_iter().find_map(|(f, v)| match v {
        Value::Int(n) if f == field => Some(n),
        _ => None,
    }))
}
pub fn nested(data: &[u8], field: u32) -> Result<Option<&[u8]>, String> {
    Ok(fields(data)?.into_iter().find_map(|(f, v)| match v {
        Value::Bytes(b) if f == field => Some(b),
        _ => None,
    }))
}
pub fn read(r: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut prefix = vec![];
    for _ in 0..4 {
        let mut b = [0];
        r.read_exact(&mut b)
            .map_err(|_| "Android TV did not respond in time, or disconnected.")?;
        prefix.push(b[0]);
        if b[0] & 128 == 0 {
            let n = number(&prefix, &mut 0)? as usize;
            if n == 0 || n > LIMIT {
                return Err("Invalid Android TV frame length.".into());
            }
            let mut v = vec![0; n];
            r.read_exact(&mut v)
                .map_err(|_| "Android TV sent an incomplete frame.")?;
            return Ok(v);
        }
    }
    Err("Invalid Android TV frame prefix.".into())
}
pub fn write(w: &mut impl Write, data: &[u8]) -> Result<(), String> {
    if data.is_empty() || data.len() > LIMIT {
        return Err("Invalid Android TV frame size.".into());
    }
    let mut v = vec![];
    varint(data.len() as u64, &mut v);
    v.extend(data);
    w.write_all(&v)
        .and_then(|_| w.flush())
        .map_err(|_| "The Android TV command could not be sent.".into())
}
pub fn pairing(field: u32, payload: &[u8]) -> Vec<u8> {
    [integer(1, 2), integer(2, 200), bytes(field, payload)].concat()
}
pub fn pair_reply(data: &[u8], expected: u32) -> Result<(), String> {
    if int(data, 1)? != Some(2) || int(data, 2)? != Some(200) || nested(data, expected)?.is_none() {
        return Err(
            "The TV rejected pairing or returned an unexpected response. Start pairing again."
                .into(),
        );
    }
    Ok(())
}
pub fn hdmi(port: u8) -> Result<Vec<u8>, String> {
    if !(1..=4).contains(&port) {
        return Err("Choose HDMI 1–4.".into());
    }
    Ok(key(242 + u64::from(port)))
}
pub fn key(code: u64) -> Vec<u8> {
    bytes(10, &[integer(1, code), integer(2, 3)].concat())
}
pub fn code(text: &str) -> Result<[u8; 3], String> {
    let t = text.trim();
    if t.len() != 6 || !t.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Enter the six hexadecimal characters shown on the TV (0–9, A–F).".into());
    }
    let mut result = [0; 3];
    for (i, b) in result.iter_mut().enumerate() {
        *b = u8::from_str_radix(&t[i * 2..i * 2 + 2], 16).map_err(|_| "Invalid TV code.")?;
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framed_messages_handle_fragmentation_and_reject_bad_lengths() {
        let data = pairing(11, &[]);
        let mut v = vec![];
        write(&mut v, &data).unwrap();
        assert_eq!(read(&mut &v[..]).unwrap(), data);
        assert!(read(&mut &[0xff, 0xff, 0xff, 0x7f][..]).is_err());
        assert!(fields(&[0x0a, 0xff]).is_err());
        assert!(fields(&[0]).is_err());
    }
    #[test]
    fn pairing_requires_success_and_expected_phase() {
        assert!(pair_reply(&pairing(11, &[]), 11).is_ok());
        assert!(pair_reply(&pairing(11, &[]), 31).is_err());
        assert!(
            pair_reply(
                &[integer(1, 2), integer(2, 402), bytes(41, &[])].concat(),
                41
            )
            .is_err()
        );
        assert_eq!(code("a1B2c3").unwrap(), [0xa1, 0xb2, 0xc3]);
        assert!(code("12345G").is_err());
    }
    #[test]
    fn hdmi_is_one_short_direct_command() {
        for port in 1..=4 {
            let v = hdmi(port).unwrap();
            let msg = nested(&v, 10).unwrap().unwrap();
            assert_eq!(int(msg, 1).unwrap(), Some(242 + u64::from(port)));
            assert_eq!(int(msg, 2).unwrap(), Some(3));
            assert_eq!(fields(&v).unwrap().len(), 1)
        }
        assert!(hdmi(0).is_err());
        assert!(hdmi(5).is_err());
    }
}
