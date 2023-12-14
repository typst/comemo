use super::*;

/// Make a type trackable.
pub fn expand(item: &syn::Item) -> Result<TokenStream> {
    // Preprocess and validate the methods.
    let mut methods = vec![];

    let (ty, generics, trait_) = match item {
        syn::Item::Impl(item) => {
            for param in item.generics.params.iter() {
                match param {
                    syn::GenericParam::Lifetime(_) => {}
                    syn::GenericParam::Type(_) => {
                        bail!(param, "tracked impl blocks cannot use type generics")
                    }
                    syn::GenericParam::Const(_) => {
                        bail!(param, "tracked impl blocks cannot use const generics")
                    }
                }
            }

            for item in &item.items {
                methods.push(prepare_impl_method(item)?);
            }

            let ty = item.self_ty.as_ref().clone();
            (ty, &item.generics, None)
        }
        syn::Item::Trait(item) => {
            if let Some(first) = item.generics.params.first() {
                bail!(first, "tracked traits cannot be generic")
            }

            for item in &item.items {
                methods.push(prepare_trait_method(item)?);
            }

            let name = &item.ident;
            let ty = parse_quote! { dyn #name + '__comemo_dynamic };
            (ty, &item.generics, Some(item.ident.clone()))
        }
        _ => bail!(item, "`track` can only be applied to impl blocks and traits"),
    };

    // Produce the necessary items for the type to become trackable.
    let variants = create_variants(&methods);
    let scope = create(&ty, generics, trait_, &methods)?;

    Ok(quote! {
        #item
        const _: () = {
            #variants
            #scope
        };
    })
}

/// Details about a method that should be tracked.
struct Method {
    vis: syn::Visibility,
    sig: syn::Signature,
    mutable: bool,
    args: Vec<syn::Ident>,
    types: Vec<syn::Type>,
    kinds: Vec<Kind>,
}

/// Whether an argument to a tracked method is bare or by reference.
enum Kind {
    Normal,
    Reference,
}

/// Preprocess and validate a method in an impl block.
fn prepare_impl_method(item: &syn::ImplItem) -> Result<Method> {
    let syn::ImplItem::Fn(method) = item else {
        bail!(item, "only methods can be tracked");
    };

    prepare_method(method.vis.clone(), &method.sig)
}

/// Preprocess and validate a method in a trait.
fn prepare_trait_method(item: &syn::TraitItem) -> Result<Method> {
    let syn::TraitItem::Fn(method) = item else {
        bail!(item, "only methods can be tracked");
    };

    prepare_method(syn::Visibility::Inherited, &method.sig)
}

/// Preprocess and validate a method signature.
fn prepare_method(vis: syn::Visibility, sig: &syn::Signature) -> Result<Method> {
    if let Some(unsafety) = sig.unsafety {
        bail!(unsafety, "unsafe methods cannot be tracked");
    }

    if let Some(asyncness) = sig.asyncness {
        bail!(asyncness, "async methods cannot be tracked");
    }

    if let Some(constness) = sig.constness {
        bail!(constness, "const methods cannot be tracked");
    }

    for param in sig.generics.params.iter() {
        match param {
            syn::GenericParam::Const(_) | syn::GenericParam::Type(_) => {
                bail!(param, "tracked method cannot be generic")
            }
            syn::GenericParam::Lifetime(_) => {}
        }
    }

    let mut inputs = sig.inputs.iter();
    let Some(syn::FnArg::Receiver(receiver)) = inputs.next() else {
        bail!(sig, "tracked method must take self");
    };

    if receiver.reference.is_none() {
        bail!(receiver, "tracked method must take self by reference");
    }

    let mut args = vec![];
    let mut types = vec![];
    let mut kinds = vec![];

    for input in inputs {
        let typed = match input {
            syn::FnArg::Typed(typed) => typed,
            syn::FnArg::Receiver(_) => continue,
        };

        let syn::Pat::Ident(syn::PatIdent {
            by_ref: None,
            mutability: None,
            ident,
            subpat: None,
            ..
        }) = typed.pat.as_ref()
        else {
            bail!(typed.pat, "only simple identifiers are supported");
        };

        let (ty, kind) = match typed.ty.as_ref() {
            syn::Type::ImplTrait(ty) => {
                bail!(ty, "tracked methods cannot be generic");
            }
            syn::Type::Reference(syn::TypeReference { mutability, elem, .. }) => {
                if mutability.is_some() {
                    bail!(typed.ty, "tracked methods cannot have mutable parameters");
                } else {
                    (elem.as_ref().clone(), Kind::Reference)
                }
            }
            ty => (ty.clone(), Kind::Normal),
        };

        args.push(ident.clone());
        types.push(ty);
        kinds.push(kind)
    }

    if let syn::ReturnType::Type(_, ty) = &sig.output {
        if let syn::Type::Reference(syn::TypeReference { mutability, .. }) = ty.as_ref() {
            if mutability.is_some() {
                bail!(ty, "tracked methods cannot return mutable references");
            }
        }
    }

    Ok(Method {
        vis,
        sig: sig.clone(),
        mutable: receiver.mutability.is_some(),
        args,
        types,
        kinds,
    })
}

/// Produces the variants for the constraint.
fn create_variants(methods: &[Method]) -> TokenStream {
    let variants = methods.iter().map(create_variant);
    let is_mutable_variants = methods.iter().map(|m| {
        let name = &m.sig.ident;
        let mutable = m.mutable;
        quote! { __ComemoVariant::#name(..) => #mutable }
    });

    let is_mutable = (!methods.is_empty())
        .then(|| {
            quote! {
                match &self.0 {
                    #(#is_mutable_variants),*
                }
            }
        })
        .unwrap_or_else(|| quote! { false });

    quote! {
        #[derive(Clone, PartialEq, Hash)]
        pub struct __ComemoCall(__ComemoVariant);

        impl ::comemo::internal::Call for __ComemoCall {
            fn is_mutable(&self) -> bool {
                #is_mutable
            }
        }

        #[derive(Clone, PartialEq, Hash)]
        #[allow(non_camel_case_types)]
        enum __ComemoVariant {
            #(#variants,)*
        }
    }
}

/// Produce the necessary items for a type to become trackable.
fn create(
    ty: &syn::Type,
    generics: &syn::Generics,
    trait_: Option<syn::Ident>,
    methods: &[Method],
) -> Result<TokenStream> {
    let t: syn::GenericParam = parse_quote! { '__comemo_tracked };
    let r: syn::GenericParam = parse_quote! { '__comemo_retrack };
    let d: syn::GenericParam = parse_quote! { '__comemo_dynamic };
    let maybe_cloned = if methods.iter().any(|it| it.mutable) {
        quote! { ::core::clone::Clone::clone(self) }
    } else {
        quote! { self }
    };

    // Prepare generics.
    let (impl_gen, type_gen, where_clause) = generics.split_for_impl();
    let mut impl_params: syn::Generics = parse_quote! { #impl_gen };
    let mut type_params: syn::Generics = parse_quote! { #type_gen };
    if trait_.is_some() {
        impl_params.params.push(d.clone());
        type_params.params.push(d.clone());
    }

    let mut impl_params_t: syn::Generics = impl_params.clone();
    let mut type_params_t: syn::Generics = type_params.clone();
    impl_params_t.params.push(t.clone());
    type_params_t.params.push(t.clone());

    // Prepare validations.
    let prefix = trait_.as_ref().map(|name| quote! { #name for });
    let validations: Vec<_> = methods.iter().map(create_validation).collect();
    let validate = if !methods.is_empty() {
        quote! {
            let mut this = #maybe_cloned;
            constraint.validate(|call| match &call.0 { #(#validations,)* })
        }
    } else {
        quote! { true }
    };
    let validate_with_id = if !methods.is_empty() {
        quote! {
            let mut this = #maybe_cloned;
            constraint.validate_with_id(
                |call| match &call.0 { #(#validations,)* },
                id,
            )
        }
    } else {
        quote! { true }
    };

    // Prepare replying.
    let immutable = methods.iter().all(|m| !m.mutable);
    let replays = methods.iter().map(create_replay);
    let replay = (!immutable).then(|| {
        quote! {
            constraint.replay(|call| match &call.0 { #(#replays,)* });
        }
    });

    // Prepare variants and wrapper methods.
    let wrapper_methods = methods
        .iter()
        .filter(|m| !m.mutable)
        .map(|m| create_wrapper(m, false));
    let wrapper_methods_mut = methods.iter().map(|m| create_wrapper(m, true));

    let constraint = if immutable {
        quote! { ImmutableConstraint }
    } else {
        quote! { MutableConstraint }
    };

    Ok(quote! {
        impl #impl_params ::comemo::Track for #ty #where_clause {}

        impl #impl_params ::comemo::Validate for #ty #where_clause {
            type Constraint = ::comemo::internal::#constraint<__ComemoCall>;

            #[inline]
            fn validate(&self, constraint: &Self::Constraint) -> bool {
                #validate
            }

            #[inline]
            fn validate_with_id(&self, constraint: &Self::Constraint, id: usize) -> bool {
                #validate_with_id
            }

            #[inline]
            #[allow(unused_variables)]
            fn replay(&mut self, constraint: &Self::Constraint) {
                #replay
            }
        }

        #[doc(hidden)]
        impl #impl_params ::comemo::internal::Surfaces for #ty  #where_clause {
            type Surface<#t> = __ComemoSurface #type_params_t where Self: #t;
            type SurfaceMut<#t> = __ComemoSurfaceMut #type_params_t where Self: #t;

            #[inline]
            fn surface_ref<#t, #r>(
                tracked: &#r ::comemo::Tracked<#t, Self>,
            ) -> &#r Self::Surface<#t> {
                // Safety: __ComemoSurface is repr(transparent).
                unsafe { &*(tracked as *const _ as *const _) }
            }

            #[inline]
            fn surface_mut_ref<#t, #r>(
                tracked: &#r ::comemo::TrackedMut<#t, Self>,
            ) -> &#r Self::SurfaceMut<#t> {
                // Safety: __ComemoSurfaceMut is repr(transparent).
                unsafe { &*(tracked as *const _ as *const _) }
            }

            #[inline]
            fn surface_mut_mut<#t, #r>(
                tracked: &#r mut ::comemo::TrackedMut<#t, Self>,
            ) -> &#r mut Self::SurfaceMut<#t> {
                // Safety: __ComemoSurfaceMut is repr(transparent).
                unsafe { &mut *(tracked as *mut _ as *mut _) }
            }
        }

        #[repr(transparent)]
        pub struct __ComemoSurface #impl_params_t(::comemo::Tracked<#t, #ty>)
        #where_clause;

        #[allow(dead_code)]
        impl #impl_params_t #prefix __ComemoSurface #type_params_t {
            #(#wrapper_methods)*
        }

        #[repr(transparent)]
        pub struct __ComemoSurfaceMut #impl_params_t(::comemo::TrackedMut<#t, #ty>)
        #where_clause;

        #[allow(dead_code)]
        impl #impl_params_t #prefix __ComemoSurfaceMut #type_params_t {
            #(#wrapper_methods_mut)*
        }
    })
}

/// Produce a constraint validation for a method.
fn create_variant(method: &Method) -> TokenStream {
    let name = &method.sig.ident;
    let types = &method.types;
    quote! { #name(#(<#types as ::std::borrow::ToOwned>::Owned),*) }
}

/// Produce a constraint validation for a method.
fn create_validation(method: &Method) -> TokenStream {
    let name = &method.sig.ident;
    let args = &method.args;
    let prepared = method.args.iter().zip(&method.kinds).map(|(arg, kind)| match kind {
        Kind::Normal => quote! { #arg.to_owned() },
        Kind::Reference => quote! { #arg },
    });
    quote! {
        __ComemoVariant::#name(#(#args),*)
            => ::comemo::internal::hash(&this.#name(#(#prepared),*))
    }
}

/// Produce a constraint validation for a method.
fn create_replay(method: &Method) -> TokenStream {
    let name = &method.sig.ident;
    let args = &method.args;
    let prepared = method.args.iter().zip(&method.kinds).map(|(arg, kind)| match kind {
        Kind::Normal => quote! { #arg.to_owned() },
        Kind::Reference => quote! { #arg },
    });
    let body = method.mutable.then(|| {
        quote! {
            self.#name(#(#prepared),*);
        }
    });
    quote! { __ComemoVariant::#name(#(#args),*) => { #body } }
}

/// Produce a wrapped surface method.
fn create_wrapper(method: &Method, tracked_mut: bool) -> TokenStream {
    let name = &method.sig.ident;
    let vis = &method.vis;
    let sig = &method.sig;
    let args = &method.args;
    let to_parts = if !tracked_mut {
        quote! { to_parts_ref(self.0) }
    } else if !method.mutable {
        quote! { to_parts_mut_ref(&self.0) }
    } else {
        quote! { to_parts_mut_mut(&mut self.0) }
    };
    quote! {
        #[track_caller]
        #[inline]
        #vis #sig {
            let __comemo_variant = __ComemoVariant::#name(#(#args.to_owned()),*);
            let (__comemo_value, __comemo_constraint) = ::comemo::internal::#to_parts;
            let output = __comemo_value.#name(#(#args,)*);
            if let Some(constraint) = __comemo_constraint {
                constraint.push(
                    __ComemoCall(__comemo_variant),
                    ::comemo::internal::hash(&output),
                );
            }
            output
        }
    }
}
