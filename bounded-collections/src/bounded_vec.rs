// This file is part of Substrate.

// Copyright (C) 2017-2023 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Traits, types and structs to support putting a bounded vector into storage, as a raw value, map
//! or a double map.

use super::WeakBoundedVec;
use crate::{Get, TryCollect};
use alloc::vec::Vec;
use core::{
	marker::PhantomData,
	ops::{Deref, Index, IndexMut, RangeBounds},
	slice::SliceIndex,
};
#[cfg(feature = "serde")]
use serde::{
	de::{Error, SeqAccess, Visitor},
	Deserialize, Deserializer, Serialize,
};

/// A bounded vector.
///
/// It has implementations for efficient append and length decoding, as with a normal `Vec<_>`, once
/// put into storage as a raw value, map or double-map.
///
/// As the name suggests, the length of the queue is always bounded. All internal operations ensure
/// this bound is respected.
#[cfg_attr(feature = "serde", derive(Serialize), serde(transparent))]
#[cfg_attr(feature = "jam-codec", derive(jam_codec::Encode))]
#[cfg_attr(feature = "scale-codec", derive(scale_codec::Encode, scale_info::TypeInfo))]
#[cfg_attr(feature = "scale-codec", scale_info(skip_type_params(S)))]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BoundedVec<T, S>(pub(super) Vec<T>, #[cfg_attr(feature = "serde", serde(skip_serializing))] PhantomData<S>);

/// Create an object through truncation.
pub trait TruncateFrom<T> {
	/// Create an object through truncation.
	fn truncate_from(unbound: T) -> Self;
}

#[cfg(feature = "serde")]
mod serde_impl {
	use super::*;

	impl<'de, T, S: Get<u32>> Deserialize<'de> for BoundedVec<T, S>
	where
		T: Deserialize<'de>,
	{
		fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
		where
			D: Deserializer<'de>,
		{
			struct VecVisitor<T, S: Get<u32>>(PhantomData<(T, S)>);

			impl<'de, T, S: Get<u32>> Visitor<'de> for VecVisitor<T, S>
			where
				T: Deserialize<'de>,
			{
				type Value = Vec<T>;

				fn expecting(&self, formatter: &mut alloc::fmt::Formatter) -> alloc::fmt::Result {
					formatter.write_str("a sequence")
				}

				fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
				where
					A: SeqAccess<'de>,
				{
					let size = seq.size_hint().unwrap_or(0);
					let max = match usize::try_from(S::get()) {
						Ok(n) => n,
						Err(_) => return Err(A::Error::custom("can't convert to usize")),
					};
					if size > max {
						Err(A::Error::custom("out of bounds"))
					} else {
						let mut values = Vec::with_capacity(size);

						while let Some(value) = seq.next_element()? {
							if values.len() >= max {
								return Err(A::Error::custom("out of bounds"));
							}
							values.push(value);
						}

						Ok(values)
					}
				}
			}

			let visitor: VecVisitor<T, S> = VecVisitor(PhantomData);
			deserializer
				.deserialize_seq(visitor)
				.map(|v| BoundedVec::<T, S>::try_from(v).map_err(|_| Error::custom("out of bounds")))?
		}
	}
}

/// A bounded slice.
///
/// Similar to a `BoundedVec`, but not owned and cannot be decoded.
#[cfg_attr(feature = "scale-codec", derive(scale_codec::Encode, scale_info::TypeInfo))]
#[cfg_attr(feature = "jam-codec", derive(jam_codec::Encode))]
pub struct BoundedSlice<'a, T, S>(pub(super) &'a [T], PhantomData<S>);

impl<'a, T, BoundSelf, BoundRhs> PartialEq<BoundedSlice<'a, T, BoundRhs>> for BoundedSlice<'a, T, BoundSelf>
where
	T: PartialEq,
	BoundSelf: Get<u32>,
	BoundRhs: Get<u32>,
{
	fn eq(&self, other: &BoundedSlice<'a, T, BoundRhs>) -> bool {
		self.0 == other.0
	}
}

impl<'a, T, BoundSelf, BoundRhs> PartialEq<BoundedVec<T, BoundRhs>> for BoundedSlice<'a, T, BoundSelf>
where
	T: PartialEq,
	BoundSelf: Get<u32>,
	BoundRhs: Get<u32>,
{
	fn eq(&self, other: &BoundedVec<T, BoundRhs>) -> bool {
		self.0 == other.0
	}
}

impl<'a, T, BoundSelf, BoundRhs> PartialEq<WeakBoundedVec<T, BoundRhs>> for BoundedSlice<'a, T, BoundSelf>
where
	T: PartialEq,
	BoundSelf: Get<u32>,
	BoundRhs: Get<u32>,
{
	fn eq(&self, other: &WeakBoundedVec<T, BoundRhs>) -> bool {
		self.0 == other.0
	}
}

impl<'a, T, S: Get<u32>> Eq for BoundedSlice<'a, T, S> where T: Eq {}

impl<'a, T, BoundSelf, BoundRhs> PartialOrd<BoundedSlice<'a, T, BoundRhs>> for BoundedSlice<'a, T, BoundSelf>
where
	T: PartialOrd,
	BoundSelf: Get<u32>,
	BoundRhs: Get<u32>,
{
	fn partial_cmp(&self, other: &BoundedSlice<'a, T, BoundRhs>) -> Option<core::cmp::Ordering> {
		self.0.partial_cmp(other.0)
	}
}

impl<'a, T, BoundSelf, BoundRhs> PartialOrd<BoundedVec<T, BoundRhs>> for BoundedSlice<'a, T, BoundSelf>
where
	T: PartialOrd,
	BoundSelf: Get<u32>,
	BoundRhs: Get<u32>,
{
	fn partial_cmp(&self, other: &BoundedVec<T, BoundRhs>) -> Option<core::cmp::Ordering> {
		self.0.partial_cmp(&*other.0)
	}
}

impl<'a, T, BoundSelf, BoundRhs> PartialOrd<WeakBoundedVec<T, BoundRhs>> for BoundedSlice<'a, T, BoundSelf>
where
	T: PartialOrd,
	BoundSelf: Get<u32>,
	BoundRhs: Get<u32>,
{
	fn partial_cmp(&self, other: &WeakBoundedVec<T, BoundRhs>) -> Option<core::cmp::Ordering> {
		self.0.partial_cmp(&*other.0)
	}
}

impl<'a, T: Ord, Bound: Get<u32>> Ord for BoundedSlice<'a, T, Bound> {
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
		self.0.cmp(&other.0)
	}
}

impl<'a, T, S: Get<u32>> TryFrom<&'a [T]> for BoundedSlice<'a, T, S> {
	type Error = &'a [T];
	fn try_from(t: &'a [T]) -> Result<Self, Self::Error> {
		if t.len() <= S::get() as usize {
			Ok(BoundedSlice(t, PhantomData))
		} else {
			Err(t)
		}
	}
}

