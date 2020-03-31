mod buffer;

use std::{fmt, cmp};
use std::cmp::{PartialOrd, Ord, Ordering};
use std::hash::{Hash, Hasher};
use std::convert::TryInto;
// use log::*;
use pct_str::PctStr;

use crate::parsing::ParsedIriRef;
use crate::{Scheme, Authority, Path, Query, Fragment, Error, Iri, IriBuf};

pub use self::buffer::*;

/// IRI-reference slice.
///
/// Wrapper around a borrowed bytes slice representing an IRI-reference.
/// An IRI-reference can be seen as an [`Iri`] with an optional [`Scheme`].
/// IRI-references are resolved against a *base IRI* into a proper IRI using
/// the [Reference Resolution Algorithm](https://tools.ietf.org/html/rfc3986#section-5) provided
/// by the [`resolved`](`IriRef::resolved`) method.
///
/// ## Example
///
/// ```rust
/// # extern crate iref;
/// # use std::convert::TryInto;
/// # use iref::{Iri, IriRef, IriRefBuf};
/// # fn main() -> Result<(), iref::Error> {
/// let base_iri = Iri::new("http://a/b/c/d;p?q")?;
/// let mut iri_ref = IriRefBuf::new("g;x=1/../y")?;
///
/// assert_eq!(iri_ref.resolved(base_iri), "http://a/b/c/y");
/// # Ok(())
/// # }
#[derive(Clone, Copy)]
pub struct IriRef<'a> {
	pub(crate) p: ParsedIriRef,
	pub(crate) data: &'a [u8],
}

impl<'a> IriRef<'a> {
	/// Create a new IRI-reference slice from a bytes slice.
	///
	/// This may fail if the source slice is not UTF-8 encoded, or if is not a valid IRI-reference.
	pub fn new<S: AsRef<[u8]> + ?Sized>(buffer: &'a S) -> Result<IriRef<'a>, Error> {
		Ok(IriRef {
			data: buffer.as_ref(),
			p: ParsedIriRef::new(buffer)?
		})
	}

	/// Get the underlying parsing data.
	pub fn parsing_data(&self) -> ParsedIriRef {
		self.p
	}

	/// Build an IRI reference from a slice and parsing data.
	///
	/// This is unsafe since the input slice is not checked against the given parsing data.
	pub const unsafe fn from_raw(data: &'a [u8], p: ParsedIriRef) -> IriRef<'a> {
		IriRef {
			p: p,
			data: data
		}
	}

	/// Get the length is the IRI-reference, in bytes.
	pub fn len(&self) -> usize {
		self.data.len()
	}

	/// Get a reference to the underlying bytes slice representing the IRI-reference.
	pub fn as_ref(&self) -> &[u8] {
		self.data
	}

	/// Convert the IRI-refrence into its underlying bytes slicee.
	pub fn into_ref(self) -> &'a [u8] {
		self.data
	}

	/// Get the IRI-reference as a string slice.
	pub fn as_str(&self) -> &str {
		unsafe {
			std::str::from_utf8_unchecked(self.data)
		}
	}

	/// Convert the IRI-reference into a string slice.
	pub fn into_str(self) -> &'a str {
		unsafe {
			std::str::from_utf8_unchecked(self.data)
		}
	}

	/// Get the IRI-reference as a percent-encoded string slice.
	pub fn as_pct_str(&self) -> &PctStr {
		unsafe {
			PctStr::new_unchecked(self.as_str())
		}
	}

	/// Convert the IRI-reference into a percent-encoded string slice.
	pub fn into_pct_str(self) -> &'a PctStr {
		unsafe {
			PctStr::new_unchecked(self.into_str())
		}
	}

	/// Get the scheme of the IRI-reference.
	///
	/// The scheme is located at the very begining of the IRI-reference and delimited by an ending
	/// `:`.
	///
	/// # Example
	///
	/// ```
	/// # use iref::IriRef;
	/// assert_eq!(IriRef::new("foo://example.com:8042").unwrap().scheme().unwrap(), "foo");
	/// assert_eq!(IriRef::new("//example.com:8042").unwrap().scheme(), None);
	/// ```
	pub fn scheme(&self) -> Option<Scheme> {
		if let Some(scheme_len) = self.p.scheme_len {
			Some(Scheme {
				data: &self.data[0..scheme_len]
			})
		} else {
			None
		}
	}

	/// Get the authority of the IRI-reference.
	///
	/// The authority is delimited by the `//` string, after the scheme.
	///
	/// # Example
	///
	/// ```
	/// # use iref::IriRef;
	/// assert_eq!(IriRef::new("foo://example.com:8042").unwrap().authority().unwrap().host(), "example.com");
	/// assert_eq!(IriRef::new("foo:").unwrap().authority(), None);
	/// ```
	pub fn authority(&self) -> Option<Authority> {
		if let Some(authority) = self.p.authority {
			let offset = self.p.authority_offset();
			Some(Authority {
				data: &self.data[offset..(offset+authority.len())],
				p: authority
			})
		} else {
			None
		}
	}

	/// Get the path of the IRI-reference.
	///
	/// The path is located just after the authority. It is always defined, even if empty.
	///
	/// # Example
	///
	/// ```
	/// # use iref::IriRef;
	/// assert_eq!(IriRef::new("foo:/a/b/c?query").unwrap().path(), "/a/b/c");
	/// assert!(IriRef::new("foo:#fragment").unwrap().path().is_empty());
	/// ```
	pub fn path(&'a self) -> Path<'a> {
		let offset = self.p.path_offset();
		Path {
			data: &self.data[offset..(offset+self.p.path_len)]
		}
	}

	/// Get the query of the IRI-reference.
	///
	/// The query part is delimited by the `?` character after the path.
	///
	/// # Example
	///
	/// ```
	/// # use iref::IriRef;
	/// assert_eq!(IriRef::new("//example.org?query").unwrap().query().unwrap(), "query");
	/// assert!(IriRef::new("//example.org/foo/bar#fragment").unwrap().query().is_none());
	/// ```
	pub fn query(&self) -> Option<Query> {
		if let Some(len) = self.p.query_len {
			let offset = self.p.query_offset();
			Some(Query {
				data: &self.data[offset..(offset+len)]
			})
		} else {
			None
		}
	}

	/// Get the fragment of the IRI-reference.
	///
	/// The fragment part is delimited by the `#` character after the query.
	///
	/// # Example
	///
	/// ```
	/// # use iref::IriRef;
	/// assert_eq!(IriRef::new("//example.org#foo").unwrap().fragment().unwrap(), "foo");
	/// assert!(IriRef::new("//example.org").unwrap().fragment().is_none());
	/// ```
	pub fn fragment(&self) -> Option<Fragment> {
		if let Some(len) = self.p.fragment_len {
			let offset = self.p.fragment_offset();
			Some(Fragment {
				data: &self.data[offset..(offset+len)]
			})
		} else {
			None
		}
	}

	/// Convert the IRI-reference into an IRI, if possible.
	///
	/// An IRI-reference is a valid IRI only if it has a defined [`Scheme`].
	pub fn into_iri(self) -> Result<Iri<'a>, IriRef<'a>> {
		self.try_into()
	}

	/// Resolve the IRI reference against the given *base IRI*.
	///
	/// Return the resolved IRI.
	/// See the [`IriRefBuf::resolve`] method for more informations about the resolution process.
	pub fn resolved<'b, Base: Into<Iri<'b>>>(&self, base_iri: Base) -> IriBuf {
		let mut iri_ref: IriRefBuf = self.into();
		iri_ref.resolve(base_iri);
		iri_ref.try_into().unwrap()
	}
}

