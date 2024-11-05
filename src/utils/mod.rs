//! `Display` wrapper for optional values.
//!
//! Allows displaying an `Option<T>`, where `T` already implements `Display`.

use std::cmp::Ordering;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Result;

use datasize::DataSize;
use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::sha;
use serde::Deserialize;
use serde::Serialize;

mod big_array {
    use serde_big_array::big_array;

    big_array! { BigArray; }
}

/// SHA512 hash.
#[derive(Copy, Clone, DataSize, Deserialize, Serialize)]
pub struct Sha512(#[serde(with = "big_array::BigArray")] [u8; Sha512::SIZE]);

impl Sha512 {
    /// Size of digest in bytes.
    const SIZE: usize = 64;

    /// OpenSSL NID.
    pub const NID: Nid = Nid::SHA512;

    /// Create a new Sha512 by hashing a slice.
    pub fn new<B: AsRef<[u8]>>(data: B) -> Self {
        let mut openssl_sha = sha::Sha512::new();
        openssl_sha.update(data.as_ref());
        Sha512(openssl_sha.finish())
    }

    /// Returns bytestring of the hash, with length `Self::SIZE`.
    fn bytes(&self) -> &[u8] {
        let bs = &self.0[..];

        debug_assert_eq!(bs.len(), Self::SIZE);
        bs
    }

    /// Returns a new OpenSSL `MessageDigest` set to SHA-512.
    pub fn create_message_digest() -> MessageDigest {
        // This can only fail if we specify a `Nid` that does not exist, which cannot
        // happen unless there is something wrong with `Self::NID`.
        MessageDigest::from_nid(Self::NID).expect("Sha512::NID is invalid")
    }
}

// Below are trait implementations for signatures and fingerprints. Both
// implement the full set of traits that are required to stick into either a
// `HashMap` or `BTreeMap`.
impl PartialEq for Sha512 {
    #[inline]
    fn eq(&self, other: &Self) -> bool { self.bytes() == other.bytes() }
}

impl Eq for Sha512 {}

impl Ord for Sha512 {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering { Ord::cmp(self.bytes(), other.bytes()) }
}

impl PartialOrd for Sha512 {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(Ord::cmp(self, other)) }
}

impl Debug for Sha512 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", base16::encode_lower(&self.0[..]))
    }
}

/// Wrapper around `Option` that implements `Display`.
///
/// For convenience, it also includes a `Serialize` implementation that works
/// identical to the underlying `Option<T>` serialization.
pub struct OptDisplay<'a, T> {
    /// The actual `Option` being displayed.
    inner: Option<T>,
    /// Value to substitute if `inner` is `None`.
    empty_display: &'a str,
}

impl<'a, T> Serialize for OptDisplay<'a, T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.inner.serialize(serializer)
    }
}

impl<'a, T: Display> OptDisplay<'a, T> {
    /// Creates a new `OptDisplay`.
    #[inline]
    pub fn new(maybe_display: Option<T>, empty_display: &'a str) -> Self {
        Self {
            inner: maybe_display,
            empty_display,
        }
    }
}

impl<'a, T: Display> Display for OptDisplay<'a, T> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self.inner {
            None => f.write_str(self.empty_display),
            Some(ref val) => val.fmt(f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OptDisplay;

    #[test]
    fn opt_display_works() {
        let some_value: Option<u32> = Some(12345);

        assert_eq!(
            OptDisplay::new(some_value.as_ref(), "does not matter").to_string(),
            "12345"
        );

        let none_value: Option<u32> = None;
        assert_eq!(
            OptDisplay::new(none_value.as_ref(), "should be none").to_string(),
            "should be none"
        );
    }
}