impl<'a, T, S> From<BoundedSlice<'a, T, S>> for &'a [T] {
	fn from(t: BoundedSlice<'a, T, S>) -> Self {
		t.0
	}
}

impl<'a, T, S: Get<u32>> TruncateFrom<&'a [T]> for BoundedSlice<'a, T, S> {
	fn truncate_from(unbound: &'a [T]) -> Self {
		BoundedSlice::<T, S>::truncate_from(unbound)
	}
}

impl<'a, T, S> Clone for BoundedSlice<'a, T, S> {
	fn clone(&self) -> Self {
		BoundedSlice(self.0, PhantomData)
	}
}

impl<'a, T, S> core::fmt::Debug for BoundedSlice<'a, T, S>
where
	&'a [T]: core::fmt::Debug,
	S: Get<u32>,
{
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_tuple("BoundedSlice").field(&self.0).field(&S::get()).finish()
	}
}

// Since a reference `&T` is always `Copy`, so is `BoundedSlice<'a, T, S>`.
impl<'a, T, S> Copy for BoundedSlice<'a, T, S> {}

// will allow for all immutable operations of `[T]` on `BoundedSlice<T>`.
impl<'a, T, S> Deref for BoundedSlice<'a, T, S> {
	type Target = [T];

	fn deref(&self) -> &Self::Target {
		self.0
	}
}

// Custom implementation of `Hash` since deriving it would require all generic bounds to also
// implement it.
#[cfg(feature = "std")]
impl<'a, T: std::hash::Hash, S> std::hash::Hash for BoundedSlice<'a, T, S> {
	fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
		self.0.hash(state);
	}
}

impl<'a, T, S> core::iter::IntoIterator for BoundedSlice<'a, T, S> {
	type Item = &'a T;
	type IntoIter = core::slice::Iter<'a, T>;
	fn into_iter(self) -> Self::IntoIter {
		self.0.iter()
	}
}

impl<'a, T, S: Get<u32>> BoundedSlice<'a, T, S> {
	/// Create an instance from the first elements of the given slice (or all of it if it is smaller
	/// than the length bound).
	pub fn truncate_from(s: &'a [T]) -> Self {
		Self(&s[0..(s.len().min(S::get() as usize))], PhantomData)
	}
}

impl<T, S> BoundedVec<T, S> {
	/// Create `Self` with no items.
	pub fn new() -> Self {
		Self(Vec::new(), Default::default())
	}

	/// Create `Self` from `t` without any checks.
	fn unchecked_from(t: Vec<T>) -> Self {
		Self(t, Default::default())
	}

	/// Exactly the same semantics as `Vec::clear`.
	pub fn clear(&mut self) {
		self.0.clear()
	}

	/// Consume self, and return the inner `Vec`. Henceforth, the `Vec<_>` can be altered in an
	/// arbitrary way. At some point, if the reverse conversion is required, `TryFrom<Vec<_>>` can
	/// be used.
	///
	/// This is useful for cases if you need access to an internal API of the inner `Vec<_>` which
	/// is not provided by the wrapper `BoundedVec`.
	pub fn into_inner(self) -> Vec<T> {
		self.0
	}

	/// Exactly the same semantics as [`slice::sort_by`].
	///
	/// This is safe since sorting cannot change the number of elements in the vector.
	pub fn sort_by<F>(&mut self, compare: F)
	where
		F: FnMut(&T, &T) -> core::cmp::Ordering,
	{
		self.0.sort_by(compare)
	}

	/// Exactly the same semantics as [`slice::sort_by_key`].
	///
	/// This is safe since sorting cannot change the number of elements in the vector.
	pub fn sort_by_key<K, F>(&mut self, f: F)
	where
		F: FnMut(&T) -> K,
		K: core::cmp::Ord,
	{
		self.0.sort_by_key(f)
	}

	/// Exactly the same semantics as [`slice::sort`].
	///
	/// This is safe since sorting cannot change the number of elements in the vector.
	pub fn sort(&mut self)
	where
		T: core::cmp::Ord,
	{
		self.0.sort()
	}

	/// Exactly the same semantics as `Vec::remove`.
	///
	/// # Panics
	///
	/// Panics if `index` is out of bounds.
	pub fn remove(&mut self, index: usize) -> T {
		self.0.remove(index)
	}

	/// Exactly the same semantics as `slice::swap_remove`.
	///
	/// # Panics
	///
	/// Panics if `index` is out of bounds.
	pub fn swap_remove(&mut self, index: usize) -> T {
		self.0.swap_remove(index)
	}

	/// Exactly the same semantics as `Vec::retain`.
	pub fn retain<F: FnMut(&T) -> bool>(&mut self, f: F) {
		self.0.retain(f)
	}

	/// Exactly the same semantics as `slice::get_mut`.
	pub fn get_mut<I: SliceIndex<[T]>>(&mut self, index: I) -> Option<&mut <I as SliceIndex<[T]>>::Output> {
		self.0.get_mut(index)
	}

	/// Exactly the same semantics as `Vec::truncate`.
	///
	/// This is safe because `truncate` can never increase the length of the internal vector.
	pub fn truncate(&mut self, s: usize) {
		self.0.truncate(s);
	}

	/// Exactly the same semantics as `Vec::pop`.
	///
	/// This is safe since popping can only shrink the inner vector.
	pub fn pop(&mut self) -> Option<T> {
		self.0.pop()
	}

	/// Exactly the same semantics as [`slice::iter_mut`].
	pub fn iter_mut(&mut self) -> core::slice::IterMut<'_, T> {
		self.0.iter_mut()
	}

	/// Exactly the same semantics as [`slice::last_mut`].
	pub fn last_mut(&mut self) -> Option<&mut T> {
		self.0.last_mut()
	}

	/// Exact same semantics as [`Vec::drain`].
	pub fn drain<R>(&mut self, range: R) -> alloc::vec::Drain<'_, T>
	where
		R: RangeBounds<usize>,
	{
		self.0.drain(range)
	}
}

impl<T, S: Get<u32>> From<BoundedVec<T, S>> for Vec<T> {
	fn from(x: BoundedVec<T, S>) -> Vec<T> {
		x.0
	}
}

impl<T, S: Get<u32>> BoundedVec<T, S> {
	/// Pre-allocate `capacity` items in self.
	///
	/// If `capacity` is greater than [`Self::bound`], then the minimum of the two is used.
	pub fn with_bounded_capacity(capacity: usize) -> Self {
		let capacity = capacity.min(Self::bound());
		Self(Vec::with_capacity(capacity), Default::default())
	}

	/// Allocate self with the maximum possible capacity.
	pub fn with_max_capacity() -> Self {
		Self::with_bounded_capacity(Self::bound())
	}

