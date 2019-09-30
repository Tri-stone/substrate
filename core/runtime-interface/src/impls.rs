// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Provides implementations for the runtime interface traits.

use crate::{RIType, pass_by::{PassBy, Inner, Codec, PassByInner}};
#[cfg(feature = "std")]
use crate::host::*;
#[cfg(not(feature = "std"))]
use crate::wasm::*;

#[cfg(not(feature = "std"))]
use static_assertions::assert_eq_size;

#[cfg(feature = "std")]
use wasm_interface::{FunctionContext, Pointer, Result};

use codec::{Encode, Decode};

use rstd::{any::TypeId, mem};

#[cfg(feature = "std")]
use rstd::borrow::Cow;

#[cfg(not(feature = "std"))]
use rstd::slice;

use primitives::{sr25519, ed25519};

/// Converts a pointer and length into an `u64`.
pub fn pointer_and_len_to_u64(ptr: u32, len: u32) -> u64 {
	((len as u64) | u64::from(ptr) << 32).to_le()
}

/// Splits an `u64` into the pointer and length.
pub fn pointer_and_len_from_u64(val: u64) -> (u32, u32) {
	let val = u64::from_le(val);
	let len = (val & (!0u32 as u64)) as u32;
	let ptr = (val >> 32) as u32;

	(ptr, len)
}

/// Implement the traits for the given primitive traits.
macro_rules! impl_traits_for_primitives {
	(
		$(
			$rty:ty, $fty:ty,
		)*
	) => {
		$(
			impl RIType for $rty {
				type FFIType = $fty;
			}

			#[cfg(not(feature = "std"))]
			impl IntoFFIValue for $rty {
				type Owned = ();

				fn into_ffi_value(&self) -> WrappedFFIValue<$fty> {
					(*self as $fty).to_le().into()
				}
			}

			#[cfg(not(feature = "std"))]
			impl FromFFIValue for $rty {
				fn from_ffi_value(arg: $fty) -> $rty {
					<$fty>::from_le(arg) as $rty
				}
			}

			#[cfg(feature = "std")]
			impl FromFFIValue for $rty {
				type SelfInstance = $rty;

				fn from_ffi_value(_: &mut dyn FunctionContext, arg: $fty) -> Result<$rty> {
					Ok(<$fty>::from_le(arg) as $rty)
				}
			}

			#[cfg(feature = "std")]
			impl IntoFFIValue for $rty {
				fn into_ffi_value(self, _: &mut dyn FunctionContext) -> Result<$fty> {
					Ok((self as $fty).to_le())
				}
			}
		)*
	}
}

impl_traits_for_primitives! {
	u8, u8,
	u16, u16,
	u32, u32,
	u64, u64,
	i8, i8,
	i16, i16,
	i32, i32,
	i64, i64,
}

impl RIType for bool {
	type FFIType = u8;
}

#[cfg(not(feature = "std"))]
impl IntoFFIValue for bool {
	type Owned = ();

	fn into_ffi_value(&self) -> WrappedFFIValue<u8> {
		if *self { 1 } else { 0 }.into()
	}
}

#[cfg(not(feature = "std"))]
impl FromFFIValue for bool {
	fn from_ffi_value(arg: u8) -> bool {
		arg == 1
	}
}

#[cfg(feature = "std")]
impl FromFFIValue for bool {
	type SelfInstance = bool;

	fn from_ffi_value(_: &mut dyn FunctionContext, arg: u8) -> Result<bool> {
		Ok(arg == 1)
	}
}

#[cfg(feature = "std")]
impl IntoFFIValue for bool {
	fn into_ffi_value(self, _: &mut dyn FunctionContext) -> Result<u8> {
		Ok(if self { 1 } else { 0 })
	}
}

impl<T> RIType for Vec<T> {
	type FFIType = u64;
}

#[cfg(feature = "std")]
impl<T: 'static + Encode> IntoFFIValue for Vec<T> {
	fn into_ffi_value(self, context: &mut dyn FunctionContext) -> Result<u64> {
		let vec: Cow<'_, [u8]> = if TypeId::of::<T>() == TypeId::of::<u8>() {
			unsafe { Cow::Borrowed(mem::transmute(&self[..])) }
		} else {
			Cow::Owned(self.encode())
		};

		let ptr = context.allocate_memory(vec.as_ref().len() as u32)?;
		context.write_memory(ptr, &vec)?;

		Ok(pointer_and_len_to_u64(ptr.into(), vec.len() as u32))
	}
}

