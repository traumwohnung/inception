use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::{
    Attribute, Error, Expr, Fields, Ident, ItemEnum, LitStr, Member, Path, Result, Token,
    parse::Parser, parse_macro_input, punctuated::Punctuated,
};

/// Define an inception error.
///
/// The input enum is transformed into a location-bearing error wrapper and an
/// exhaustive `<ErrorName>Kind` enum. A variant may provide a stable
/// `#[code("...")]`; otherwise `Enum.Variant` is used. A variant may provide
/// a static `#[description("...")]`; otherwise its name is used. A field marked
/// `#[caused_by]` is an external source;
/// `#[caused_by(inception)]` marks a nested inception error.
#[proc_macro_attribute]
pub fn error(arguments: TokenStream, input: TokenStream) -> TokenStream {
    let serialize = if arguments.is_empty() {
        false
    } else {
        match syn::parse::<Ident>(arguments) {
            Ok(argument) if argument == "serde" => true,
            Ok(_) => {
                return Error::new(
                    Span::call_site(),
                    "#[inception::error] accepts only the `serde` option",
                )
                .to_compile_error()
                .into();
            }
            Err(error) => return error.into_compile_error().into(),
        }
    };
    let item = parse_macro_input!(input as ItemEnum);
    expand_error(item, serialize)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// Construct an Inception layer and capture this invocation's coordinates.
#[proc_macro]
pub fn locate(input: TokenStream) -> TokenStream {
    let expression = parse_macro_input!(input as Expr);
    expand_constructor(expression)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// Turn a fallible operation's error into an Inceptioned external cause.
#[proc_macro]
pub fn locate_err(input: TokenStream) -> TokenStream {
    let expression = parse_macro_input!(input as Expr);
    let Expr::Path(path) = expression else {
        return Error::new(
            Span::call_site(),
            "locate_err! expects a qualified `Error::Variant` path",
        )
        .to_compile_error()
        .into();
    };
    let mut error_path = path.path;
    let Some(variant) = error_path
        .segments
        .pop()
        .map(|pair| pair.into_value().ident)
    else {
        return Error::new(
            Span::call_site(),
            "locate_err! expects a qualified Error::Variant path",
        )
        .to_compile_error()
        .into();
    };
    if error_path.segments.is_empty() {
        return Error::new(
            Span::call_site(),
            "locate_err! expects a qualified Error::Variant path",
        )
        .to_compile_error()
        .into();
    }
    let method = locate_err_method_name(&variant);
    error_path.segments.push(method.into());
    quote! {
        |source| {
            #error_path(
                ::inception::Inceptioned::new_at(
                    source,
                    ::inception::Location::new(
                        ::core::file!(),
                        ::core::line!(),
                        ::core::column!(),
                    ),
                ),
                ::inception::Location::new(
                    ::core::file!(),
                    ::core::line!(),
                    ::core::column!(),
                ),
            )
        }
    }
    .into()
}

/// Return an inception from the current function.
#[proc_macro]
pub fn bail(input: TokenStream) -> TokenStream {
    let expression = parse_macro_input!(input as Expr);
    match expand_constructor(expression) {
        Ok(error) => quote!(return ::core::result::Result::Err(#error)).into(),
        Err(error) => error.into_compile_error().into(),
    }
}

/// Return an ordinary error as a located Inceptioned layer.
#[proc_macro]
pub fn bail_err(input: TokenStream) -> TokenStream {
    let expression = parse_macro_input!(input as Expr);
    quote! {
        return ::core::result::Result::Err(::inception::Inceptioned::new_at(
            #expression,
            ::inception::Location::new(::core::file!(), ::core::line!(), ::core::column!()),
        ))
    }
    .into()
}

#[allow(clippy::too_many_lines)]
fn expand_error(mut item: ItemEnum, serialize: bool) -> Result<proc_macro2::TokenStream> {
    let name = item.ident.clone();
    let kind_name = format_ident!("{}Kind", name);
    let visibility = item.vis.clone();
    let generics = item.generics.clone();
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let kind_derives = take_derives(&mut item.attrs)?;
    let item_attributes = item.attrs.clone();
    let wrapper_derives = kind_derives
        .iter()
        .filter(|path| derive_is(path, "Clone") || derive_is(path, "Copy"))
        .collect::<Vec<_>>();
    let wrapper_derive_attribute =
        (!wrapper_derives.is_empty()).then(|| quote!(#[derive(#(#wrapper_derives),*)]));
    let kind_derive_attribute =
        (!kind_derives.is_empty()).then(|| quote!(#[derive(#(#kind_derives),*)]));
    let serde_wrapper_derive = serialize.then(|| quote!(#[derive(::inception::serde::Serialize)]));
    let serde_kind_derive = serialize.then(|| quote!(#[derive(::inception::serde::Serialize)]));
    let semantic_impls = semantic_trait_impls(&name, &generics, &kind_derives);

    let mut descriptions = Vec::new();
    let mut codes = Vec::new();
    let mut patterns = Vec::new();
    let mut standard_sources = Vec::new();
    let mut nested_errors = Vec::new();
    let mut constructors = Vec::new();
    let mut locate_err_methods = Vec::new();
    let mut builder_types = Vec::new();
    let mut builder_methods = Vec::new();
    let mut context_arms = Vec::new();
    let mut kind_debug_arms = Vec::new();
    let mut from_impls = Vec::new();

    for variant in &mut item.variants {
        let variant_name = variant.ident.clone();
        let code = take_code(&mut variant.attrs)?
            .unwrap_or_else(|| LitStr::new(&format!("{name}.{variant_name}"), variant_name.span()));
        let description = take_description(&mut variant.attrs)?
            .unwrap_or_else(|| LitStr::new(&variant_name.to_string(), variant_name.span()));

        let mut source_member = None;
        let mut source_is_semantic = false;
        let mut source_is_boxed = false;
        let mut from_source_type = None;
        let mut context_members = Vec::new();
        for (index, field) in variant.fields.iter_mut().enumerate() {
            let error_kind = take_cause_marker(&mut field.attrs)?;
            let nested_error = error_kind == Some(true);
            let external_source = error_kind == Some(false);
            let from_source = take_marker(&mut field.attrs, "from")?;
            let hidden = take_marker(&mut field.attrs, "hide")?;
            if hidden && (nested_error || external_source) {
                return Err(Error::new_spanned(field, "cause fields cannot be #[hide]"));
            }
            if from_source && !(nested_error || external_source) {
                return Err(Error::new_spanned(
                    field,
                    "#[from] requires #[caused_by] or #[caused_by(inception)]",
                ));
            }
            if from_source {
                from_source_type = Some(field.ty.clone());
            }
            let member = field
                .ident
                .clone()
                .map_or_else(|| Member::Unnamed(index.into()), Member::Named);
            if !nested_error && !external_source {
                context_members.push((member.clone(), hidden));
            }
            if nested_error || external_source {
                if source_member.is_some() {
                    return Err(Error::new_spanned(
                        field,
                        "an inception variant may have only one error",
                    ));
                }
                source_member = Some(member);
                source_is_semantic = nested_error;
                source_is_boxed = type_is_box(&field.ty);
            }
        }

        if from_source_type.is_some() && fields_len(&variant.fields) != 1 {
            return Err(Error::new_spanned(
                &variant.fields,
                "a #[from] variant must contain only its cause field",
            ));
        }

        let pattern = variant_pattern(&kind_name, &variant_name, &variant.fields);
        let source_pattern = source_member.as_ref().map(|member| {
            variant_source_pattern(&kind_name, &variant_name, &variant.fields, member)
        });
        let constructor = constructor_method(&kind_name, &variant_name, &variant.fields);
        if source_member.is_some() && fields_len(&variant.fields) == 1 {
            locate_err_methods.push(locate_err_method(
                &kind_name,
                &variant_name,
                &variant.fields,
            ));
        }
        if let Some((builder_type, builder_method)) = builder_for_named_variant(
            &name,
            &visibility,
            &generics,
            &variant_name,
            &variant.fields,
        ) {
            builder_types.push(builder_type);
            builder_methods.push(builder_method);
        }

        descriptions.push(description);
        codes.push(code);
        patterns.push(pattern.clone());
        constructors.push(constructor);
        if let Some(source_type) = from_source_type {
            let method = constructor_name(&variant_name);
            from_impls.push(quote! {
                impl #impl_generics ::core::convert::From<#source_type>
                    for #name #type_generics #where_clause
                {
                    #[track_caller]
                    fn from(error: #source_type) -> Self {
                        Self::#method(error)
                    }
                }
            });
        }
        context_arms.push(variant_context_entries(
            &kind_name,
            &variant_name,
            &variant.fields,
            &context_members,
        ));
        kind_debug_arms.push(quote! {
            #pattern => formatter.write_str(concat!(stringify!(#kind_name), "::", stringify!(#variant_name)))
        });
        standard_sources.push(match &source_pattern {
            Some(pattern) if source_is_boxed => quote!(
                #pattern => Some(source.as_ref() as &(dyn ::core::error::Error + 'static))
            ),
            Some(pattern) => quote!(
                #pattern => Some(::inception::external_source(source))
            ),
            None => quote!(#pattern => None),
        });
        nested_errors.push(if source_is_semantic {
            let pattern = source_pattern.expect("locate source has a member");
            if source_is_boxed {
                quote!(#pattern => Some(source.as_ref() as &(dyn ::inception::InceptionError + 'static)))
            } else {
                quote!(#pattern => Some(source as &(dyn ::inception::InceptionError + 'static)))
            }
        } else {
            quote!(#pattern => None)
        });
    }

    let variants = item.variants;
    let description_arms = patterns
        .iter()
        .zip(&descriptions)
        .map(|(pattern, description)| quote!(#pattern => #description));
    let code_arms = patterns
        .iter()
        .zip(&codes)
        .map(|(pattern, code)| quote!(#pattern => #code));
    let catalog_entries = codes
        .iter()
        .zip(&descriptions)
        .map(|(code, description)| quote!(::inception::ErrorDescriptor::new(#code, #description)));

    Ok(quote! {
        #(#item_attributes)*
        #wrapper_derive_attribute
        #serde_wrapper_derive
        #visibility struct #name #generics {
            error: #kind_name #type_generics,
            location: ::inception::Location,
        }

        #kind_derive_attribute
        #serde_kind_derive
        #visibility enum #kind_name #generics {
            #variants
        }

        impl #impl_generics ::core::fmt::Debug for #kind_name #type_generics #where_clause {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    #(#kind_debug_arms),*
                }
            }
        }

        #semantic_impls

        #(#from_impls)*

        impl #impl_generics #name #type_generics #where_clause {
            /// Stable catalog used by API schema generation and rename checks.
            pub const CATALOG: &'static [::inception::ErrorDescriptor] = &[
                #(#catalog_entries),*
            ];

            #[doc(hidden)]
            #[must_use]
            ///
            /// # Safety
            ///
            /// Callers must pass the source coordinate at which this semantic
            /// error layer is actually created. Prefer generated constructors
            /// or `locate!`, which enforce that invariant automatically.
            pub unsafe fn __inception_new_at(
                error: #kind_name #type_generics,
                location: ::inception::Location,
            ) -> Self {
                Self { error, location }
            }

            #[must_use]
            pub fn kind(&self) -> &#kind_name #type_generics {
                &self.error
            }

            #[must_use]
            pub fn into_kind(self) -> #kind_name #type_generics {
                self.error
            }

            #[must_use]
            pub fn code(&self) -> &'static str {
                ::inception::InceptionError::code(self)
            }

            #[must_use]
            pub fn description(&self) -> &'static str {
                ::inception::InceptionError::description(self)
            }

            #[must_use]
            pub fn created_at(&self) -> ::inception::Location {
                ::inception::InceptionError::created_at(self)
            }

            #[must_use]
            pub fn trace(&self) -> ::inception::Trace<'_> {
                ::inception::InceptionError::trace(self)
            }

            #[track_caller]
            #[must_use]
            pub fn locate(self) -> Self {
                // SAFETY: track_caller supplies this method's actual caller.
                unsafe {
                    Self::__inception_new_at(
                        self.error,
                        ::inception::Location::from_location(::core::panic::Location::caller()),
                    )
                }
            }

            #[must_use]
            pub fn entries(&self) -> ::inception::__alloc::vec::Vec<::inception::Entry> {
                ::inception::InceptionError::entries(self)
            }

            #(#builder_methods)*
            #(#constructors)*
            #(#locate_err_methods)*
        }

        impl #impl_generics ::core::fmt::Display for #name #type_generics #where_clause {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter.write_str(::inception::InceptionError::description(self))
            }
        }

        impl #impl_generics ::core::fmt::Debug for #name #type_generics #where_clause {
            fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                formatter
                    .debug_struct(stringify!(#name))
                    .field("code", &self.code())
                    .field("description", &self.description())
                    .field("location", &self.location)
                    .finish_non_exhaustive()
            }
        }

        impl #impl_generics ::core::error::Error for #name #type_generics #where_clause {
            fn source(&self) -> Option<&(dyn ::core::error::Error + 'static)> {
                match &self.error {
                    #(#standard_sources),*
                }
            }
        }

        impl #impl_generics ::inception::InceptionError for #name #type_generics #where_clause {
            fn code(&self) -> &'static str {
                match &self.error {
                    #(#code_arms),*
                }
            }

            fn description(&self) -> &'static str {
                match &self.error {
                    #(#description_arms),*
                }
            }

            fn created_at(&self) -> ::inception::Location {
                self.location
            }

            fn nested_error(&self) -> Option<&(dyn ::inception::InceptionError + 'static)> {
                match &self.error {
                    #(#nested_errors),*
                }
            }

            fn entries(&self) -> ::inception::__alloc::vec::Vec<::inception::Entry> {
                match &self.error {
                    #(#context_arms),*
                }
            }
        }

        #(#builder_types)*
    })
}

fn take_derives(attributes: &mut Vec<Attribute>) -> Result<Vec<Path>> {
    let mut derives = Vec::new();
    let mut retained = Vec::with_capacity(attributes.len());
    for attribute in attributes.drain(..) {
        if attribute.path().is_ident("derive") {
            let parser = Punctuated::<Path, Token![,]>::parse_terminated;
            for path in parser.parse2(attribute.meta.require_list()?.tokens.clone())? {
                if path
                    .segments
                    .last()
                    .is_none_or(|segment| segment.ident != "Debug")
                {
                    derives.push(path);
                }
            }
        } else {
            retained.push(attribute);
        }
    }
    *attributes = retained;
    Ok(derives)
}

fn derive_is(path: &Path, expected: &str) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == expected)
}

fn semantic_trait_impls(
    name: &Ident,
    generics: &syn::Generics,
    derives: &[Path],
) -> proc_macro2::TokenStream {
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let partial_eq = derives
        .iter()
        .any(|path| derive_is(path, "PartialEq"))
        .then(|| {
            quote! {
                impl #impl_generics ::core::cmp::PartialEq for #name #type_generics #where_clause {
                    fn eq(&self, other: &Self) -> bool {
                        self.error == other.error
                    }
                }
            }
        });
    let eq = derives.iter().any(|path| derive_is(path, "Eq")).then(|| {
        quote! {
            impl #impl_generics ::core::cmp::Eq for #name #type_generics #where_clause {}
        }
    });
    let partial_ord = derives
        .iter()
        .any(|path| derive_is(path, "PartialOrd"))
        .then(|| {
            quote! {
                impl #impl_generics ::core::cmp::PartialOrd for #name #type_generics #where_clause {
                    fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
                        self.error.partial_cmp(&other.error)
                    }
                }
            }
        });
    let ord = derives.iter().any(|path| derive_is(path, "Ord")).then(|| {
        quote! {
            impl #impl_generics ::core::cmp::Ord for #name #type_generics #where_clause {
                fn cmp(&self, other: &Self) -> ::core::cmp::Ordering {
                    self.error.cmp(&other.error)
                }
            }
        }
    });
    let hash = derives.iter().any(|path| derive_is(path, "Hash")).then(|| {
        quote! {
            impl #impl_generics ::core::hash::Hash for #name #type_generics #where_clause {
                fn hash<H: ::core::hash::Hasher>(&self, state: &mut H) {
                    ::core::hash::Hash::hash(&self.error, state);
                }
            }
        }
    });
    quote!(#partial_eq #eq #partial_ord #ord #hash)
}