	/// Consume and truncate the vector `v` in order to create a new instance of `Self` from it.
	pub fn truncate_from(mut v: Vec<T>) -> Self {
		v.truncate(Self::bound());
		Self::unchecked_from(v)
	}

	/// Get the bound of the type in `usize`.
	pub fn bound() -> usize {
		S::get() as usize
	}

	/// Returns true if this collection is full.
	pub fn is_full(&self) -> bool {
		self.len() >= Self::bound()
	}

	/// Forces the insertion of `element` into `self` retaining all items with index at least
	/// `index`.
	///
	/// If `index == 0` and `self.len() == Self::bound()`, then this is a no-op.
	///
	/// If `Self::bound() < index` or `self.len() < index`, then this is also a no-op.
	///
	/// Returns `Ok(maybe_removed)` if the item was inserted, where `maybe_removed` is
	/// `Some(removed)` if an item was removed to make room for the new one. Returns `Err(element)`
	/// if `element` cannot be inserted.
	pub fn force_insert_keep_right(&mut self, index: usize, mut element: T) -> Result<Option<T>, T> {
		// Check against panics.
		if Self::bound() < index || self.len() < index {
			Err(element)
		} else if self.len() < Self::bound() {
			// Cannot panic since self.len() >= index;
			self.0.insert(index, element);
			Ok(None)
		} else {
			if index == 0 {
				return Err(element)
			}
			core::mem::swap(&mut self[0], &mut element);
			// `[0..index] cannot panic since self.len() >= index.
			// `rotate_left(1)` cannot panic because there is at least 1 element.
			self[0..index].rotate_left(1);
			Ok(Some(element))
		}
	}

	/// Forces the insertion of `element` into `self` retaining all items with index at most
	/// `index`.
	///
	/// If `index == Self::bound()` and `self.len() == Self::bound()`, then this is a no-op.
	///
	/// If `Self::bound() < index` or `self.len() < index`, then this is also a no-op.
	///
	/// Returns `Ok(maybe_removed)` if the item was inserted, where `maybe_removed` is
	/// `Some(removed)` if an item was removed to make room for the new one. Returns `Err(element)`
	/// if `element` cannot be inserted.
	pub fn force_insert_keep_left(&mut self, index: usize, element: T) -> Result<Option<T>, T> {
		// Check against panics.
		if Self::bound() < index || self.len() < index || Self::bound() == 0 {
			return Err(element)
		}
		// Noop condition.
		if Self::bound() == index && self.len() <= Self::bound() {
			return Err(element)
		}
		let maybe_removed = if self.is_full() {
			// defensive-only: since we are at capacity, this is a noop.
			self.0.truncate(Self::bound());
			// if we truncate anything, it will be the last one.
			self.0.pop()
		} else {
			None
		};

		// Cannot panic since `self.len() >= index`;
		self.0.insert(index, element);
		Ok(maybe_removed)
	}

	/// Move the position of an item from one location to another in the slice.
	///
	/// Except for the item being moved, the order of the slice remains the same.
	///
	/// - `index` is the location of the item to be moved.
	/// - `insert_position` is the index of the item in the slice which should *immediately follow*
	///   the item which is being moved.
	///
	/// Returns `true` of the operation was successful, otherwise `false` if a noop.
	pub fn slide(&mut self, index: usize, insert_position: usize) -> bool {
		// Check against panics.
		if self.len() <= index || self.len() < insert_position || index == usize::MAX {
			return false
		}
		// Noop conditions.
		if index == insert_position || index + 1 == insert_position {
			return false
		}
		if insert_position < index && index < self.len() {
			// --- --- --- === === === === @@@ --- --- ---
			//            ^-- N            ^O^
			// ...
			//               /-----<<<-----\
			// --- --- --- === === === === @@@ --- --- ---
			//               >>> >>> >>> >>>
			// ...
			// --- --- --- @@@ === === === === --- --- ---
			//             ^N^
			self[insert_position..index + 1].rotate_right(1);
			return true
		} else if insert_position > 0 && index + 1 < insert_position {
			// Note that the apparent asymmetry of these two branches is due to the
			// fact that the "new" position is the position to be inserted *before*.
			// --- --- --- @@@ === === === === --- --- ---
			//             ^O^                ^-- N
			// ...
			//               /----->>>-----\
			// --- --- --- @@@ === === === === --- --- ---
			//               <<< <<< <<< <<<
			// ...
			// --- --- --- === === === === @@@ --- --- ---
			//                             ^N^
			self[index..insert_position].rotate_left(1);
			return true
		}

		debug_assert!(false, "all noop conditions should have been covered above");
		false
	}

	/// Forces the insertion of `s` into `self` truncating first if necessary.
	///
	/// Infallible, but if the bound is zero, then it's a no-op.
	pub fn force_push(&mut self, element: T) {
		if Self::bound() > 0 {
			self.0.truncate(Self::bound() as usize - 1);
			self.0.push(element);
		}
	}

	/// Same as `Vec::resize`, but if `size` is more than [`Self::bound`], then [`Self::bound`] is
	/// used.
	pub fn bounded_resize(&mut self, size: usize, value: T)
	where
		T: Clone,
	{
		let size = size.min(Self::bound());
		self.0.resize(size, value);
	}

	/// Exactly the same semantics as [`Vec::extend`], but returns an error and does nothing if the
	/// length of the outcome is larger than the bound.
	pub fn try_extend(&mut self, with: impl IntoIterator<Item = T> + ExactSizeIterator) -> Result<(), ()> {
		if with.len().saturating_add(self.len()) <= Self::bound() {
			self.0.extend(with);
			Ok(())
		} else {
			Err(())
		}
	}

	/// Exactly the same semantics as [`Vec::append`], but returns an error and does nothing if the
	/// length of the outcome is larger than the bound.
	pub fn try_append(&mut self, other: &mut Vec<T>) -> Result<(), ()> {
		if other.len().saturating_add(self.len()) <= Self::bound() {
			self.0.append(other);
			Ok(())
		} else {
			Err(())
		}
	}

	/// Consumes self and mutates self via the given `mutate` function.
	///
	/// If the outcome of mutation is within bounds, `Some(Self)` is returned. Else, `None` is
	/// returned.
	///
	/// This is essentially a *consuming* shorthand [`Self::into_inner`] -> `...` ->
	/// [`Self::try_from`].
	pub fn try_mutate(mut self, mut mutate: impl FnMut(&mut Vec<T>)) -> Option<Self> {
		mutate(&mut self.0);
		(self.0.len() <= Self::bound()).then(move || self)
	}

