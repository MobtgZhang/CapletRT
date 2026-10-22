//! Path manipulation utilities
//!
//! CapletRT VFS paths are absolute, slash-separated, and have a
//! maximum length of 128 bytes (including the trailing NUL). They
//! must start with a single `/`. Multiple consecutive slashes are
//! collapsed.

use alloc::string::String;

/// Maximum path length (including the trailing NUL).
pub const MAX_PATH_LEN: usize = 128;

/// Path utility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    inner: String,
}

impl Path {
    /// Parse a path string into a normalised [`Path`].
    ///
    /// # Errors
    /// Returns [`PathError::TooLong`] if the path exceeds 128
    /// bytes after normalisation.
    pub fn parse(s: &str) -> Result<Self, PathError> {
        if s.is_empty() || !s.starts_with('/') {
            return Err(PathError::Invalid);
        }
        let mut out = String::with_capacity(s.len());
        out.push('/');
        let mut prev_slash = true;
        for c in s.chars().skip(1) {
            if c == '/' {
                if !prev_slash {
                    out.push('/');
                }
                prev_slash = true;
            } else {
                out.push(c);
                prev_slash = false;
            }
        }
        while out.len() > 1 && out.ends_with('/') {
            out.pop();
        }
        if out.len() + 1 > MAX_PATH_LEN {
            return Err(PathError::TooLong);
        }
        Ok(Self { inner: out })
    }

    /// Borrow as a `&str`.
    pub fn as_str(&self) -> &str {
        &self.inner
    }

    /// Split the path into mount point + sub-path. Returns
    /// `(component, remainder)`. The component is the first
    /// path segment, and the remainder is the path (with leading
    /// `/`) after that segment.
    pub fn split_mount(&self) -> Option<(&str, &str)> {
        let rest = &self.inner[1..];
        let slash = rest.find('/');
        match slash {
            None => Some((rest, "")),
            Some(n) => {
                let mount = &rest[..n];
                let sub = &rest[n..];
                Some((mount, sub))
            }
        }
    }

    /// Walk the path segment by segment, calling `cb` on each.
    pub fn segments(&self) -> PathSegments<'_> {
        PathSegments {
            path: &self.inner[1..],
            at_slash: false,
            done: false,
        }
    }
}

/// Errors returned by [`Path::parse`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathError {
    /// Empty path or missing leading slash.
    Invalid,
    /// Path exceeds [`MAX_PATH_LEN`].
    TooLong,
}

/// Iterator over path segments.
pub struct PathSegments<'a> {
    path: &'a str,
    /// Whether the next character is expected to be (or be the
    /// first of) a slash.
    at_slash: bool,
    /// Whole path already emitted.
    done: bool,
}

impl<'a> Iterator for PathSegments<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.done || self.path.is_empty() {
            return None;
        }
        // Skip a leading slash.
        if self.at_slash {
            self.at_slash = false;
            if self.path.starts_with('/') {
                self.path = &self.path[1..];
            }
            if self.path.is_empty() {
                self.done = true;
                return None;
            }
        }
        // Find next slash.
        let end = self.path.find('/').unwrap_or(self.path.len());
        let seg = &self.path[..end];
        self.path = &self.path[end..];
        self.at_slash = true;
        Some(seg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_path() {
        let p = Path::parse("/a/b/c").unwrap();
        assert_eq!(p.as_str(), "/a/b/c");
    }

    #[test]
    fn collapses_multiple_slashes() {
        let p = Path::parse("//a///b//").unwrap();
        assert_eq!(p.as_str(), "/a/b");
    }

    #[test]
    fn rejects_relative_path() {
        assert_eq!(Path::parse("a/b"), Err(PathError::Invalid));
        assert_eq!(Path::parse(""), Err(PathError::Invalid));
    }

    #[test]
    fn splits_mount() {
        let p = Path::parse("/data/log.txt").unwrap();
        let (mount, sub) = p.split_mount().unwrap();
        assert_eq!(mount, "data");
        assert_eq!(sub, "/log.txt");
    }

    #[test]
    fn splits_root_mount() {
        let p = Path::parse("/etc").unwrap();
        let (mount, sub) = p.split_mount().unwrap();
        assert_eq!(mount, "etc");
        assert_eq!(sub, "");
    }

    #[test]
    fn iterates_segments() {
        use alloc::vec;
        use alloc::vec::Vec;
        let p = Path::parse("/a/b/c").unwrap();
        let segs: Vec<_> = p.segments().collect();
        assert_eq!(segs, vec!["a", "b", "c"]);
    }

    #[test]
    fn rejects_overlong_paths() {
        use alloc::string::ToString;
        let s = "/".to_string() + &"a".repeat(MAX_PATH_LEN);
        assert_eq!(Path::parse(&s), Err(PathError::TooLong));
    }
}
