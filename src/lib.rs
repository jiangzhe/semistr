pub mod error;
pub use error::{Result, Error};

use std::mem::{transmute, ManuallyDrop};
use std::alloc::{alloc, Layout};
use std::ops::Deref;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::cmp::Ordering;
use std::sync::Arc;
use std::iter;
use std::borrow::Borrow;

const INLINE_CAP: usize = 12;

/// SemiStr is an immutable string with length no more than 4GB.
#[repr(C, align(8))]
pub struct SemiStr([u8; 16]);

impl SemiStr {
    #[inline]
    pub fn new(s: &str) -> Self {
        Self::try_from(s).unwrap()
    }

    #[inline]
    pub fn inline(s: &str) -> Self {
        assert!(s.len() <= INLINE_CAP);
        unsafe { inline_str(s.as_bytes()) }
    }

    #[inline]
    pub fn len(&self) -> usize {
        let heap: &Heap = unsafe { transmute(self) };
        heap.len as usize
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        self.as_ref().as_bytes()
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }

    fn from_char_iter<I: iter::Iterator<Item = char>>(mut iter: I) -> SemiStr {
        let (min_size, _) = iter.size_hint();
        assert!(min_size <= u32::MAX as usize);
        if min_size > INLINE_CAP {
            let s: String = iter.collect();
            return unsafe { heap_string(s.into_bytes()) }
        }
        let mut len = 0;
        let mut data = [0u8; INLINE_CAP];
        while let Some(ch) = iter.next() {
            let size = ch.len_utf8();
            if size + len > INLINE_CAP {
                let (min_remaining, _) = iter.size_hint();
                let mut heap = String::with_capacity(size + len + min_remaining);
                heap.push_str(core::str::from_utf8(&data[..len]).unwrap());
                heap.push(ch);
                heap.extend(iter);
                return unsafe { heap_string(heap.into_bytes()) }
            }
            ch.encode_utf8(&mut data[len..]);
            len += size;
            assert!(len <= u32::MAX as usize);
        }
        let inline = Inline{len: len as u32, data};
        unsafe { transmute(inline) }
    }
}

impl Deref for SemiStr {
    type Target = str;
    #[inline]
    fn deref(&self) -> &str {
        unsafe {
            let len = self.len();
            if len <= INLINE_CAP {
                std::str::from_utf8_unchecked(&self.0[4..4+len])
            } else {
                let heap: &Heap = transmute(self);
                std::str::from_utf8_unchecked(&heap.ptr)
            }
        }
    }
}

impl AsRef<str> for SemiStr {
    #[inline]
    fn as_ref(&self) -> &str {
        &*self
    }
}

impl Default for SemiStr {
    #[inline]
    fn default() -> Self {
        let inline = Inline{len: 0, data: [0u8; 12]};
        unsafe { transmute(inline) }
    }
}

impl<'s> TryFrom<&'s str> for SemiStr {
    type Error = Error;
    #[inline]
    fn try_from(value: &'s str) -> Result<Self> {
        if value.len() <= INLINE_CAP {
            // SAFETY
            //
            // valid utf-8 string and length is no more than 12
            Ok(unsafe { inline_str(value.as_bytes()) })
        } else if value.len() <= u32::MAX as usize {
            // SAFETY
            // 
            // valid utf-8 string and length between 13 and u32::MAX
            Ok(unsafe { heap_str(value.as_bytes()) })
        } else {
            Err(Error::StringTooLong(value.len()))
        }
    }
}

impl<'s> TryFrom<&'s [u8]> for SemiStr {
    type Error = Error;
    #[inline]
    fn try_from(value: &'s [u8]) -> Result<Self> {
        let s = std::str::from_utf8(value)
            .map_err(|_| Error::InvalidUtf8String)?;
        Self::try_from(s)
    }
}

impl TryFrom<String> for SemiStr {
    type Error = Error;
    #[inline]
    fn try_from(value: String) -> Result<Self> {
        if value.len() <= INLINE_CAP {
            // SAFETY
            //
            // valid utf-8 string and length is no more than 12
            Ok(unsafe { inline_str(value.as_bytes()) })
        } else if value.len() <= u32::MAX as usize {
            // SAFETY
            // 
            // valid utf-8 string and length between 13 and u32::MAX
            Ok(unsafe { heap_string(value.into_bytes()) })
        } else {
            Err(Error::StringTooLong(value.len()))
        }
    }
}

impl Drop for SemiStr {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            let heap = transmute::<&mut SemiStr, &mut Heap>(self);
            if heap.len as usize <= INLINE_CAP {
                return // skip inline format
            }
            std::ptr::drop_in_place(&mut heap.ptr as *mut Arc<Box<[u8]>>);
        }
    }
}

impl PartialEq<str> for SemiStr {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        if self.len() != other.len() {
            return false
        }
        if other.len() <= INLINE_CAP {
            return self.as_ref() == other
        }
        // compare prefix first
        if self.0[4..8] != other.as_bytes()[..4] {
            return false
        }
        self.as_ref() == other
    }
}

impl PartialEq<SemiStr> for str {
    #[inline]
    fn eq(&self, other: &SemiStr) -> bool {
        other.eq(self)
    }
}