	/// Exactly the same semantics as [`Vec::insert`], but returns an `Err` (and is a noop) if the
	/// new length of the vector exceeds `S`.
	///
	/// # Panics
	///
	/// Panics if `index > len`.
	pub fn try_insert(&mut self, index: usize, element: T) -> Result<(), T> {
		if self.len() < Self::bound() {
			self.0.insert(index, element);
			Ok(())
		} else {
			Err(element)
		}
	}

	/// Exactly the same semantics as [`Vec::push`], but returns an `Err` (and is a noop) if the
	/// new length of the vector exceeds `S`.
	///
	/// # Panics
	///
	/// Panics if the new capacity exceeds isize::MAX bytes.
	pub fn try_push(&mut self, element: T) -> Result<(), T> {
		if self.len() < Self::bound() {
			self.0.push(element);
			Ok(())
		} else {
			Err(element)
		}
	}

	/// Exactly the same semantics as [`Vec::rotate_left`], but returns an `Err` (and is a noop) if `mid` is larger then the current length.
	pub fn try_rotate_left(&mut self, mid: usize) -> Result<(), ()> {
		if mid > self.len() {
			return Err(())
		}

		self.0.rotate_left(mid);
		Ok(())
	}

	/// Exactly the same semantics as [`Vec::rotate_right`], but returns an `Err` (and is a noop) if `mid` is larger then the current length.
	pub fn try_rotate_right(&mut self, mid: usize) -> Result<(), ()> {
		if mid > self.len() {
			return Err(())
		}

		self.0.rotate_right(mid);
		Ok(())
	}
}

impl<T, S> BoundedVec<T, S> {
	/// Return a [`BoundedSlice`] with the content and bound of [`Self`].
	pub fn as_bounded_slice(&self) -> BoundedSlice<T, S> {
		BoundedSlice(&self.0[..], PhantomData::default())
	}
}

impl<T, S> Default for BoundedVec<T, S> {
	fn default() -> Self {
		// the bound cannot be below 0, which is satisfied by an empty vector
		Self::unchecked_from(Vec::default())
	}
}

impl<T, S> core::fmt::Debug for BoundedVec<T, S>
where
	Vec<T>: core::fmt::Debug,
	S: Get<u32>,
{
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_tuple("BoundedVec").field(&self.0).field(&Self::bound()).finish()
	}
}

impl<T, S> Clone for BoundedVec<T, S>
where
	T: Clone,
{
	fn clone(&self) -> Self {
		// bound is retained
		Self::unchecked_from(self.0.clone())
	}
}

impl<T, S: Get<u32>> TryFrom<Vec<T>> for BoundedVec<T, S> {
	type Error = Vec<T>;
	fn try_from(t: Vec<T>) -> Result<Self, Self::Error> {
		if t.len() <= Self::bound() {
			// explicit check just above
			Ok(Self::unchecked_from(t))
		} else {
			Err(t)
		}
	}
}

impl<T, S: Get<u32>> TruncateFrom<Vec<T>> for BoundedVec<T, S> {
	fn truncate_from(unbound: Vec<T>) -> Self {
		BoundedVec::<T, S>::truncate_from(unbound)
	}
}

// Custom implementation of `Hash` since deriving it would require all generic bounds to also
// implement it.
#[cfg(feature = "std")]
impl<T: std::hash::Hash, S> std::hash::Hash for BoundedVec<T, S> {
	fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
		self.0.hash(state);
	}
}

// It is okay to give a non-mutable reference of the inner vec to anyone.
impl<T, S> AsRef<Vec<T>> for BoundedVec<T, S> {
	fn as_ref(&self) -> &Vec<T> {
		&self.0
	}
}

impl<T, S> AsRef<[T]> for BoundedVec<T, S> {
	fn as_ref(&self) -> &[T] {
		&self.0
	}
}

impl<T, S> AsMut<[T]> for BoundedVec<T, S> {
	fn as_mut(&mut self) -> &mut [T] {
		&mut self.0
	}
}

// will allow for all immutable operations of `Vec<T>` on `BoundedVec<T>`.
impl<T, S> Deref for BoundedVec<T, S> {
	type Target = Vec<T>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

// Allows for indexing similar to a normal `Vec`. Can panic if out of bound.
impl<T, S, I> Index<I> for BoundedVec<T, S>
where
	I: SliceIndex<[T]>,
{
	type Output = I::Output;

	#[inline]
	fn index(&self, index: I) -> &Self::Output {
		self.0.index(index)
	}
}

impl<T, S, I> IndexMut<I> for BoundedVec<T, S>
where
	I: SliceIndex<[T]>,
{
	#[inline]
	fn index_mut(&mut self, index: I) -> &mut Self::Output {
		self.0.index_mut(index)
	}
}

impl<T, S> core::iter::IntoIterator for BoundedVec<T, S> {
	type Item = T;
	type IntoIter = alloc::vec::IntoIter<T>;
	fn into_iter(self) -> Self::IntoIter {
		self.0.into_iter()
	}
}

impl<'a, T, S> core::iter::IntoIterator for &'a BoundedVec<T, S> {
	type Item = &'a T;
	type IntoIter = core::slice::Iter<'a, T>;
	fn into_iter(self) -> Self::IntoIter {
		self.0.iter()
	}
}

impl<'a, T, S> core::iter::IntoIterator for &'a mut BoundedVec<T, S> {
	type Item = &'a mut T;
	type IntoIter = core::slice::IterMut<'a, T>;
	fn into_iter(self) -> Self::IntoIter {
		self.0.iter_mut()
	}
}

impl<T, BoundSelf, BoundRhs> PartialEq<BoundedVec<T, BoundRhs>> for BoundedVec<T, BoundSelf>
where
	T: PartialEq,
	BoundSelf: Get<u32>,
	BoundRhs: Get<u32>,
{
	fn eq(&self, rhs: &BoundedVec<T, BoundRhs>) -> bool {
		self.0 == rhs.0
	}
}

impl<T, BoundSelf, BoundRhs> PartialEq<WeakBoundedVec<T, BoundRhs>> for BoundedVec<T, BoundSelf>
where
	T: PartialEq,
	BoundSelf: Get<u32>,
	BoundRhs: Get<u32>,
{
	fn eq(&self, rhs: &WeakBoundedVec<T, BoundRhs>) -> bool {
		self.0 == rhs.0
	}
}

impl<'a, T, BoundSelf, BoundRhs> PartialEq<BoundedSlice<'a, T, BoundRhs>> for BoundedVec<T, BoundSelf>
where
	T: PartialEq,
	BoundSelf: Get<u32>,
	BoundRhs: Get<u32>,
{
	fn eq(&self, rhs: &BoundedSlice<'a, T, BoundRhs>) -> bool {
		self.0 == rhs.0
	}
}

impl<'a, T: PartialEq, S: Get<u32>> PartialEq<&'a [T]> for BoundedSlice<'a, T, S> {
	fn eq(&self, other: &&'a [T]) -> bool {
		&self.0 == other
	}
}