fn take_description(attributes: &mut Vec<Attribute>) -> Result<Option<LitStr>> {
    let mut description = None;
    let mut retained = Vec::with_capacity(attributes.len());
    for attribute in attributes.drain(..) {
        if attribute.path().is_ident("description") {
            if description.is_some() {
                return Err(Error::new_spanned(
                    attribute,
                    "duplicate #[description(...)]",
                ));
            }
            description = Some(attribute.parse_args::<LitStr>()?);
        } else {
            retained.push(attribute);
        }
    }
    *attributes = retained;
    Ok(description)
}

fn take_code(attributes: &mut Vec<Attribute>) -> Result<Option<LitStr>> {
    let mut code = None;
    let mut retained = Vec::with_capacity(attributes.len());
    for attribute in attributes.drain(..) {
        if attribute.path().is_ident("code") {
            if code.is_some() {
                return Err(Error::new_spanned(attribute, "duplicate #[code(...)]"));
            }
            let value = attribute.parse_args::<LitStr>()?;
            if value.value().is_empty() {
                return Err(Error::new_spanned(value, "error code cannot be empty"));
            }
            code = Some(value);
        } else {
            retained.push(attribute);
        }
    }
    *attributes = retained;
    Ok(code)
}

fn take_marker(attributes: &mut Vec<Attribute>, name: &str) -> Result<bool> {
    let mut found = false;
    let mut retained = Vec::with_capacity(attributes.len());
    for attribute in attributes.drain(..) {
        if attribute.path().is_ident(name) {
            if found {
                return Err(Error::new_spanned(
                    attribute,
                    format!("duplicate #[{name}]"),
                ));
            }
            if !matches!(attribute.meta, syn::Meta::Path(_)) {
                return Err(Error::new_spanned(
                    attribute,
                    format!("#[{name}] does not take arguments"),
                ));
            }
            found = true;
        } else {
            retained.push(attribute);
        }
    }
    *attributes = retained;
    Ok(found)
}

