//! Text utilities.

#[derive(Debug)]
pub(crate) struct StrBuffer {
    data: String,
    offset: Vec<u32>,
}

impl StrBuffer {
    pub(crate) fn from_iter<I, S>(iter: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let iter = iter.into_iter();
        let (lower, _) = iter.size_hint();

        let mut data = String::with_capacity(lower * 12);
        let mut offset = Vec::with_capacity(lower);

        for s in iter {
            let s = s.as_ref();
            offset.push(data.len() as u32);
            data.push_str(s);
        }
        Self { data, offset }
    }

    pub(crate) const fn len(&self) -> usize {
        self.offset.len()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &str> {
        (0..self.len()).map(move |i| self.get(i))
    }

    pub(crate) fn get(&self, index: usize) -> &str {
        let start = self.offset[index] as usize;
        let end = self
            .offset
            .get(index + 1)
            .map(|&v| v as usize)
            .unwrap_or(self.data.len());
        &self.data[start..end]
    }
}