impl<'a> fmt::Display for IriRef<'a> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.as_str().fmt(f)
	}
}

impl<'a> fmt::Debug for IriRef<'a> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.as_str().fmt(f)
	}
}

impl<'a> cmp::PartialEq for IriRef<'a> {
	fn eq(&self, other: &IriRef) -> bool {
		self.scheme() == other.scheme() && self.fragment() == other.fragment() && self.authority() == other.authority() && self.path() == other.path() && self.query() == other.query()
	}
}

impl<'a> Eq for IriRef<'a> { }

impl<'a> cmp::PartialEq<IriRefBuf> for IriRef<'a> {
	fn eq(&self, other: &IriRefBuf) -> bool {
		*self == other.as_iri_ref()
	}
}

impl<'a> cmp::PartialEq<Iri<'a>> for IriRef<'a> {
	fn eq(&self, other: &Iri<'a>) -> bool {
		*self == other.as_iri_ref()
	}
}

impl<'a> cmp::PartialEq<IriBuf> for IriRef<'a> {
	fn eq(&self, other: &IriBuf) -> bool {
		*self == other.as_iri_ref()
	}
}

impl<'a> cmp::PartialEq<&'a str> for IriRef<'a> {
	fn eq(&self, other: &&'a str) -> bool {
		if let Ok(other) = IriRef::new(other) {
			self == &other
		} else {
			false
		}
	}
}

impl<'a> PartialOrd for IriRef<'a> {
	fn partial_cmp(&self, other: &IriRef<'a>) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl<'a> Ord for IriRef<'a> {
	fn cmp(&self, other: &IriRef<'a>) -> Ordering {
		if self.scheme() == other.scheme() {
			if self.authority() == other.authority() {
				if self.path() == other.path() {
					if self.query() == other.query() {
						self.fragment().cmp(&other.fragment())
					} else {
						self.query().cmp(&other.query())
					}
				} else {
					self.path().cmp(&other.path())
				}
			} else {
				self.authority().cmp(&other.authority())
			}
		} else {
			self.scheme().cmp(&other.scheme())
		}
	}
}

impl<'a> PartialOrd<IriRefBuf> for IriRef<'a> {
	fn partial_cmp(&self, other: &IriRefBuf) -> Option<Ordering> {
		self.partial_cmp(&other.as_iri_ref())
	}
}

impl<'a> PartialOrd<Iri<'a>> for IriRef<'a> {
	fn partial_cmp(&self, other: &Iri<'a>) -> Option<Ordering> {
		self.partial_cmp(&other.as_iri_ref())
	}
}

impl<'a> PartialOrd<IriBuf> for IriRef<'a> {
	fn partial_cmp(&self, other: &IriBuf) -> Option<Ordering> {
		self.partial_cmp(&other.as_iri_ref())
	}
}

impl<'a> From<&'a IriRefBuf> for IriRef<'a> {
	fn from(iri_ref_buf: &'a IriRefBuf) -> IriRef<'a> {
		iri_ref_buf.as_iri_ref()
	}
}

impl<'a> From<Iri<'a>> for IriRef<'a> {
	fn from(iri: Iri<'a>) -> IriRef<'a> {
		iri.as_iri_ref()
	}
}

impl<'a> From<&'a IriBuf> for IriRef<'a> {
	fn from(iri_ref_buf: &'a IriBuf) -> IriRef<'a> {
		iri_ref_buf.as_iri_ref()
	}
}

impl<'a> Hash for IriRef<'a> {
	fn hash<H: Hasher>(&self, hasher: &mut H) {
		self.scheme().hash(hasher);
		self.authority().hash(hasher);
		self.path().hash(hasher);
		self.query().hash(hasher);
		self.fragment().hash(hasher);
	}
}
