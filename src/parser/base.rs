pub struct BaseParser<S: BaseSink> {
    buffer: String,
    cursor: usize,
    opened: Option<usize>,
    double: bool,
    single: bool,
    sink: S,
}

impl<S: BaseSink> BaseParser<S> {
    pub fn with_capacity(sink: S, capacity: usize) -> Self {
        Self {
            buffer: String::with_capacity(capacity),
            cursor: 0,
            opened: None,
            double: false,
            single: false,
            sink,
        }
    }

    pub fn new(sink: S) -> Self {
        Self::with_capacity(sink, 0)
    }

    #[inline]
    pub fn feed(&mut self, chunk: &str) -> anyhow::Result<()> {
        self.buffer.push_str(chunk);
        for i in (self.cursor..self.buffer.len() & (usize::MAX - 64 + 1)).step_by(64) {
            let mut mask = {
                use std::simd::{u8x64, SimdPartialEq, ToBitMask};
                let chunk = u8x64::from_slice(&self.buffer.as_bytes()[i..i + 64]);
                let mask0 = chunk.simd_eq(u8x64::splat(b'<')).to_bitmask();
                let mask1 = chunk.simd_eq(u8x64::splat(b'>')).to_bitmask();
                let mask2 = chunk.simd_eq(u8x64::splat(b'"')).to_bitmask();
                let mask3 = chunk.simd_eq(u8x64::splat(0x27)).to_bitmask();
                mask0 | mask1 | mask2 | mask3
            };
            while mask.trailing_zeros() < 64 {
                self.on_match(i + mask.trailing_zeros() as usize)?;
                mask = mask & u64::MAX << 1 << mask.trailing_zeros();
            }
        }
        self.cursor = self.buffer.len() & (usize::MAX - 64 + 1);
        Ok(())
    }

    pub fn stop(mut self) -> anyhow::Result<S> {
        for i in self.cursor..self.buffer.len() {
            self.on_match(i)?;
        }
        Ok(self.sink)
    }

    #[inline]
    fn on_match(&mut self, pos: usize) -> anyhow::Result<()> {
        if let Some(opened) = self.opened {
            match self.buffer.as_bytes()[pos] {
                b'>' => {
                    if !self.double && !self.single {
                        match self.buffer.as_bytes()[opened + 1] {
                            b'!' => {
                                if self.buffer[opened + 2..].starts_with("DOCTYPE ") {
                                    // todo
                                }
                                if self.buffer[opened + 2..].starts_with("--") {
                                    self.sink.on_comment_tag(CommentTag {
                                        value: &self.buffer[opened + 4..pos],
                                    })?;
                                }
                            }
                            b'/' => {
                                if self.buffer.as_bytes()[opened + 2].is_ascii_alphabetic() {
                                    self.sink.on_closing_tag(ClosingTag {
                                        value: &self.buffer[opened + 2..pos],
                                    })?;
                                }
                            }
                            _ => {
                                if self.buffer.as_bytes()[opened + 2].is_ascii_alphabetic() {
                                    let mut value = "";
                                    let mut attrs = Vec::new();
                                    let mut lstws = opened;
                                    let mut equal = false;
                                    let mut close = false;
                                    let mut d = false;
                                    let mut s = false;
                                    for i in opened + 1..pos {
                                        match self.buffer.as_bytes()[i] {
                                            0x09 | 0x0A | 0x0C | 0x0D | 0x20 => {
                                                if !d && !s {
                                                    if i - lstws > 1 {
                                                        let v = &self.buffer[lstws + 1..i];
                                                        if value.is_empty() {
                                                            value = &v;
                                                        } else {
                                                            if equal {
                                                                if let Some((_, rhs)) =
                                                                    attrs.last_mut()
                                                                {
                                                                    *rhs = Some(v);
                                                                }
                                                                equal = false;
                                                            } else {
                                                                attrs.push((v, None))
                                                            }
                                                        }
                                                    }
                                                    lstws = i;
                                                }
                                            }
                                            b'=' => {
                                                if !d && !s {
                                                    if i - lstws > 1 {
                                                        attrs.push((
                                                            &self.buffer[lstws + 1..i],
                                                            None,
                                                        ))
                                                    }
                                                    lstws = i;
                                                    equal = true
                                                }
                                            }
                                            b'/' => {
                                                close = true;
                                                break;
                                            }
                                            b'"' => {
                                                d = !d;
                                            }
                                            0x27 => {
                                                s = !s;
                                            }
                                            _ => {}
                                        }
                                    }
                                    if pos - lstws > 1 {
                                        let v = &self.buffer[lstws + 1..pos];
                                        if value.is_empty() {
                                            value = &v;
                                        } else {
                                            if equal {
                                                if let Some((_, rhs)) = attrs.last_mut() {
                                                    *rhs = Some(v);
                                                }
                                            } else {
                                                attrs.push((v, None))
                                            }
                                        }
                                    }
                                    self.sink.on_opening_tag(OpeningTag { value, attrs })?;
                                    if close {
                                        self.sink.on_closing_tag(ClosingTag { value })?;
                                    }
                                }
                                self.opened = None
                            }
                        }
                        self.opened = None;
                    }
                }
                b'"' => self.double = !self.double,
                0x27 => self.single = !self.single,
                _ => {}
            }
        } else {
            match self.buffer.as_bytes()[pos] {
                b'<' => self.opened = Some(pos),
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct DoctypeTag<'a> {
    pub value: &'a str,
    pub attrs: Vec<(&'a str, Option<&'a str>)>,
}

#[derive(Debug)]
pub struct CommentTag<'a> {
    pub value: &'a str,
}

#[derive(Debug)]
pub struct OpeningTag<'a> {
    pub value: &'a str,
    pub attrs: Vec<(&'a str, Option<&'a str>)>,
}

#[derive(Debug)]
pub struct ClosingTag<'a> {
    pub value: &'a str,
}

#[derive(Debug)]
pub struct Text<'a> {
    pub value: &'a str,
}

pub trait BaseSink {
    fn on_doctype_tag<'a>(&'a mut self, node: DoctypeTag<'a>) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_comment_tag<'a>(&'a mut self, node: CommentTag<'a>) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_opening_tag<'a>(&'a mut self, node: OpeningTag<'a>) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_closing_tag<'a>(&'a mut self, node: ClosingTag<'a>) -> anyhow::Result<()> {
        Ok(())
    }

    fn on_text<'a>(&'a mut self, node: Text<'a>) -> anyhow::Result<()> {
        Ok(())
    }
}
