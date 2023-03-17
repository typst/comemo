use super::*;

/// Make a type trackable.
pub fn expand(item: &syn::Item) -> Result<TokenStream> {
    // Preprocess and validate the methods.
    let mut methods = vec![];

    let (ty, trait_) = match item {
        syn::Item::Impl(item) => {
            for param in item.generics.params.iter() {
                bail!(param, "tracked impl blocks cannot be generic")
            }

            for item in &item.items {
                methods.push(prepare_impl_method(&item)?);
            }

            let ty = item.self_ty.as_ref().clone();
            (ty, None)
        }
        syn::Item::Trait(item) => {
            for param in item.generics.params.iter() {
                bail!(param, "tracked traits cannot be generic")
            }

            for item in &item.items {
                methods.push(prepare_trait_method(&item)?);
            }

            let name = &item.ident;
            let ty = parse_quote! { dyn #name };
            (ty, Some(name.clone()))
        }
        _ => bail!(
            item,
            "`track` can only be applied to impl blocks and traits"
        ),
    };

    // Produce the necessary items for the type to become trackable.
    let scope = create(&ty, trait_, &methods)?;

    Ok(quote! {
        #item
        const _: () = { #scope };
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
    let syn::ImplItem::Method(method) = item else {
        bail!(item, "only methods can be tracked");
    };

    prepare_method(method.vis.clone(), &method.sig)
}

/// Preprocess and validate a method in a trait.
fn prepare_trait_method(item: &syn::TraitItem) -> Result<Method> {
    let syn::TraitItem::Method(method) = item else {
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
        }) = typed.pat.as_ref() else {
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

/// Produce the necessary items for a type to become trackable.
fn create(
    ty: &syn::Type,
    trait_: Option<syn::Ident>,
    methods: &[Method],
) -> Result<TokenStream> {
    let prefix = trait_.map(|name| quote! { #name for });
    let variants = methods.iter().map(create_variant);
    let validations = methods.iter().map(create_validation);
    let replays = methods.iter().map(create_replay);
    let wrapper_methods = methods
        .iter()
        .filter(|m| !m.mutable)
        .map(|m| create_wrapper(m, false));
    let wrapper_methods_mut = methods.iter().map(|m| create_wrapper(m, true));
    let maybe_cloned = if methods.iter().any(|it| it.mutable) {
        quote! { ::std::clone::Clone::clone(self) }
    } else {
        quote! { self }
    };

    Ok(quote! {
        #[derive(Clone, PartialEq)]
        pub struct __ComemoCall(__ComemoVariant);

        #[derive(Clone, PartialEq)]
        #[allow(non_camel_case_types)]
        enum __ComemoVariant {
            #(#variants,)*
        }

        impl ::comemo::Track for #ty {
            #[inline]
            fn valid(&self, constraint: &::comemo::Constraint<Self>) -> bool {
                let mut this = #maybe_cloned;
                constraint.valid(|call| match &call.0 { #(#validations,)* })
            }
        }

        #[doc(hidden)]
        impl ::comemo::internal::Trackable for #ty {
            type Call = __ComemoCall;
            type Surface = __ComemoSurfaceFamily;
            type SurfaceMut = __ComemoSurfaceMutFamily;

            #[inline]
            #[allow(unused_variables)]
            fn replay(&mut self, constraint: &::comemo::Constraint<Self>) {
                constraint.replay(|call| match &call.0 { #(#replays,)* });
            }

            #[inline]
            fn surface_ref<'a, 'r>(
                tracked: &'r ::comemo::Tracked<'a, Self>,
            ) -> &'r __ComemoSurface<'a> {
                // Safety: __ComemoSurface is repr(transparent).
                unsafe { &*(tracked as *const _ as *const _) }
            }

            #[inline]
            fn surface_mut_ref<'a, 'r>(
                tracked: &'r ::comemo::TrackedMut<'a, Self>,
            ) -> &'r __ComemoSurfaceMut<'a> {
                // Safety: __ComemoSurfaceMut is repr(transparent).
                unsafe { &*(tracked as *const _ as *const _) }
            }

            #[inline]
            fn surface_mut_mut<'a, 'r>(
                tracked: &'r mut ::comemo::TrackedMut<'a, Self>,
            ) -> &'r mut __ComemoSurfaceMut<'a> {
                // Safety: __ComemoSurfaceMut is repr(transparent).
                unsafe { &mut *(tracked as *mut _ as *mut _) }
            }
        }

        #[repr(transparent)]
        pub struct __ComemoSurface<'a>(::comemo::Tracked<'a, #ty>);

        #[allow(dead_code)]
        impl #prefix __ComemoSurface<'_> {
            #(#wrapper_methods)*
        }

        pub enum __ComemoSurfaceFamily {}
        impl<'a> ::comemo::internal::Family<'a> for __ComemoSurfaceFamily {
            type Out = __ComemoSurface<'a>;
        }

        #[repr(transparent)]
        pub struct __ComemoSurfaceMut<'a>(::comemo::TrackedMut<'a, #ty>);

        #[allow(dead_code)]
        impl #prefix __ComemoSurfaceMut<'_> {
            #(#wrapper_methods_mut)*
        }

        pub enum __ComemoSurfaceMutFamily {}
        impl<'a> ::comemo::internal::Family<'a> for __ComemoSurfaceMutFamily {
            type Out = __ComemoSurfaceMut<'a>;
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
    let mutable = method.mutable;
    let to_parts = if !tracked_mut {
        quote! { to_parts_ref(self.0) }
    } else if !mutable {
        quote! { to_parts_mut_ref(&self.0) }
    } else {
        quote! { to_parts_mut_mut(&mut self.0) }
    };
    quote! {
        #[track_caller]
        #[inline]
        #vis #sig {
            let call = __ComemoVariant::#name(#(#args.to_owned()),*);
            let (value, constraint) = ::comemo::internal::#to_parts;
            let output = value.#name(#(#args,)*);
            if let Some(constraint) = constraint {
                constraint.push(
                    __ComemoCall(call),
                    ::comemo::internal::hash(&output),
                    #mutable,
                );
            }
            output
        }
    }
}
