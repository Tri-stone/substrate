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
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>

//! Util function used by this crate.

use proc_macro2::{TokenStream, Span};

use syn::{Ident, Error, Signature, Pat, PatType, FnArg, Type, token};

use proc_macro_crate::crate_name;

use std::env;

use quote::quote;

use inflector::Inflector;

/// Generates the include for the runtime-interface crate.
pub fn generate_runtime_interface_include() -> TokenStream {
	if env::var("CARGO_PKG_NAME").unwrap() == "substrate-runtime-interface" {
		TokenStream::new()
	} else {
		match crate_name("substrate-runtime-interface") {
			Ok(crate_name) => {
				let crate_name = Ident::new(&crate_name, Span::call_site());
				quote!(
					#[doc(hidden)]
					extern crate #crate_name as proc_macro_runtime_interface;
				)
			},
			Err(e) => {
				let err = Error::new(Span::call_site(), &e).to_compile_error();
				quote!( #err )
			}
		}
	}
}

/// Generates the access to the `substrate-runtime-interface` crate.
pub fn generate_crate_access() -> TokenStream {
	if env::var("CARGO_PKG_NAME").unwrap() == "substrate-runtime-interface" {
		quote!( crate )
	} else {
		quote!( proc_macro_runtime_interface )
	}
}

/// Create the host function identifier for the given function name.
pub fn create_host_function_ident(name: &Ident, trait_name: &Ident) -> Ident {
	Ident::new(
		&format!(
			"ext_{}_{}",
			trait_name.to_string().to_snake_case(),
			name,
		),
		Span::call_site(),
	)
}

/// Returns the function arguments of the given `Signature`, minus any `self` arguments.
pub fn get_function_arguments<'a>(sig: &'a Signature) -> impl Iterator<Item = &'a PatType> {
	sig.inputs
		.iter()
		.filter_map(|a| match a {
			FnArg::Receiver(_) => None,
			FnArg::Typed(pat_type) => Some(pat_type),
		})
}

/// Returns the function argument names of the given `Signature`, minus any `self`.
pub fn get_function_argument_names<'a>(sig: &'a Signature) -> impl Iterator<Item = &'a Box<Pat>> {
	get_function_arguments(sig).map(|pt| &pt.pat)
}

/// Returns the function argument types, minus any `Self` type. If any of the arguments
/// is a reference, the underlying type without the ref is returned.
pub fn get_function_argument_types_without_ref<'a>(
	sig: &'a Signature,
) -> impl Iterator<Item = &'a Box<Type>> {
	get_function_arguments(sig)
		.map(|pt| &pt.ty)
		.map(|ty| match &**ty {
			Type::Reference(type_ref) => &type_ref.elem,
			_ => ty,
		})
}

/// Returns the function argument names and types, minus any `self`. If any of the arguments
/// is a reference, the underlying type without the ref is returned.
pub fn get_function_argument_names_and_types_without_ref<'a>(
	sig: &'a Signature,
) -> impl Iterator<Item = (&'a Box<Pat>, &'a Box<Type>)> {
	get_function_arguments(sig)
		.map(|pt| match &*pt.ty {
			Type::Reference(type_ref) => (&pt.pat, &type_ref.elem),
			_ => (&pt.pat, &pt.ty),
		})
}

/// Returns the `&`/`&mut` for all function argument types, minus the `self` arg. If a function
/// argument is not a reference, `None` is returned.
pub fn get_function_argument_types_ref_and_mut<'a>(
	sig: &'a Signature,
) -> impl Iterator<Item = Option<(&'a token::And, Option<&'a token::Mut>)>> {
	get_function_arguments(sig)
		.map(|pt| &pt.ty)
		.map(|ty| match &**ty {
			Type::Reference(type_ref) => Some((&type_ref.and_token, type_ref.mutability.as_ref())),
			_ => None,
		})
}