impl PartialEq<&'_ str> for SemiStr {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        if self.len() != other.len() {
            return false
        }
        if other.len() <= INLINE_CAP {
            return self.as_ref() == *other
        }
        // compare prefix first
        if self.0[4..8] != other.as_bytes()[..4] {
            return false
        }
        self.as_ref() == *other
    }
}

impl PartialEq<SemiStr> for &'_ str {
    #[inline]
    fn eq(&self, other: &SemiStr) -> bool {
        other.eq(self)
    }
}

impl PartialEq for SemiStr {
    #[inline]
    fn eq(&self, other: &SemiStr) -> bool {
        if self.len() != other.len() {
            return false
        }
        if other.len() <= INLINE_CAP {
            return self.0[4..] == other.0[4..]
        }
        // compare prefix
        if self.0[4..8] != other.0[4..8] {
            return false
        }
        self.as_ref() == other.as_ref()
    }
} 

impl Eq for SemiStr {}

impl Hash for SemiStr {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state)
    }
}

impl PartialOrd for SemiStr {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SemiStr {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_ref().cmp(other.as_ref())
    }
}

impl fmt::Debug for SemiStr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_ref(), f)
    }
}

impl fmt::Display for SemiStr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_ref(), f)
    }
}

impl Clone for SemiStr {
    #[inline]
    fn clone(&self) -> Self {
        if self.len() <= INLINE_CAP {
            return SemiStr(self.0)
        }
        unsafe { heap_str(self.as_bytes()) }
    }
}

impl Borrow<str> for SemiStr {
    #[inline]
    fn borrow(&self) -> &str {
        self.as_ref()
    }
}

impl iter::FromIterator<char> for SemiStr {
    fn from_iter<I: iter::IntoIterator<Item = char>>(iter: I) -> SemiStr {
        let iter = iter.into_iter();
        Self::from_char_iter(iter)
    }
}

/// Inline represents the inline format of short string,
/// which is no longer than 12 bytes.
/// In this scenario, all bytes are stored on stack.
#[repr(C, align(8))]
struct Inline {
    len: u32,
    data: [u8; INLINE_CAP],
}

/// Heap represents the long string stored on heap.
#[repr(C, align(8))]
struct Heap {
    len: u32,
    prefix: [u8; 4],
    ptr: Arc<Box<[u8]>>,
}

/// Construct SemiStr with inline format.
/// 
/// # Safety
/// 
/// input bytes must be valid utf-8 string and length should be no more than 12.
#[inline]
unsafe fn inline_str(value: &[u8]) -> SemiStr {
    let mut data = [0u8; INLINE_CAP];
    data[..value.len()].copy_from_slice(value);
    let inline = Inline{len: value.len() as u32, data};
    unsafe { transmute(inline) }
}

/// Construct SemiStr with heap format.
/// 
/// # Safety
/// 
/// input bytes must be valid utf-8 string and length should be between 13 and u32::MAX.
#[inline]
unsafe fn heap_str(value: &[u8]) -> SemiStr {
    let mut prefix = [0u8; 4];
    prefix.copy_from_slice(&value[..4]);
    let layout = Layout::from_size_align(value.len(), 1).unwrap();
    let ptr = alloc(layout);
    std::ptr::copy_nonoverlapping(value.as_ptr(), ptr, value.len());
    let slice = std::slice::from_raw_parts_mut(ptr, value.len());
    let boxed: Box<[u8]> = transmute(slice);
    let heap = Heap{len: value.len() as u32, prefix, ptr: Arc::new(boxed)};
    transmute(heap)
}

#[inline]
unsafe fn heap_string(mut value: Vec<u8>) -> SemiStr {
    debug_assert!(value.len() > INLINE_CAP && value.len() <= u32::MAX as usize);
    value.shrink_to_fit();
    let mut prefix = [0u8; 4];
    prefix.copy_from_slice(&value[..4]);
    let mut value = ManuallyDrop::new(value);
    let ptr = value.as_mut_ptr();
    let len = value.len();
    let slice = std::slice::from_raw_parts_mut(ptr, value.len());
    let boxed: Box<[u8]> = transmute(slice);
    let heap = Heap{len: len as u32, prefix, ptr: Arc::new(boxed)};
    unsafe { transmute(heap) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semi_str() {
        let s1 = SemiStr::new("hello");
        assert!(!s1.is_empty());
        assert_eq!(5, s1.len());
        assert_eq!(s1, "hello");
        assert_eq!("hello", s1);
        let s2 = SemiStr::inline("world");
        assert!(s2 > s1);
        let s3 = SemiStr::new("a little longer than 12 bytes");
        let s4 = SemiStr::from_iter(b"a little longer than 12 bytes".into_iter().map(|b| *b as char));
        assert_eq!(s3, s4);
        let s5 = SemiStr::from_iter("short str".chars());
        assert_eq!(s5.len(), 9);
        println!("{:?}, {}", s5, s5);
        let s6 = SemiStr::default();
        assert!(s6.is_empty());
        let s7 = SemiStr::new("");
        assert_eq!(s6, s7);
        assert!(SemiStr::try_from(&[0u8, 0xff, 0xff, 0xff][..]).is_err());
    }
}