#[cfg(feature = "std")]
impl<T: 'static + Decode> FromFFIValue for Vec<T> {
	type SelfInstance = Vec<T>;

	fn from_ffi_value(context: &mut dyn FunctionContext, arg: u64) -> Result<Vec<T>> {
		<[T] as FromFFIValue>::from_ffi_value(context, arg)
	}
}

impl<T> RIType for [T] {
	type FFIType = u64;
}

#[cfg(feature = "std")]
impl<T: 'static + Decode> FromFFIValue for [T] {
	type SelfInstance = Vec<T>;

	fn from_ffi_value(context: &mut dyn FunctionContext, arg: u64) -> Result<Vec<T>> {
		let (ptr, len) = pointer_and_len_from_u64(arg);

		let vec = context.read_memory(Pointer::new(ptr), len)?;

		if TypeId::of::<T>() == TypeId::of::<u8>() {
			Ok(unsafe { mem::transmute(vec) })
		} else {
			Ok(Vec::<T>::decode(&mut &vec[..]).expect("Wasm to host values are encoded correctly; qed"))
		}
	}
}

#[cfg(feature = "std")]
impl IntoPreallocatedFFIValue for [u8] {
	type SelfInstance = Vec<u8>;

	fn into_preallocated_ffi_value(
		self_instance: Self::SelfInstance,
		context: &mut dyn FunctionContext,
		allocated: u64,
	) -> Result<()> {
		let (ptr, len) = pointer_and_len_from_u64(allocated);

		if (len as usize) < self_instance.len() {
			Err(
				format!(
					"Preallocated buffer is not big enough (given {} vs needed {})!",
					len,
					self_instance.len()
				)
			)
		} else {
			context.write_memory(Pointer::new(ptr), &self_instance)
		}
	}
}

// Make sure that our assumptions for storing a pointer + its size in `u64` is valid.
#[cfg(not(feature = "std"))]
assert_eq_size!(usize_check; usize, u32);
#[cfg(not(feature = "std"))]
assert_eq_size!(ptr_check; *const u8, u32);

#[cfg(not(feature = "std"))]
impl<T: 'static + Encode> IntoFFIValue for [T] {
	type Owned = Vec<u8>;

	fn into_ffi_value(&self) -> WrappedFFIValue<u64, Vec<u8>> {
		if TypeId::of::<T>() == TypeId::of::<u8>() {
			let slice = unsafe { mem::transmute::<&[T], &[u8]>(self) };
			pointer_and_len_to_u64(slice.as_ptr() as u32, slice.len() as u32).into()
		} else {
			let data = self.encode();
			let ffi_value = pointer_and_len_to_u64(data.as_ptr() as u32, data.len() as u32);
			(ffi_value, data).into()
		}
	}
}

#[cfg(not(feature = "std"))]
impl<T: 'static + Encode> IntoFFIValue for Vec<T> {
	type Owned = Vec<u8>;

	fn into_ffi_value(&self) -> WrappedFFIValue<u64, Vec<u8>> {
		self[..].into_ffi_value()
	}
}

#[cfg(not(feature = "std"))]
impl<T: 'static + Decode> FromFFIValue for Vec<T> {
	fn from_ffi_value(arg: u64) -> Vec<T> {
		let (ptr, len) = pointer_and_len_from_u64(arg);
		let len = len as usize;

		if TypeId::of::<T>() == TypeId::of::<u8>() {
			unsafe { mem::transmute(Vec::from_raw_parts(ptr as *mut u8, len, len)) }
		} else {
			let slice = unsafe { slice::from_raw_parts(ptr as *const u8, len) };
			Self::decode(&mut &slice[..]).expect("Host to wasm values are encoded correctly; qed")
		}
	}
}

