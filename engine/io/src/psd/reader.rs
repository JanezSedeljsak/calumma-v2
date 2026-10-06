/// A checked cursor over the file bytes. Every read can fail — this is parsing a file nobody
/// asked the engine to trust — so nothing here indexes or slices without a bounds check first;
/// a truncated or hostile PSD is refused with `None` at the point it stops making sense, never
/// panics.
pub(super) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub(super) fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub(super) fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    pub(super) fn skip(&mut self, n: usize) -> Option<()> {
        self.take(n).map(|_| ())
    }

    pub(super) fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    pub(super) fn i8(&mut self) -> Option<i8> {
        self.u8().map(|b| b as i8)
    }

    pub(super) fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_be_bytes([b[0], b[1]]))
    }

    pub(super) fn i16(&mut self) -> Option<i16> {
        self.u16().map(|v| v as i16)
    }

    pub(super) fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(super) fn i32(&mut self) -> Option<i32> {
        self.u32().map(|v| v as i32)
    }
}

/// PackBits, the only compression PSD channel data uses besides raw. A control byte `n`: `n
/// >= 0` copies the next `n + 1` bytes literally; `n < 0` (and not the no-op `-128`) repeats
/// the following single byte `1 - n` times. Two's-complement `i8` reads that straight off the
/// wire, which is why `Reader::i8` exists.
///
/// PSD additionally prefixes each scanline's compressed bytes with its own byte count (read by
/// the caller, one `u16`/`u32` per row) so a reader can skip a row without decompressing it.
/// This decoder does not need that shortcut — it always wants the whole channel — so it runs
/// PackBits as one continuous stream across every row's bytes back to back rather than
/// resetting at each row boundary, which decodes identically since no control code ever
/// straddles what would have been a row's end (Photoshop never emits one that does).
pub(super) fn unpack_bits(reader: &mut Reader, out_len: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(out_len);
    while out.len() < out_len {
        let n = reader.i8()?;
        if n >= 0 {
            let count = n as usize + 1;
            out.extend_from_slice(reader.take(count)?);
        } else if n != -128 {
            let count = 1 - n as isize;
            let byte = reader.u8()?;
            out.resize(out.len() + count as usize, byte);
        }
    }
    out.truncate(out_len);
    Some(out)
}