impl<T: PartialEq, S: Get<u32>> PartialEq<Vec<T>> for BoundedVec<T, S> {
	fn eq(&self, other: &Vec<T>) -> bool {
		&self.0 == other
	}
}

impl<T, S: Get<u32>> Eq for BoundedVec<T, S> where T: Eq {}

impl<T, BoundSelf, BoundRhs> PartialOrd<BoundedVec<T, BoundRhs>> for BoundedVec<T, BoundSelf>
where
	T: PartialOrd,
	BoundSelf: Get<u32>,
	BoundRhs: Get<u32>,
{
	fn partial_cmp(&self, other: &BoundedVec<T, BoundRhs>) -> Option<core::cmp::Ordering> {
		self.0.partial_cmp(&other.0)
	}
}

impl<T, BoundSelf, BoundRhs> PartialOrd<WeakBoundedVec<T, BoundRhs>> for BoundedVec<T, BoundSelf>
where
	T: PartialOrd,
	BoundSelf: Get<u32>,
	BoundRhs: Get<u32>,
{
	fn partial_cmp(&self, other: &WeakBoundedVec<T, BoundRhs>) -> Option<core::cmp::Ordering> {
		self.0.partial_cmp(&other.0)
	}
}

impl<'a, T, BoundSelf, BoundRhs> PartialOrd<BoundedSlice<'a, T, BoundRhs>> for BoundedVec<T, BoundSelf>
where
	T: PartialOrd,
	BoundSelf: Get<u32>,
	BoundRhs: Get<u32>,
{
	fn partial_cmp(&self, other: &BoundedSlice<'a, T, BoundRhs>) -> Option<core::cmp::Ordering> {
		(&*self.0).partial_cmp(other.0)
	}
}

impl<T: Ord, Bound: Get<u32>> Ord for BoundedVec<T, Bound> {
	fn cmp(&self, other: &Self) -> core::cmp::Ordering {
		self.0.cmp(&other.0)
	}
}

impl<I, T, Bound> TryCollect<BoundedVec<T, Bound>> for I
where
	I: ExactSizeIterator + Iterator<Item = T>,
	Bound: Get<u32>,
{
	type Error = &'static str;

	fn try_collect(self) -> Result<BoundedVec<T, Bound>, Self::Error> {
		if self.len() > Bound::get() as usize {
			Err("iterator length too big")
		} else {
			Ok(BoundedVec::<T, Bound>::unchecked_from(self.collect::<Vec<T>>()))
		}
	}
}

#[cfg(any(feature = "scale-codec", feature = "jam-codec"))]
macro_rules! codec_impl {
	($codec:ident) => {
		use super::*;

		use $codec::{
			decode_vec_with_len, Compact, Decode, DecodeLength, DecodeWithMemTracking, Encode, EncodeLike, Error,
			Input, MaxEncodedLen,
		};

		impl<T: Decode, S: Get<u32>> Decode for BoundedVec<T, S> {
			fn decode<I: Input>(input: &mut I) -> Result<Self, Error> {
				// Same as the underlying implementation for `Decode` on `Vec`, except we fail early if the
				// len is too big.
				let len: u32 = <Compact<u32>>::decode(input)?.into();
				if len > S::get() {
					return Err("BoundedVec exceeds its limit".into());
				}
				let inner = decode_vec_with_len(input, len as usize)?;
				Ok(Self(inner, PhantomData))
			}

			fn skip<I: Input>(input: &mut I) -> Result<(), Error> {
				Vec::<T>::skip(input)
			}
		}

		impl<T: DecodeWithMemTracking, S: Get<u32>> DecodeWithMemTracking for BoundedVec<T, S> {}

		// `BoundedVec`s encode to something which will always decode as a `Vec`.
		impl<T: Encode + Decode, S: Get<u32>> EncodeLike<Vec<T>> for BoundedVec<T, S> {}

		impl<T, S> MaxEncodedLen for BoundedVec<T, S>
		where
			T: MaxEncodedLen,
			S: Get<u32>,
			BoundedVec<T, S>: Encode,
		{
			fn max_encoded_len() -> usize {
				// BoundedVec<T, S> encodes like Vec<T> which encodes like [T], which is a compact u32
				// plus each item in the slice:
				// See: https://docs.substrate.io/reference/scale-codec/
				Compact(S::get())
					.encoded_size()
					.saturating_add(Self::bound().saturating_mul(T::max_encoded_len()))
			}
		}

		impl<T, S> DecodeLength for BoundedVec<T, S> {
			fn len(self_encoded: &[u8]) -> Result<usize, Error> {
				// `BoundedVec<T, _>` stored just a `Vec<T>`, thus the length is at the beginning in
				// `Compact` form, and same implementation as `Vec<T>` can be used.
				<Vec<T> as DecodeLength>::len(self_encoded)
			}
		}

		// `BoundedSlice`s encode to something which will always decode into a `BoundedVec`,
		// `WeakBoundedVec`, or a `Vec`.
		impl<'a, T: Encode + Decode, S: Get<u32>> EncodeLike<BoundedVec<T, S>> for BoundedSlice<'a, T, S> {}

		impl<'a, T: Encode + Decode, S: Get<u32>> EncodeLike<WeakBoundedVec<T, S>> for BoundedSlice<'a, T, S> {}

		impl<'a, T: Encode + Decode, S: Get<u32>> EncodeLike<Vec<T>> for BoundedSlice<'a, T, S> {}
	};
}

#[cfg(feature = "scale-codec")]
mod scale_codec_impl {
	codec_impl!(scale_codec);
}

#[cfg(feature = "jam-codec")]
mod jam_codec_impl {
	codec_impl!(jam_codec);
}

#[cfg(all(test, feature = "std"))]
mod test {
	use super::*;
	use crate::{bounded_vec, ConstU32};
	#[cfg(feature = "scale-codec")]
	use scale_codec::{Compact, CompactLen, Decode, Encode};

	#[test]
	#[cfg(feature = "scale-codec")]
	fn encoding_same_as_unbounded_vec() {
		let b: BoundedVec<u32, ConstU32<6>> = bounded_vec![0, 1, 2, 3, 4, 5];
		let v: Vec<u32> = vec![0, 1, 2, 3, 4, 5];

		assert_eq!(b.encode(), v.encode());
	}

	#[test]
	fn slice_truncate_from_works() {
		let bounded = BoundedSlice::<u32, ConstU32<4>>::truncate_from(&[1, 2, 3, 4, 5]);
		assert_eq!(bounded.deref(), &[1, 2, 3, 4]);
		let bounded = BoundedSlice::<u32, ConstU32<4>>::truncate_from(&[1, 2, 3, 4]);
		assert_eq!(bounded.deref(), &[1, 2, 3, 4]);
		let bounded = BoundedSlice::<u32, ConstU32<4>>::truncate_from(&[1, 2, 3]);
		assert_eq!(bounded.deref(), &[1, 2, 3]);
	}