/// Implement the traits for the `[u8; N]` arrays, where `N` is the input to this macro.
macro_rules! impl_traits_for_arrays {
	(
		$(
			$n:expr
		),*
		$(,)?
	) => {
		$(
			impl RIType for [u8; $n] {
				type FFIType = u32;
			}

			#[cfg(not(feature = "std"))]
			impl IntoFFIValue for [u8; $n] {
				type Owned = ();

				fn into_ffi_value(&self) -> WrappedFFIValue<u32> {
					(self.as_ptr() as u32).into()
				}
			}

			#[cfg(not(feature = "std"))]
			impl FromFFIValue for [u8; $n] {
				fn from_ffi_value(arg: u32) -> [u8; $n] {
					let mut res = unsafe { mem::MaybeUninit::<[u8; $n]>::zeroed().assume_init() };
					res.copy_from_slice(unsafe { slice::from_raw_parts(arg as *const u8, $n) });

					// Make sure we free the pointer.
					let _ = unsafe { Box::from_raw(arg as *mut u8) };

					res
				}
			}

			#[cfg(feature = "std")]
			impl FromFFIValue for [u8; $n] {
				type SelfInstance = [u8; $n];

				fn from_ffi_value(context: &mut dyn FunctionContext, arg: u32) -> Result<[u8; $n]> {
					let data = context.read_memory(Pointer::new(arg), $n)?;
					let mut res = unsafe { mem::MaybeUninit::<[u8; $n]>::zeroed().assume_init() };
					res.copy_from_slice(&data);
					Ok(res)
				}
			}

			#[cfg(feature = "std")]
			impl IntoFFIValue for [u8; $n] {
				fn into_ffi_value(self, context: &mut dyn FunctionContext) -> Result<u32> {
					let addr = context.allocate_memory($n)?;
					context.write_memory(addr, &self)?;
					Ok(addr.into())
				}
			}

			#[cfg(feature = "std")]
			impl IntoPreallocatedFFIValue for [u8; $n] {
				type SelfInstance = [u8; $n];

				fn into_preallocated_ffi_value(
					self_instance: Self::SelfInstance,
					context: &mut dyn FunctionContext,
					allocated: u32,
				) -> Result<()> {
					context.write_memory(Pointer::new(allocated), &self_instance)
				}
			}
		)*
	}
}

impl_traits_for_arrays! {
	1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26,
	27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50,
	51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64
}

impl<T: codec::Codec> PassBy for Option<T> {
	type PassBy = Codec<Self>;
}

impl<T: codec::Codec, E: codec::Codec> PassBy for rstd::result::Result<T, E> {
	type PassBy = Codec<Self>;
}

impl PassBy for sr25519::Public {
	type PassBy = Inner<Self, [u8; 32]>;
}

impl PassByInner for sr25519::Public {
	type Inner = [u8; 32];

	fn into_inner(self) -> Self::Inner {
		self.0
	}

	fn inner(&self) -> &Self::Inner {
		&self.0
	}

	fn from_inner(inner: Self::Inner) -> Self {
		Self(inner)
	}
}

impl PassBy for sr25519::Signature {
	type PassBy = Inner<Self, [u8; 64]>;
}

impl PassByInner for sr25519::Signature {
	type Inner = [u8; 64];

	fn into_inner(self) -> Self::Inner {
		self.0
	}

	fn inner(&self) -> &Self::Inner {
		&self.0
	}

	fn from_inner(inner: Self::Inner) -> Self {
		Self(inner)
	}
}

impl PassBy for ed25519::Public {
	type PassBy = Inner<Self, [u8; 32]>;
}

impl PassByInner for ed25519::Public {
	type Inner = [u8; 32];

	fn into_inner(self) -> Self::Inner {
		self.0
	}

	fn inner(&self) -> &Self::Inner {
		&self.0
	}

	fn from_inner(inner: Self::Inner) -> Self {
		Self(inner)
	}
}

impl PassBy for ed25519::Signature {
	type PassBy = Inner<Self, [u8; 64]>;
}

impl PassByInner for ed25519::Signature {
	type Inner = [u8; 64];

	fn into_inner(self) -> Self::Inner {
		self.0
	}

	fn inner(&self) -> &Self::Inner {
		&self.0
	}

	fn from_inner(inner: Self::Inner) -> Self {
		Self(inner)
	}
}
