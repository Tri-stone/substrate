// Copyright 2017-2019 Parity Technologies (UK) Ltd.
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

//! # Indices Module
//!
//! To use the indices module, you need to implement the
//! [indices Trait](https://crates.parity.io/srml_indices/trait.Trait.html).
//!
//! ## Overview
//!
//! An index is a short form of an address. This module handles the allocation of indices for newly-created accounts
//! and provides lookup functions for matching an index with an account ID. When implemented with modules that handle
//! balance transfer, this will make remembering and entering addresses easier and less error-prone. 
//!
//! ### Terminology
//!
//! - **Account Index:** The short form of an address.
//! - **Account Id:** The public key of an address.
//! - **Reclaim:** The act of claiming a formerly-used index for a new account.
//!
//! ### Implementations
//!
//! The indices module provides implementations for the following traits. If these traits provide the functionality that
//! you need, then you can avoid coupling with the indices module.
//!
//! - [`OnNewAccount`](https://crates.parity.io/srml_system/trait.OnNewAccount.html): Provides the function to find the
//! first available index and assign a newly-created account to it.
//! - [`StaticLookup`](https://crates.parity.io/sr_primitives/traits/trait.StaticLookup.html): Means of changing one
//! type into another in a manner dependent on the source type. Does not (and cannot) require any context.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! The indices module does not implement any dispatchable functions.
//!
//! ### Public Functions
//!
//! See the [`Module`](https://crates.parity.io/srml_indices/struct.Module.html) for details on publicly available
//! functions.
//!
//! **Note:** When using the publicly exposed functions, you (the runtime developer) are responsible for implementing
//! any necessary checks before calling a function that will affect storage.
//!
//! ## Usage
//!
//! <!-- TODO -->
//!
//! ## Genesis config
//!
//! The module uses the following storage items in the genesis config:
//!
//! - [`NextEnumSet`](https://crates.parity.io/srml_indices/struct.NextEnumSet.html): The next free enumeration set.
//!
//! ## Related Modules
//!
//! The indices module depends on the [`system`](https://crates.parity.io/srml_system/index.html) and
//! [`srml_support`](https://crates.parity.io/srml_support/index.html) modules as well as Substrate Core libraries
//! and the Rust standard library.

#![cfg_attr(not(feature = "std"), no_std)]

use rstd::{prelude::*, result, marker::PhantomData};
use parity_codec::{Encode, Decode, Codec, Input, Output};
use srml_support::{StorageValue, StorageMap, Parameter, decl_module, decl_event, decl_storage};
use primitives::traits::{One, SimpleArithmetic, As, StaticLookup, Member};
use system::{IsDeadAccount, OnNewAccount};

use self::address::Address as RawAddress;

mod mock;

pub mod address;
mod tests;

/// Number of account IDs stored per enum set.
const ENUM_SET_SIZE: usize = 64;

pub type Address<T> = RawAddress<<T as system::Trait>::AccountId, <T as Trait>::AccountIndex>;

/// Turn an Id into an Index, or None for the purpose of getting
/// a hint at a possibly desired index.
pub trait ResolveHint<AccountId: Encode, AccountIndex: As<usize>> {
	/// Turn an Id into an Index, or None for the purpose of getting
	/// a hint at a possibly desired index.
	fn resolve_hint(who: &AccountId) -> Option<AccountIndex>;
}

/// Simple encode-based resolve hint implemenntation.
pub struct SimpleResolveHint<AccountId, AccountIndex>(PhantomData<(AccountId, AccountIndex)>);
impl<AccountId: Encode, AccountIndex: As<usize>> ResolveHint<AccountId, AccountIndex> for SimpleResolveHint<AccountId, AccountIndex> {
	fn resolve_hint(who: &AccountId) -> Option<AccountIndex> {
		Some(AccountIndex::sa(who.using_encoded(|e| e[0] as usize + e[1] as usize * 256)))
	}
}

/// The module's config trait.
pub trait Trait: system::Trait {
	/// Type used for storing an account's index; implies the maximum number of accounts the system
	/// can hold.
	type AccountIndex: Parameter + Member + Codec + Default + SimpleArithmetic + As<u8> + As<u16> + As<u32> + As<u64> + As<usize> + Copy;

	/// Whether an account is dead or not.
	type IsDeadAccount: IsDeadAccount<Self::AccountId>;

	/// How to turn an id into an index.
	type ResolveHint: ResolveHint<Self::AccountId, Self::AccountIndex>;

	/// The overarching event type.
	type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		fn deposit_event<T>() = default;
	}
}