	#[test]
	fn slide_works() {
		let mut b: BoundedVec<u32, ConstU32<6>> = bounded_vec![0, 1, 2, 3, 4, 5];
		assert!(b.slide(1, 5));
		assert_eq!(*b, vec![0, 2, 3, 4, 1, 5]);
		assert!(b.slide(4, 0));
		assert_eq!(*b, vec![1, 0, 2, 3, 4, 5]);
		assert!(b.slide(0, 2));
		assert_eq!(*b, vec![0, 1, 2, 3, 4, 5]);
		assert!(b.slide(1, 6));
		assert_eq!(*b, vec![0, 2, 3, 4, 5, 1]);
		assert!(b.slide(0, 6));
		assert_eq!(*b, vec![2, 3, 4, 5, 1, 0]);
		assert!(b.slide(5, 0));
		assert_eq!(*b, vec![0, 2, 3, 4, 5, 1]);
		assert!(!b.slide(6, 0));
		assert!(!b.slide(7, 0));
		assert_eq!(*b, vec![0, 2, 3, 4, 5, 1]);

		let mut c: BoundedVec<u32, ConstU32<6>> = bounded_vec![0, 1, 2];
		assert!(!c.slide(1, 5));
		assert_eq!(*c, vec![0, 1, 2]);
		assert!(!c.slide(4, 0));
		assert_eq!(*c, vec![0, 1, 2]);
		assert!(!c.slide(3, 0));
		assert_eq!(*c, vec![0, 1, 2]);
		assert!(c.slide(2, 0));
		assert_eq!(*c, vec![2, 0, 1]);
	}

	#[test]
	fn slide_noops_work() {
		let mut b: BoundedVec<u32, ConstU32<6>> = bounded_vec![0, 1, 2, 3, 4, 5];
		assert!(!b.slide(3, 3));
		assert_eq!(*b, vec![0, 1, 2, 3, 4, 5]);
		assert!(!b.slide(3, 4));
		assert_eq!(*b, vec![0, 1, 2, 3, 4, 5]);
	}

	#[test]
	fn force_insert_keep_left_works() {
		let mut b: BoundedVec<u32, ConstU32<4>> = bounded_vec![];
		assert_eq!(b.force_insert_keep_left(1, 10), Err(10));
		assert!(b.is_empty());

		assert_eq!(b.force_insert_keep_left(0, 30), Ok(None));
		assert_eq!(b.force_insert_keep_left(0, 10), Ok(None));
		assert_eq!(b.force_insert_keep_left(1, 20), Ok(None));
		assert_eq!(b.force_insert_keep_left(3, 40), Ok(None));
		assert_eq!(*b, vec![10, 20, 30, 40]);
		// at capacity.
		assert_eq!(b.force_insert_keep_left(4, 41), Err(41));
		assert_eq!(*b, vec![10, 20, 30, 40]);
		assert_eq!(b.force_insert_keep_left(3, 31), Ok(Some(40)));
		assert_eq!(*b, vec![10, 20, 30, 31]);
		assert_eq!(b.force_insert_keep_left(1, 11), Ok(Some(31)));
		assert_eq!(*b, vec![10, 11, 20, 30]);
		assert_eq!(b.force_insert_keep_left(0, 1), Ok(Some(30)));
		assert_eq!(*b, vec![1, 10, 11, 20]);

		let mut z: BoundedVec<u32, ConstU32<0>> = bounded_vec![];
		assert!(z.is_empty());
		assert_eq!(z.force_insert_keep_left(0, 10), Err(10));
		assert!(z.is_empty());
	}

	#[test]
	fn force_insert_keep_right_works() {
		let mut b: BoundedVec<u32, ConstU32<4>> = bounded_vec![];
		assert_eq!(b.force_insert_keep_right(1, 10), Err(10));
		assert!(b.is_empty());

		assert_eq!(b.force_insert_keep_right(0, 30), Ok(None));
		assert_eq!(b.force_insert_keep_right(0, 10), Ok(None));
		assert_eq!(b.force_insert_keep_right(1, 20), Ok(None));
		assert_eq!(b.force_insert_keep_right(3, 40), Ok(None));
		assert_eq!(*b, vec![10, 20, 30, 40]);

		// at capacity.
		assert_eq!(b.force_insert_keep_right(0, 0), Err(0));
		assert_eq!(*b, vec![10, 20, 30, 40]);
		assert_eq!(b.force_insert_keep_right(1, 11), Ok(Some(10)));
		assert_eq!(*b, vec![11, 20, 30, 40]);
		assert_eq!(b.force_insert_keep_right(3, 31), Ok(Some(11)));
		assert_eq!(*b, vec![20, 30, 31, 40]);
		assert_eq!(b.force_insert_keep_right(4, 41), Ok(Some(20)));
		assert_eq!(*b, vec![30, 31, 40, 41]);

		assert_eq!(b.force_insert_keep_right(5, 69), Err(69));
		assert_eq!(*b, vec![30, 31, 40, 41]);

		let mut z: BoundedVec<u32, ConstU32<0>> = bounded_vec![];
		assert!(z.is_empty());
		assert_eq!(z.force_insert_keep_right(0, 10), Err(10));
		assert!(z.is_empty());
	}

	#[test]
	fn bound_returns_correct_value() {
		assert_eq!(BoundedVec::<u32, ConstU32<7>>::bound(), 7);
	}

	#[test]
	fn try_insert_works() {
		let mut bounded: BoundedVec<u32, ConstU32<4>> = bounded_vec![1, 2, 3];
		bounded.try_insert(1, 0).unwrap();
		assert_eq!(*bounded, vec![1, 0, 2, 3]);

		assert!(bounded.try_insert(0, 9).is_err());
		assert_eq!(*bounded, vec![1, 0, 2, 3]);
	}

	#[test]
	fn constructor_macro_works() {
		// With values. Use some brackets to make sure the macro doesn't expand.
		let bv: BoundedVec<(u32, u32), ConstU32<3>> = bounded_vec![(1, 2), (1, 2), (1, 2)];
		assert_eq!(bv, vec![(1, 2), (1, 2), (1, 2)]);

		// With repetition.
		let bv: BoundedVec<(u32, u32), ConstU32<3>> = bounded_vec![(1, 2); 3];
		assert_eq!(bv, vec![(1, 2), (1, 2), (1, 2)]);
	}

	#[test]
	#[should_panic(expected = "insertion index (is 9) should be <= len (is 3)")]
	fn try_inert_panics_if_oob() {
		let mut bounded: BoundedVec<u32, ConstU32<4>> = bounded_vec![1, 2, 3];
		bounded.try_insert(9, 0).unwrap();
	}