fn take_cause_marker(attributes: &mut Vec<Attribute>) -> Result<Option<bool>> {
    let mut kind = None;
    let mut retained = Vec::with_capacity(attributes.len());
    for attribute in attributes.drain(..) {
        if attribute.path().is_ident("caused_by") {
            if kind.is_some() {
                return Err(Error::new_spanned(attribute, "duplicate #[caused_by(...)]"));
            }
            kind = Some(match attribute.meta {
                syn::Meta::Path(_) => false,
                syn::Meta::List(_) => {
                    let argument = attribute.parse_args::<Ident>()?;
                    if argument == "inception" {
                        true
                    } else {
                        return Err(Error::new_spanned(
                            argument,
                            "cause kind must be `inception`",
                        ));
                    }
                }
                syn::Meta::NameValue(_) => {
                    return Err(Error::new_spanned(
                        attribute,
                        "use #[caused_by] or #[caused_by(inception)]",
                    ));
                }
            });
        } else {
            retained.push(attribute);
        }
    }
    *attributes = retained;
    Ok(kind)
}

fn variant_pattern(kind: &Ident, variant: &Ident, fields: &Fields) -> proc_macro2::TokenStream {
    match fields {
        Fields::Named(_) => quote!(#kind::#variant { .. }),
        Fields::Unnamed(_) => quote!(#kind::#variant(..)),
        Fields::Unit => quote!(#kind::#variant),
    }
}

fn variant_source_pattern(
    kind: &Ident,
    variant: &Ident,
    fields: &Fields,
    source: &Member,
) -> proc_macro2::TokenStream {
    match (fields, source) {
        (Fields::Named(_), Member::Named(source)) => {
            quote!(#kind::#variant { #source: source, .. })
        }
        (Fields::Unnamed(fields), Member::Unnamed(source)) => {
            let bindings = fields.unnamed.iter().enumerate().map(|(index, _)| {
                if index == source.index as usize {
                    quote!(source)
                } else {
                    quote!(_)
                }
            });
            quote!(#kind::#variant(#(#bindings),*))
        }
        _ => unreachable!("source member matches variant fields"),
    }
}

fn variant_context_entries(
    kind: &Ident,
    variant: &Ident,
    fields: &Fields,
    context_members: &[(Member, bool)],
) -> proc_macro2::TokenStream {
    if context_members.is_empty() {
        let pattern = variant_pattern(kind, variant, fields);
        return quote!(#pattern => ::inception::__alloc::vec::Vec::new());
    }

    let bindings = context_members
        .iter()
        .enumerate()
        .map(|(index, _)| format_ident!("_inception_context_{index}"))
        .collect::<Vec<_>>();

    let pattern = match fields {
        Fields::Named(_) => {
            let members = context_members.iter().map(|(member, _)| match member {
                Member::Named(name) => name,
                Member::Unnamed(_) => unreachable!("named context member"),
            });
            quote!(#kind::#variant { #(#members: #bindings),*, .. })
        }
        Fields::Unnamed(fields) => {
            let values = fields.unnamed.iter().enumerate().map(|(field_index, _)| {
                context_members
                    .iter()
                    .position(|(member, _)| {
                        matches!(member, Member::Unnamed(index) if index.index as usize == field_index)
                    })
                    .map_or_else(|| quote!(_), |position| {
                        let binding = &bindings[position];
                        quote!(#binding)
                    })
            });
            quote!(#kind::#variant(#(#values),*))
        }
        Fields::Unit => unreachable!("unit variants cannot contain context fields"),
    };

    let entries = context_members
        .iter()
        .zip(&bindings)
        .map(|((member, hidden), binding)| {
            let field_name = match member {
                Member::Named(name) => name.to_string(),
                Member::Unnamed(index) => index.index.to_string(),
            };
            match hidden {
                false => quote!(::inception::Entry::display(#field_name, #binding)),
                true => quote!(::inception::Entry::hide(#field_name)),
            }
        });

    quote!(#pattern => ::inception::__alloc::vec![#(#entries),*])
}

fn fields_len(fields: &Fields) -> usize {
    match fields {
        Fields::Named(fields) => fields.named.len(),
        Fields::Unnamed(fields) => fields.unnamed.len(),
        Fields::Unit => 0,
    }
}

fn type_is_box(ty: &syn::Type) -> bool {
    matches!(
        ty,
        syn::Type::Path(path)
            if path.qself.is_none()
                && path.path.segments.last().is_some_and(|segment| segment.ident == "Box")
    )
}

fn constructor_name(variant: &Ident) -> Ident {
    Ident::new(&to_snake_case(&variant.to_string()), variant.span())
}

fn locate_err_method_name(variant: &Ident) -> Ident {
    format_ident!("__inception_locate_err_{}", constructor_name(variant))
}

fn locate_err_method(kind: &Ident, variant: &Ident, fields: &Fields) -> proc_macro2::TokenStream {
    let method = locate_err_method_name(variant);
    match fields {
        Fields::Named(fields) => {
            let field = fields.named.first().expect("one source field");
            let name = field.ident.as_ref().expect("named field");
            let ty = &field.ty;
            quote! {
                #[doc(hidden)]
                #[must_use]
                pub fn #method(source: #ty, location: ::inception::Location) -> Self {
                    // SAFETY: locate_err! captures its invocation's source coordinate.
                    unsafe { Self::__inception_new_at(#kind::#variant { #name: source }, location) }
                }
            }
        }
        Fields::Unnamed(fields) => {
            let field = fields.unnamed.first().expect("one source field");
            let ty = &field.ty;
            quote! {
                #[doc(hidden)]
                #[must_use]
                pub fn #method(source: #ty, location: ::inception::Location) -> Self {
                    // SAFETY: locate_err! captures its invocation's source coordinate.
                    unsafe { Self::__inception_new_at(#kind::#variant(source), location) }
                }
            }
        }
        Fields::Unit => unreachable!("a source field cannot be unit"),
    }
}

fn constructor_method(kind: &Ident, variant: &Ident, fields: &Fields) -> proc_macro2::TokenStream {
    let method = constructor_name(variant);
    match fields {
        Fields::Unit => quote! {
            #[track_caller]
            #[must_use]
            pub fn #method() -> Self {
                // SAFETY: track_caller supplies this constructor's actual caller.
                unsafe {
                    Self::__inception_new_at(
                        #kind::#variant,
                        ::inception::Location::from_location(::core::panic::Location::caller()),
                    )
                }
            }
        },
        Fields::Named(fields) => {
            let parameters = fields.named.iter().map(|field| {
                let name = field.ident.as_ref().expect("named field");
                let ty = &field.ty;
                quote!(#name: #ty)
            });
            let names = fields
                .named
                .iter()
                .map(|field| field.ident.as_ref().expect("named field"));
            quote! {
                #[track_caller]
                #[must_use]
                pub fn #method(#(#parameters),*) -> Self {
                    // SAFETY: track_caller supplies this constructor's actual caller.
                    unsafe {
                        Self::__inception_new_at(
                            #kind::#variant { #(#names),* },
                            ::inception::Location::from_location(::core::panic::Location::caller()),
                        )
                    }
                }
            }
        }
        Fields::Unnamed(fields) => {
            let parameters = fields.unnamed.iter().enumerate().map(|(index, field)| {
                let name = format_ident!("field_{index}");
                let ty = &field.ty;
                quote!(#name: #ty)
            });
            let names = fields
                .unnamed
                .iter()
                .enumerate()
                .map(|(index, _)| format_ident!("field_{index}"));
            quote! {
                #[track_caller]
                #[must_use]
                pub fn #method(#(#parameters),*) -> Self {
                    // SAFETY: track_caller supplies this constructor's actual caller.
                    unsafe {
                        Self::__inception_new_at(
                            #kind::#variant(#(#names),*),
                            ::inception::Location::from_location(::core::panic::Location::caller()),
                        )
                    }
                }
            }
        }
    }
}

fn builder_for_named_variant(
    name: &Ident,
    visibility: &syn::Visibility,
    generics: &syn::Generics,
    variant: &Ident,
    fields: &Fields,
) -> Option<(proc_macro2::TokenStream, proc_macro2::TokenStream)> {
    let Fields::Named(fields) = fields else {
        return None;
    };
    let builder_name = format_ident!("{name}{variant}Builder");
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let field_definitions = fields.named.iter().map(|field| {
        let field_name = field.ident.as_ref().expect("named field");
        let field_type = &field.ty;
        quote!(#field_name: ::core::option::Option<#field_type>)
    });
    let initial_fields = fields.named.iter().map(|field| {
        let field_name = field.ident.as_ref().expect("named field");
        quote!(#field_name: ::core::option::Option::None)
    });
    let setters = fields.named.iter().map(|field| {
        let field_name = field.ident.as_ref().expect("named field");
        let field_type = &field.ty;
        quote! {
            #[must_use]
            pub fn #field_name(mut self, value: #field_type) -> Self {
                self.#field_name = Some(value);
                self
            }
        }
    });
    let values = fields.named.iter().map(|field| {
        let field_name = field.ident.as_ref().expect("named field");
        quote!(self.#field_name.ok_or_else(|| ::inception::BuildError::missing(stringify!(#field_name)))?)
    });
    let method = constructor_name(variant);
    let builder_method = format_ident!("{}_builder", method);
    let builder_type = quote! {
        #visibility struct #builder_name #generics {
            #(#field_definitions,)*
        }

        impl #impl_generics #builder_name #type_generics #where_clause {
            #(#setters)*

            #[must_use]
            pub fn build(self) -> ::core::result::Result<#name #type_generics, ::inception::BuildError> {
                Ok(#name::#method(#(#values),*))
            }
        }
    };
    let builder_method = quote! {
        #[must_use]
        pub fn #builder_method() -> #builder_name #type_generics {
            #builder_name {
                #(#initial_fields,)*
            }
        }
    };
    Some((builder_type, builder_method))
}

fn expand_constructor(expression: Expr) -> Result<proc_macro2::TokenStream> {
    if matches!(&expression, Expr::Struct(_))
        || matches!(&expression, Expr::Path(path) if path.path.segments.len() >= 2)
    {
        let (error_path, kind_expression) = rewrite_as_kind(expression)?;
        return Ok(quote! {
            // SAFETY: the macro captures the invocation's source coordinate.
            unsafe {
                #error_path::__inception_new_at(
                    #kind_expression,
                    ::inception::Location::new(::core::file!(), ::core::line!(), ::core::column!()),
                )
            }
        });
    }

    Ok(quote! {
        ::inception::Inceptioned::new_at(
            #expression,
            ::inception::Location::new(::core::file!(), ::core::line!(), ::core::column!()),
        )
    })
}

fn rewrite_as_kind(expression: Expr) -> Result<(Path, Expr)> {
    match expression {
        Expr::Path(mut expression) => {
            let error_path = error_path(&expression.path)?;
            rewrite_variant_path(&mut expression.path)?;
            Ok((error_path, Expr::Path(expression)))
        }
        Expr::Struct(mut expression) => {
            let error_path = error_path(&expression.path)?;
            rewrite_variant_path(&mut expression.path)?;
            Ok((error_path, Expr::Struct(expression)))
        }
        Expr::Call(mut expression) => {
            let function = match expression.func.as_mut() {
                Expr::Path(function) => function,
                other => {
                    return Err(Error::new_spanned(
                        other,
                        "locate! expects an enum variant constructor",
                    ));
                }
            };
            let error_path = error_path(&function.path)?;
            rewrite_variant_path(&mut function.path)?;
            Ok((error_path, Expr::Call(expression)))
        }
        other => Err(Error::new_spanned(
            other,
            "locate! expects `Error::Variant`, `Error::Variant(...)`, or `Error::Variant { ... }`",
        )),
    }
}

fn error_path(variant_path: &Path) -> Result<Path> {
    if variant_path.segments.len() < 2 {
        return Err(Error::new_spanned(
            variant_path,
            "locate! requires a qualified `Error::Variant` path",
        ));
    }
    let mut path = variant_path.clone();
    path.segments.pop();
    path.segments.pop_punct();
    Ok(path)
}

fn rewrite_variant_path(path: &mut Path) -> Result<()> {
    if path.segments.len() < 2 {
        return Err(Error::new_spanned(path, "expected `Error::Variant`"));
    }
    let variant = path.segments.pop().expect("checked length").into_value();
    let error = path.segments.last_mut().expect("checked length");
    error.ident = format_ident!("{}Kind", error.ident);
    path.segments.push(variant);
    Ok(())
}

fn to_snake_case(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 4);
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}