decl_event!(
	pub enum Event<T> where
		<T as system::Trait>::AccountId,
		<T as Trait>::AccountIndex
	{
		/// A new account index was assigned.
		///
		/// This event is not triggered when an existing index is reassigned
		/// to another `AccountId`.
		NewAccountIndex(AccountId, AccountIndex),
	}
);

decl_storage! {
	trait Store for Module<T: Trait> as Indices {
		/// The next free enumeration set.
		pub NextEnumSet get(next_enum_set) build(|config: &GenesisConfig<T>| {
			T::AccountIndex::sa(config.ids.len() / ENUM_SET_SIZE)
		}): T::AccountIndex;

		/// The enumeration sets.
		pub EnumSet get(enum_set): map T::AccountIndex => Vec<T::AccountId>;
	}
	add_extra_genesis {
		config(ids): Vec<T::AccountId>;
		build(|storage: &mut primitives::StorageOverlay, _: &mut primitives::ChildrenStorageOverlay, config: &GenesisConfig<T>| {
			for i in 0..(config.ids.len() + ENUM_SET_SIZE - 1) / ENUM_SET_SIZE {
				storage.insert(GenesisConfig::<T>::hash(&<EnumSet<T>>::key_for(T::AccountIndex::sa(i))).to_vec(),
					config.ids[i * ENUM_SET_SIZE..config.ids.len().min((i + 1) * ENUM_SET_SIZE)].to_owned().encode());
			}
		});
	}
}

impl<T: Trait> Module<T> {
	// PUBLIC IMMUTABLES

	/// Look up an T::AccountIndex to get an Id, if there's one there.
	pub fn lookup_index(index: T::AccountIndex) -> Option<T::AccountId> {
		let enum_set_size = Self::enum_set_size();
		let set = Self::enum_set(index / enum_set_size);
		let i: usize = (index % enum_set_size).as_();
		set.get(i).cloned()
	}

	/// `true` if the account `index` is ready for reclaim.
	pub fn can_reclaim(try_index: T::AccountIndex) -> bool {
		let enum_set_size = Self::enum_set_size();
		let try_set = Self::enum_set(try_index / enum_set_size);
		let i = (try_index % enum_set_size).as_();
		i < try_set.len() && T::IsDeadAccount::is_dead_account(&try_set[i])
	}

	/// Look up an address to get an Id, if there's one there.
	pub fn lookup_address(a: address::Address<T::AccountId, T::AccountIndex>) -> Option<T::AccountId> {
		match a {
			address::Address::Id(i) => Some(i),
			address::Address::Index(i) => Self::lookup_index(i),
		}
	}

	fn enum_set_size() -> T::AccountIndex {
		T::AccountIndex::sa(ENUM_SET_SIZE)
	}
}

impl<T: Trait> OnNewAccount<T::AccountId> for Module<T> {
	fn on_new_account(who: &T::AccountId) {
		let enum_set_size = Self::enum_set_size();
		let next_set_index = Self::next_enum_set();

		if let Some(try_index) = T::ResolveHint::resolve_hint(who) {
			// then check to see if this account id identifies a dead account index.
			let set_index = try_index / enum_set_size;
			let mut try_set = Self::enum_set(set_index);
			let item_index = (try_index % enum_set_size).as_();
			if item_index < try_set.len() {
				if T::IsDeadAccount::is_dead_account(&try_set[item_index]) {
					// yup - this index refers to a dead account. can be reused.
					try_set[item_index] = who.clone();
					<EnumSet<T>>::insert(set_index, try_set);

					return
				}
			}
		}

		// insert normally as a back up
		let mut set_index = next_set_index;
		// defensive only: this loop should never iterate since we keep NextEnumSet up to date later.
		let mut set = loop {
			let set = Self::enum_set(set_index);
			if set.len() < ENUM_SET_SIZE {
				break set;
			}
			set_index += One::one();
		};

		let index = T::AccountIndex::sa(set_index.as_() * ENUM_SET_SIZE + set.len());

		// update set.
		set.push(who.clone());

		// keep NextEnumSet up to date
		if set.len() == ENUM_SET_SIZE {
			<NextEnumSet<T>>::put(set_index + One::one());
		}

		// write set.
		<EnumSet<T>>::insert(set_index, set);

		Self::deposit_event(RawEvent::NewAccountIndex(who.clone(), index));
	}
}

impl<T: Trait> StaticLookup for Module<T> {
	type Source = address::Address<T::AccountId, T::AccountIndex>;
	type Target = T::AccountId;
	fn lookup(a: Self::Source) -> result::Result<Self::Target, &'static str> {
		Self::lookup_address(a).ok_or("invalid account index")
	}
	fn unlookup(a: Self::Target) -> Self::Source {
		address::Address::Id(a)
	}
}