	#[test]
	fn try_push_works() {
		let mut bounded: BoundedVec<u32, ConstU32<4>> = bounded_vec![1, 2, 3];
		bounded.try_push(0).unwrap();
		assert_eq!(*bounded, vec![1, 2, 3, 0]);

		assert!(bounded.try_push(9).is_err());
	}

	#[test]
	fn deref_vec_coercion_works() {
		let bounded: BoundedVec<u32, ConstU32<7>> = bounded_vec![1, 2, 3];
		// these methods come from deref-ed vec.
		assert_eq!(bounded.len(), 3);
		assert!(bounded.iter().next().is_some());
		assert!(!bounded.is_empty());
	}

	#[test]
	fn deref_slice_coercion_works() {
		let bounded = BoundedSlice::<u32, ConstU32<7>>::try_from(&[1, 2, 3][..]).unwrap();
		// these methods come from deref-ed slice.
		assert_eq!(bounded.len(), 3);
		assert!(bounded.iter().next().is_some());
		assert!(!bounded.is_empty());
	}

	#[test]
	fn try_mutate_works() {
		let bounded: BoundedVec<u32, ConstU32<7>> = bounded_vec![1, 2, 3, 4, 5, 6];
		let bounded = bounded.try_mutate(|v| v.push(7)).unwrap();
		assert_eq!(bounded.len(), 7);
		assert!(bounded.try_mutate(|v| v.push(8)).is_none());
	}

	#[test]
	fn slice_indexing_works() {
		let bounded: BoundedVec<u32, ConstU32<7>> = bounded_vec![1, 2, 3, 4, 5, 6];
		assert_eq!(&bounded[0..=2], &[1, 2, 3]);
	}

	#[test]
	fn vec_eq_works() {
		let bounded: BoundedVec<u32, ConstU32<7>> = bounded_vec![1, 2, 3, 4, 5, 6];
		assert_eq!(bounded, vec![1, 2, 3, 4, 5, 6]);
	}

	#[test]
	#[cfg(feature = "scale-codec")]
	fn too_big_vec_fail_to_decode() {
		let v: Vec<u32> = vec![1, 2, 3, 4, 5];
		assert_eq!(
			BoundedVec::<u32, ConstU32<4>>::decode(&mut &v.encode()[..]),
			Err("BoundedVec exceeds its limit".into()),
		);
	}

	#[test]
	#[cfg(feature = "scale-codec")]
	fn dont_consume_more_data_than_bounded_len() {
		let v: Vec<u32> = vec![1, 2, 3, 4, 5];
		let data = v.encode();
		let data_input = &mut &data[..];

		BoundedVec::<u32, ConstU32<4>>::decode(data_input).unwrap_err();
		assert_eq!(data_input.len(), data.len() - Compact::<u32>::compact_len(&(data.len() as u32)));
	}

	#[test]
	fn eq_works() {
		// of same type
		let b1: BoundedVec<u32, ConstU32<7>> = bounded_vec![1, 2, 3];
		let b2: BoundedVec<u32, ConstU32<7>> = bounded_vec![1, 2, 3];
		assert_eq!(b1, b2);

		// of different type, but same value and bound.
		crate::parameter_types! {
			B1: u32 = 7;
			B2: u32 = 7;
		}
		let b1: BoundedVec<u32, B1> = bounded_vec![1, 2, 3];
		let b2: BoundedVec<u32, B2> = bounded_vec![1, 2, 3];
		assert_eq!(b1, b2);
	}

	#[test]
	fn ord_works() {
		use std::cmp::Ordering;
		let b1: BoundedVec<u32, ConstU32<7>> = bounded_vec![1, 2, 3];
		let b2: BoundedVec<u32, ConstU32<7>> = bounded_vec![1, 3, 2];

		// ordering for vec is lexicographic.
		assert_eq!(b1.cmp(&b2), Ordering::Less);
		assert_eq!(b1.cmp(&b2), b1.into_inner().cmp(&b2.into_inner()));
	}

	#[test]
	fn try_extend_works() {
		let mut b: BoundedVec<u32, ConstU32<5>> = bounded_vec![1, 2, 3];

		assert!(b.try_extend(vec![4].into_iter()).is_ok());
		assert_eq!(*b, vec![1, 2, 3, 4]);

		assert!(b.try_extend(vec![5].into_iter()).is_ok());
		assert_eq!(*b, vec![1, 2, 3, 4, 5]);

		assert!(b.try_extend(vec![6].into_iter()).is_err());
		assert_eq!(*b, vec![1, 2, 3, 4, 5]);

		let mut b: BoundedVec<u32, ConstU32<5>> = bounded_vec![1, 2, 3];
		assert!(b.try_extend(vec![4, 5].into_iter()).is_ok());
		assert_eq!(*b, vec![1, 2, 3, 4, 5]);

		let mut b: BoundedVec<u32, ConstU32<5>> = bounded_vec![1, 2, 3];
		assert!(b.try_extend(vec![4, 5, 6].into_iter()).is_err());
		assert_eq!(*b, vec![1, 2, 3]);
	}

