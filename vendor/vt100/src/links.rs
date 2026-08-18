/// Interned OSC 8 hyperlink targets, addressed by the `u16` id stored on each
/// [`crate::Cell`].
///
/// Ids are 1-based indices into `urls`, so `0` is free to mean "no link" and
/// equal targets automatically share an id — which is why the OSC 8 `id=`
/// parameter can be ignored: two runs pointing at the same URI already unify.
#[derive(Clone, Debug, Default)]
pub struct Links {
    urls: Vec<String>,
    index: std::collections::HashMap<String, u16>,
    bytes: usize,
}

/// Longest URI accepted. Anything beyond this is treated as no link.
const MAX_URI_LEN: usize = 2048;

/// Total interned URI bytes tolerated before the table is recycled.
const MAX_TABLE_BYTES: usize = 1024 * 1024;

/// Outcome of interning a URI.
pub enum Intern {
    /// Use this id.
    Id(u16),
    /// The table is full. The caller must clear every cell's link id before
    /// calling [`Links::recycle`] and interning again.
    Exhausted,
}

impl Links {
    /// Interns `uri`, returning the id to stamp onto subsequent cells.
    ///
    /// Returns `Id(0)` for a URI that is empty or fails validation, which
    /// closes any open link.
    pub fn intern(&mut self, uri: &[u8]) -> Intern {
        let Some(uri) = validate(uri) else {
            return Intern::Id(0);
        };
        if let Some(&id) = self.index.get(uri) {
            return Intern::Id(id);
        }
        if self.urls.len() >= usize::from(u16::MAX)
            || self.bytes + uri.len() > MAX_TABLE_BYTES
        {
            return Intern::Exhausted;
        }
        self.urls.push(uri.to_string());
        self.bytes += uri.len();
        // ids are 1-based, so the id is the length after pushing
        let id = u16::try_from(self.urls.len()).unwrap();
        self.index.insert(uri.to_string(), id);
        Intern::Id(id)
    }

    /// Drops every interned target. Only safe once no cell references an id.
    pub fn recycle(&mut self) {
        self.urls.clear();
        self.index.clear();
        self.bytes = 0;
    }

    pub fn get(&self, id: u16) -> Option<&str> {
        if id == 0 {
            return None;
        }
        self.urls.get(usize::from(id) - 1).map(String::as_str)
    }
}

/// Rejects anything that is not a plausible URI. The bytes come from arbitrary
/// program output and end up in a URL the user can click, so they are checked
/// here at the boundary rather than trusted downstream.
fn validate(uri: &[u8]) -> Option<&str> {
    if uri.is_empty() || uri.len() > MAX_URI_LEN {
        return None;
    }
    let uri = std::str::from_utf8(uri).ok()?;
    if uri.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    Some(uri)
}