	#[test]
	fn test_serializer() {
		let c: BoundedVec<u32, ConstU32<6>> = bounded_vec![0, 1, 2];
		assert_eq!(serde_json::json!(&c).to_string(), r#"[0,1,2]"#);
	}

	#[test]
	fn test_deserializer() {
		let c: BoundedVec<u32, ConstU32<6>> = serde_json::from_str(r#"[0,1,2]"#).unwrap();

		assert_eq!(c.len(), 3);
		assert_eq!(c[0], 0);
		assert_eq!(c[1], 1);
		assert_eq!(c[2], 2);
	}

	#[test]
	fn test_deserializer_bound() {
		let c: BoundedVec<u32, ConstU32<3>> = serde_json::from_str(r#"[0,1,2]"#).unwrap();

		assert_eq!(c.len(), 3);
		assert_eq!(c[0], 0);
		assert_eq!(c[1], 1);
		assert_eq!(c[2], 2);
	}

	#[test]
	fn test_deserializer_failed() {
		let c: Result<BoundedVec<u32, ConstU32<4>>, serde_json::error::Error> = serde_json::from_str(r#"[0,1,2,3,4]"#);

		match c {
			Err(msg) => assert_eq!(msg.to_string(), "out of bounds at line 1 column 11"),
			_ => unreachable!("deserializer must raise error"),
		}
	}

	#[test]
	fn bounded_vec_try_from_works() {
		assert!(BoundedVec::<u32, ConstU32<2>>::try_from(vec![0]).is_ok());
		assert!(BoundedVec::<u32, ConstU32<2>>::try_from(vec![0, 1]).is_ok());
		assert!(BoundedVec::<u32, ConstU32<2>>::try_from(vec![0, 1, 2]).is_err());
	}

	#[test]
	fn bounded_slice_try_from_works() {
		assert!(BoundedSlice::<u32, ConstU32<2>>::try_from(&[0][..]).is_ok());
		assert!(BoundedSlice::<u32, ConstU32<2>>::try_from(&[0, 1][..]).is_ok());
		assert!(BoundedSlice::<u32, ConstU32<2>>::try_from(&[0, 1, 2][..]).is_err());
	}

	#[test]
	fn can_be_collected() {
		let b1: BoundedVec<u32, ConstU32<5>> = bounded_vec![1, 2, 3, 4];
		let b2: BoundedVec<u32, ConstU32<5>> = b1.iter().map(|x| x + 1).try_collect().unwrap();
		assert_eq!(b2, vec![2, 3, 4, 5]);

		// can also be collected into a collection of length 4.
		let b2: BoundedVec<u32, ConstU32<4>> = b1.iter().map(|x| x + 1).try_collect().unwrap();
		assert_eq!(b2, vec![2, 3, 4, 5]);

		// can be mutated further into iterators that are `ExactSizedIterator`.
		let b2: BoundedVec<u32, ConstU32<4>> = b1.iter().map(|x| x + 1).rev().try_collect().unwrap();
		assert_eq!(b2, vec![5, 4, 3, 2]);

		let b2: BoundedVec<u32, ConstU32<4>> = b1.iter().map(|x| x + 1).rev().skip(2).try_collect().unwrap();
		assert_eq!(b2, vec![3, 2]);
		let b2: BoundedVec<u32, ConstU32<2>> = b1.iter().map(|x| x + 1).rev().skip(2).try_collect().unwrap();
		assert_eq!(b2, vec![3, 2]);

		let b2: BoundedVec<u32, ConstU32<4>> = b1.iter().map(|x| x + 1).rev().take(2).try_collect().unwrap();
		assert_eq!(b2, vec![5, 4]);
		let b2: BoundedVec<u32, ConstU32<2>> = b1.iter().map(|x| x + 1).rev().take(2).try_collect().unwrap();
		assert_eq!(b2, vec![5, 4]);

		// but these worn't work
		let b2: Result<BoundedVec<u32, ConstU32<3>>, _> = b1.iter().map(|x| x + 1).try_collect();
		assert!(b2.is_err());

		let b2: Result<BoundedVec<u32, ConstU32<1>>, _> = b1.iter().map(|x| x + 1).rev().take(2).try_collect();
		assert!(b2.is_err());
	}

	#[test]
	fn bounded_vec_debug_works() {
		let bound = BoundedVec::<u32, ConstU32<5>>::truncate_from(vec![1, 2, 3]);
		assert_eq!(format!("{:?}", bound), "BoundedVec([1, 2, 3], 5)");
	}

	#[test]
	fn bounded_slice_debug_works() {
		let bound = BoundedSlice::<u32, ConstU32<5>>::truncate_from(&[1, 2, 3]);
		assert_eq!(format!("{:?}", bound), "BoundedSlice([1, 2, 3], 5)");
	}

	#[test]
	fn bounded_vec_sort_by_key_works() {
		let mut v: BoundedVec<i32, ConstU32<5>> = bounded_vec![-5, 4, 1, -3, 2];
		// Sort by absolute value.
		v.sort_by_key(|k| k.abs());
		assert_eq!(v, vec![1, 2, -3, 4, -5]);
	}

	#[test]
	fn bounded_vec_truncate_from_works() {
		let unbound = vec![1, 2, 3, 4, 5];
		let bound = BoundedVec::<u32, ConstU32<3>>::truncate_from(unbound.clone());
		assert_eq!(bound, vec![1, 2, 3]);
	}

	#[test]
	fn bounded_slice_truncate_from_works() {
		let unbound = [1, 2, 3, 4, 5];
		let bound = BoundedSlice::<u32, ConstU32<3>>::truncate_from(&unbound);
		assert_eq!(bound, &[1, 2, 3][..]);
	}

	#[test]
	fn bounded_slice_partialeq_slice_works() {
		let unbound = [1, 2, 3];
		let bound = BoundedSlice::<u32, ConstU32<3>>::truncate_from(&unbound);

		assert_eq!(bound, &unbound[..]);
		assert!(bound == &unbound[..]);
	}

	#[test]
	fn bounded_vec_try_rotate_left_works() {
		let o = BoundedVec::<u32, ConstU32<3>>::truncate_from(vec![1, 2, 3]);
		let mut bound = o.clone();

		bound.try_rotate_left(0).unwrap();
		assert_eq!(bound, o);
		bound.try_rotate_left(3).unwrap();
		assert_eq!(bound, o);

		bound.try_rotate_left(4).unwrap_err();
		assert_eq!(bound, o);

		bound.try_rotate_left(1).unwrap();
		assert_eq!(bound, vec![2, 3, 1]);
		bound.try_rotate_left(2).unwrap();
		assert_eq!(bound, o);
	}

	#[test]
	fn bounded_vec_try_rotate_right_works() {
		let o = BoundedVec::<u32, ConstU32<3>>::truncate_from(vec![1, 2, 3]);
		let mut bound = o.clone();

		bound.try_rotate_right(0).unwrap();
		assert_eq!(bound, o);
		bound.try_rotate_right(3).unwrap();
		assert_eq!(bound, o);

		bound.try_rotate_right(4).unwrap_err();
		assert_eq!(bound, o);

		bound.try_rotate_right(1).unwrap();
		assert_eq!(bound, vec![3, 1, 2]);
		bound.try_rotate_right(2).unwrap();
		assert_eq!(bound, o);
	}

	// Just a test that structs containing `BoundedVec` and `BoundedSlice` can derive `Hash`. (This was broken when
	// they were deriving `Hash`).
	#[test]
	#[cfg(feature = "std")]
	fn container_can_derive_hash() {
		#[derive(Hash)]
		struct Foo<'a> {
			bar: u8,
			slice: BoundedSlice<'a, usize, ConstU32<4>>,
			map: BoundedVec<String, ConstU32<16>>,
		}
		let _foo = Foo { bar: 42, slice: BoundedSlice::truncate_from(&[0, 1][..]), map: BoundedVec::default() };
	}

	#[test]
	fn is_full_works() {
		let mut bounded: BoundedVec<u32, ConstU32<4>> = bounded_vec![1, 2, 3];
		assert!(!bounded.is_full());
		bounded.try_insert(1, 0).unwrap();
		assert_eq!(*bounded, vec![1, 0, 2, 3]);

		assert!(bounded.is_full());
		assert!(bounded.try_insert(0, 9).is_err());
		assert_eq!(*bounded, vec![1, 0, 2, 3]);
	}
}
